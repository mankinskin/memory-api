//! Pure MCP message interception logic for the cost-gate middleware.
//!
//! These functions operate on parsed JSON-RPC messages so they can be unit
//! tested without spawning processes. The wiring in `main.rs` reads/writes
//! newline-delimited JSON on stdio and calls into here.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::{
    Value,
    json,
};

use toolmon_policy_api::{CALLER_MODEL_ARG, Decision, Policy, inject_caller_model_schema};

/// Optional grant id argument for budget offset.
pub const GRANT_ID_ARG: &str = "grant_id";

/// What the proxy should do with a client→server message.
#[derive(Debug)]
pub enum ClientAction {
    /// Forward this (possibly rewritten) message to the real server.
    Forward(Value),
    /// Do not forward; send this response straight back to the client.
    Respond(Value),
}

/// Track which JSON-RPC ids were `tools/list` requests, so their responses can
/// be schema-augmented on the way back.
#[derive(Default)]
pub struct PendingList {
    ids: HashSet<String>,
}

impl PendingList {
    pub fn record(&mut self, id: &Value) {
        self.ids.insert(id_key(id));
    }

    pub fn take(&mut self, id: &Value) -> bool {
        self.ids.remove(&id_key(id))
    }
}

fn id_key(id: &Value) -> String {
    serde_json::to_string(id).unwrap_or_default()
}

/// Payload telemetry for an MCP tool call (ticket 9d527ad1).
///
/// `tokens_estimated` is a rough chars/4 estimate over the combined
/// request+response payloads — never an observed token count, and never a
/// dollar cost (tools have no dollar cost; see spec 7be68a48 R4).
///
/// Coverage is intentionally partial: this proxy only measures MCP
/// `tools/call` traffic that traverses this middleware.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallTelemetry {
    pub timestamp: String,
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<String>,
    pub decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_chars: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub response_chars: Option<u64>,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens_estimated: Option<u64>,
}

/// A `tools/call` forwarded to the real server, awaiting its response.
///
/// Captured at the moment of forwarding so `handle_server_message` can
/// compute `duration_ms` and emit a `CallTelemetry` once the matching
/// response arrives (correlated by JSON-RPC id).
#[derive(Debug, Clone)]
pub struct PendingCall {
    pub tool_name: String,
    pub caller_model: Option<String>,
    pub grant_id: Option<String>,
    pub decision: String,
    pub request_bytes: u64,
    pub request_chars: u64,
    pub started_at: std::time::Instant,
    /// Soft warning to surface on the eventual server response when the
    /// `caller_model` only resolved after fallback normalization.
    pub warning: Option<String>,
}

/// Tracks in-flight forwarded `tools/call` requests by JSON-RPC id.
#[derive(Default)]
pub struct PendingCalls {
    calls: std::collections::HashMap<String, PendingCall>,
}

impl PendingCalls {
    pub fn record(&mut self, id: &Value, call: PendingCall) {
        self.calls.insert(id_key(id), call);
    }

    pub fn take(&mut self, id: &Value) -> Option<PendingCall> {
        self.calls.remove(&id_key(id))
    }
}

fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Fallback normalization for `caller_model` strings, applied only when the
/// raw value fails the gate's exact/substring resolution. Strips a trailing
/// parenthetical client qualifier (e.g. `"Claude Sonnet 5 (copilot)"` ->
/// `"Claude Sonnet 5"`), then folds spaces and underscores to hyphens, then
/// lowercases. No fuzzy or edit-distance matching.
pub fn normalize_caller_model(model: &str) -> String {
    let trimmed = model.trim();
    let stripped = if trimmed.ends_with(')') {
        trimmed
            .rfind('(')
            .map(|idx| trimmed[..idx].trim_end())
            .unwrap_or(trimmed)
    } else {
        trimmed
    };
    stripped
        .chars()
        .map(|c| if c == ' ' || c == '_' { '-' } else { c })
        .collect::<String>()
        .to_lowercase()
}

/// Compute payload size and estimated tokens from a JSON value.
pub fn compute_payload_telemetry(value: &Value) -> (u64, u64, u64) {
    let json_str = serde_json::to_string(value).unwrap_or_default();
    let bytes = json_str.as_bytes().len() as u64;
    let chars = json_str.chars().count() as u64;
    let tokens_estimated = chars / 4; // chars/4 divisor per ticket spec
    (bytes, chars, tokens_estimated)
}

/// Build a `tools/call` result carrying an error message (isError=true).
fn error_result(id: &Value, text: &str) -> Value {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": {
            "content": [{ "type": "text", "text": text }],
            "isError": true
        }
    })
}

/// Handle a client→server message.
///
/// * `tools/list` requests are recorded and forwarded.
/// * `tools/call` requests are gated: a missing `caller_model` is rejected; a
///   delegate decision is refused with guidance; an allow strips `caller_model`
///   and forwards the cleaned call.
/// * `caller_model` is resolved as-is first (exact match, then substring,
///   unchanged precedence). Only if that fails is a normalized candidate
///   tried as a fallback (see [`normalize_caller_model`]); a match there
///   still allows the call but attaches a `costGateWarning` to the eventual
///   response instead of rejecting.
/// * Everything else is forwarded unchanged.
///
/// When `gate` is `None` (fail-open, e.g. price table missing) the message is
/// forwarded unchanged.
pub fn handle_client_message(
    mut msg: Value,
    policy: Option<&dyn Policy>,
    pending: &mut PendingList,
    pending_calls: &mut PendingCalls,
) -> (ClientAction, Option<CallTelemetry>) {
    let Some(policy) = policy else {
        return (ClientAction::Forward(msg), None);
    };

    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "tools/list" => {
            if let Some(id) = msg.get("id") {
                pending.record(id);
            }
            (ClientAction::Forward(msg), None)
        }
        "tools/call" => {
            let id = msg.get("id").cloned().unwrap_or(Value::Null);
            let params = msg.get("params").cloned().unwrap_or(Value::Null);
            let tool = params
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            let caller_model = params
                .get("arguments")
                .and_then(|a| a.get(CALLER_MODEL_ARG))
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_string();
            let grant_id = params
                .get("arguments")
                .and_then(|a| a.get(GRANT_ID_ARG))
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string());
            let (request_bytes, request_chars, _) = compute_payload_telemetry(&msg);

            // Build an immediate (non-forwarded) telemetry record: nothing was
            // sent to the server, so response counts are zero and duration_ms
            // is zero (no wall-clock span to measure).
            let immediate_telemetry = |decision: &str, caller_model: Option<String>| CallTelemetry {
                timestamp: now_rfc3339(),
                tool_name: tool.clone(),
                caller_model,
                grant_id: grant_id.clone(),
                decision: decision.to_string(),
                request_bytes: Some(request_bytes),
                request_chars: Some(request_chars),
                response_bytes: Some(0),
                response_chars: Some(0),
                duration_ms: 0,
                tokens_estimated: Some(request_chars / 4),
            };

            if caller_model.is_empty() {
                let telemetry = immediate_telemetry("reject-missing-model", None);
                return (
                    ClientAction::Respond(error_result(
                        &id,
                        &format!(
                            "Missing required '{CALLER_MODEL_ARG}' argument. Every tool \
                             call must declare the id of the model issuing it (e.g. \
                             claude-opus-4-8) so price-awareness enforcement can run."
                        ),
                    )),
                    Some(telemetry),
                );
            }

            // Resolve the raw caller_model first (exact -> substring, unchanged
            // precedence). Only when that fails do we retry with a normalized
            // candidate (trailing client qualifier stripped; separators
            // folded to hyphens) as a fallback, never in place of it.
            let mut effective_model = caller_model.clone();
            let mut soft_warning: Option<String> = None;
            if !policy.resolves(&caller_model) {
                let normalized = normalize_caller_model(&caller_model);
                if normalized != caller_model && policy.resolves(&normalized) {
                    soft_warning = Some(format!(
                        "caller_model '{caller_model}' did not match the price table \
                         exactly; normalized to '{normalized}' (stripped trailing client \
                         qualifier and/or folded separators to hyphens) and resolved from \
                         there. Pass the exact price-table model_id to avoid this warning."
                    ));
                    effective_model = normalized;
                }
            }

            match policy.evaluate(&effective_model, &tool, grant_id.as_deref()) {
                Decision::Reject { guidance } => {
                    let telemetry = immediate_telemetry("reject", Some(caller_model));
                    (ClientAction::Respond(error_result(&id, &guidance)), Some(telemetry))
                }
                Decision::Delegate { guidance } => {
                    let telemetry = immediate_telemetry("delegate", Some(caller_model));
                    (ClientAction::Respond(error_result(&id, &guidance)), Some(telemetry))
                }
                Decision::Allow => {
                    // Strip caller_model and grant_id before forwarding to the real server.
                    if let Some(args) = msg
                        .get_mut("params")
                        .and_then(|p| p.get_mut("arguments"))
                        .and_then(Value::as_object_mut)
                    {
                        args.remove(CALLER_MODEL_ARG);
                        args.remove(GRANT_ID_ARG);
                    }
                    let decision_label = if soft_warning.is_some() { "allow-normalized" } else { "allow" };
                    pending_calls.record(
                        &id,
                        PendingCall {
                            tool_name: tool,
                            caller_model: Some(caller_model),
                            grant_id,
                            decision: decision_label.to_string(),
                            request_bytes,
                            request_chars,
                            started_at: std::time::Instant::now(),
                            warning: soft_warning,
                        },
                    );
                    (ClientAction::Forward(msg), None)
                }
            }
        }
        _ => (ClientAction::Forward(msg), None),
    }
}

/// Handle a server→client message: if it is the response to a recorded
/// `tools/list` request, inject a required `caller_model` argument into every
/// advertised tool's `inputSchema`. Otherwise pass through unchanged.
pub fn handle_server_message(
    mut msg: Value,
    policy: Option<&dyn Policy>,
    pending: &mut PendingList,
    pending_calls: &mut PendingCalls,
) -> (Value, Option<CallTelemetry>) {
    let mut warning_to_inject: Option<String> = None;
    let telemetry = msg.get("id").and_then(|id| pending_calls.take(id)).map(|call| {
        let (response_bytes, response_chars, _) = compute_payload_telemetry(&msg);
        let duration_ms = call.started_at.elapsed().as_millis() as u64;
        let tokens_estimated = (call.request_chars + response_chars) / 4;
        warning_to_inject = call.warning.clone();
        CallTelemetry {
            timestamp: now_rfc3339(),
            tool_name: call.tool_name,
            caller_model: call.caller_model,
            grant_id: call.grant_id,
            decision: call.decision,
            request_bytes: Some(call.request_bytes),
            request_chars: Some(call.request_chars),
            response_bytes: Some(response_bytes),
            response_chars: Some(response_chars),
            duration_ms,
            tokens_estimated: Some(tokens_estimated),
        }
    });

    if let Some(warning) = warning_to_inject {
        if let Some(result) = msg.get_mut("result").and_then(Value::as_object_mut) {
            result.insert("costGateWarning".to_string(), json!(warning));
        }
    }

    let is_list_response = msg
        .get("id")
        .map(|id| pending.take(id))
        .unwrap_or(false)
        && msg
            .get("result")
            .and_then(|r| r.get("tools"))
            .map(Value::is_array)
            .unwrap_or(false);

    if !is_list_response {
        return (msg, telemetry);
    }

    if let Some(tools) = msg
        .get_mut("result")
        .and_then(|r| r.get_mut("tools"))
        .and_then(Value::as_array_mut)
    {
        for tool in tools.iter_mut() {
            match policy {
                Some(p) => p.on_tools_list(tool),
                None => inject_caller_model_schema(tool),
            }
        }
    }
    (msg, telemetry)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use toolmon_costgate::{CostGatePolicy, Gate};

    fn test_gate() -> CostGatePolicy {
        // Write a tiny fixture table to a unique temp file and load it. A
        // per-call counter avoids collisions between parallel tests (same pid).
        use std::sync::atomic::{
            AtomicU64,
            Ordering,
        };
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir();
        let path = dir.join(format!("mcpcg-fixture-{}-{}.json", std::process::id(), n));
        std::fs::write(
            &path,
            r#"{"models":[
                {"provider_id":"anthropic","model_id":"claude-opus-4-1","output_mtok":75.0},
                {"provider_id":"openai","model_id":"gpt-5-mini","output_mtok":2.0}
            ]}"#,
        )
        .unwrap();
        let g = Gate::load(
            Path::new(&path),
            toolmon_costgate::ModelBudgetCalibration::default(),
            None,
            None,
        )
        .unwrap();
        let _ = std::fs::remove_file(&path);
        CostGatePolicy::new(g)
    }

    fn call(tool: &str, model: Option<&str>) -> Value {
        let mut args = serde_json::Map::new();
        if let Some(m) = model {
            args.insert(CALLER_MODEL_ARG.into(), json!(m));
        }
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "tools/call",
            "params": { "name": tool, "arguments": args }
        })
    }

    #[test]
    fn missing_caller_model_is_rejected() {
        let g = test_gate();
        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        match handle_client_message(call("read_file", None), Some(&g), &mut p, &mut pc) {
            (ClientAction::Respond(v), telemetry) => {
                assert_eq!(v["result"]["isError"], json!(true));
                let text = v["result"]["content"][0]["text"].as_str().unwrap();
                assert!(text.contains(CALLER_MODEL_ARG));
                let telemetry = telemetry.expect("expected telemetry for refused call");
                assert_eq!(telemetry.decision, "reject-missing-model");
                assert_eq!(telemetry.duration_ms, 0);
                assert_eq!(telemetry.response_bytes, Some(0));
            }
            other => panic!("expected Respond, got {other:?}"),
        }
    }

    #[test]
    fn expensive_measured_tool_is_refused() {
        // Build a gate with a rollup that measures read_file with cost 75
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir();
        let tid = std::thread::current().id();
        let path = dir.join(format!("mcpcg-fixture-{:?}-{}-{}.json", tid, std::process::id(), n));
        let rollup_path = dir.join(format!("mcpcg-rollup-{:?}-{}-{}.json", tid, std::process::id(), n));
        std::fs::write(
            &path,
            r#"{"models":[
                {"provider_id":"anthropic","model_id":"claude-opus-4-1","output_mtok":75.0},
                {"provider_id":"openai","model_id":"gpt-5-mini","output_mtok":2.0}
            ]}"#,
        )
        .unwrap();
        std::fs::write(
            &rollup_path,
            r#"{"report":{"tools":[
                {"tool_name":"read_file","call_count":10,"cost":75}
            ]}}"#,
        )
        .unwrap();
        let g = CostGatePolicy::new(
            Gate::load(
                std::path::Path::new(&path),
                toolmon_costgate::ModelBudgetCalibration::default(),
                Some(std::path::Path::new(&rollup_path)),
                None,
            )
            .unwrap(),
        );
        // Clean up temp files after loading
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&rollup_path);

        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        match handle_client_message(call("read_file", Some("claude-opus-4-1")), Some(&g), &mut p, &mut pc) {
            (ClientAction::Respond(v), telemetry) => {
                assert_eq!(v["result"]["isError"], json!(true));
                assert!(v["result"]["content"][0]["text"].as_str().unwrap().to_lowercase().contains("delegate"));
                assert_eq!(telemetry.expect("expected telemetry").decision, "delegate");
            }
            other => panic!("expected Respond, got {other:?}"),
        }
    }

    #[test]
    fn unmeasured_tool_fail_open() {
        // Without a rollup, even expensive models can call any tool (fail open)
        let g = test_gate();
        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        match handle_client_message(call("read_file", Some("claude-opus-4-1")), Some(&g), &mut p, &mut pc) {
            (ClientAction::Forward(v), telemetry) => {
                let args = &v["params"]["arguments"];
                assert!(args.get(CALLER_MODEL_ARG).is_none(), "caller_model must be stripped");
                assert!(telemetry.is_none(), "forwarded calls emit telemetry on response, not on forward");
            }
            other => panic!("expected Forward (fail open), got {other:?}"),
        }
    }

    #[test]
    fn cheap_forwards_and_strips_caller_model() {
        let g = test_gate();
        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        // Use a light tool (cost 1) that gpt-5-mini (budget ~97) can afford
        match handle_client_message(call("some_unknown_tool", Some("gpt-5-mini")), Some(&g), &mut p, &mut pc) {
            (ClientAction::Forward(v), _) => {
                let args = &v["params"]["arguments"];
                assert!(args.get(CALLER_MODEL_ARG).is_none(), "caller_model must be stripped");
            }
            other => panic!("expected Forward, got {other:?}"),
        }
    }

    #[test]
    fn expensive_light_tool_forwards() {
        let g = test_gate();
        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        match handle_client_message(call("runSubagent", Some("claude-opus-4-1")), Some(&g), &mut p, &mut pc) {
            (ClientAction::Forward(_), _) => {}
            other => panic!("expected Forward, got {other:?}"),
        }
    }

    #[test]
    fn unknown_caller_model_is_rejected() {
        let g = test_gate();
        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        match handle_client_message(call("read_file", Some("github-copilot")), Some(&g), &mut p, &mut pc) {
            (ClientAction::Respond(v), telemetry) => {
                assert_eq!(v["result"]["isError"], json!(true));
                let text = v["result"]["content"][0]["text"].as_str().unwrap();
                assert!(text.to_lowercase().contains("unknown caller_model"));
                assert_eq!(telemetry.expect("expected telemetry").decision, "reject");
            }
            other => panic!("expected Respond, got {other:?}"),
        }
    }

    #[test]
    fn parenthetical_client_qualifier_is_tolerated() {
        // "gpt-5-mini (copilot)" doesn't match exactly or by substring, but
        // stripping the trailing "(copilot)" qualifier resolves to "gpt-5-mini".
        let g = test_gate();
        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        match handle_client_message(
            call("some_unknown_tool", Some("gpt-5-mini (copilot)")),
            Some(&g),
            &mut p,
            &mut pc,
        ) {
            (ClientAction::Forward(v), _) => {
                let args = &v["params"]["arguments"];
                assert!(args.get(CALLER_MODEL_ARG).is_none(), "caller_model must be stripped");
            }
            other => panic!("expected Forward (allow after normalization), got {other:?}"),
        }

        // The soft warning surfaces on the eventual server response.
        let resp = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": { "content": [{ "type": "text", "text": "ok" }] }
        });
        let (out, telemetry) = handle_server_message(resp, Some(&g), &mut p, &mut pc);
        assert!(out["result"]["costGateWarning"].as_str().unwrap().contains("normalized"));
        assert_eq!(telemetry.unwrap().decision, "allow-normalized");
    }

    #[test]
    fn space_and_underscore_separators_are_normalized() {
        // "Claude_Opus 4 1" doesn't match exactly, but normalizing separators
        // to hyphens and lowercasing resolves to "claude-opus-4-1".
        let g = test_gate();
        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        match handle_client_message(
            call("runSubagent", Some("Claude_Opus 4 1")),
            Some(&g),
            &mut p,
            &mut pc,
        ) {
            (ClientAction::Forward(v), _) => {
                let args = &v["params"]["arguments"];
                assert!(args.get(CALLER_MODEL_ARG).is_none(), "caller_model must be stripped");
            }
            other => panic!("expected Forward (allow after normalization), got {other:?}"),
        }
        let resp = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": { "content": [{ "type": "text", "text": "ok" }] }
        });
        let (out, _) = handle_server_message(resp, Some(&g), &mut p, &mut pc);
        assert!(out["result"]["costGateWarning"].is_string());
    }

    #[test]
    fn genuinely_unknown_model_still_rejected_after_normalization() {
        // Normalizing "Totally Unknown Model (copilot)" still doesn't match
        // anything in the price table, so the call is rejected as before.
        let g = test_gate();
        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        match handle_client_message(
            call("read_file", Some("Totally Unknown Model (copilot)")),
            Some(&g),
            &mut p,
            &mut pc,
        ) {
            (ClientAction::Respond(v), telemetry) => {
                assert_eq!(v["result"]["isError"], json!(true));
                let text = v["result"]["content"][0]["text"].as_str().unwrap();
                assert!(text.to_lowercase().contains("unknown caller_model"));
                assert_eq!(telemetry.expect("expected telemetry").decision, "reject");
            }
            other => panic!("expected Respond, got {other:?}"),
        }
    }

    #[test]
    fn no_gate_is_passthrough() {
        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        match handle_client_message(call("read_file", None), None, &mut p, &mut pc) {
            (ClientAction::Forward(_), None) => {}
            other => panic!("expected Forward with no telemetry, got {other:?}"),
        }
    }

    #[test]
    fn tools_list_response_gets_schema_injected() {
        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        // Record the list request id.
        let req = json!({"jsonrpc":"2.0","id":7,"method":"tools/list","params":{}});
        let g = test_gate();
        let _ = handle_client_message(req, Some(&g), &mut p, &mut pc);

        let resp = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "result": { "tools": [ { "name": "read_file", "inputSchema": { "type": "object", "properties": {}, "required": [] } } ] }
        });
        let (out, telemetry) = handle_server_message(resp, Some(&g), &mut p, &mut pc);
        let tool = &out["result"]["tools"][0];
        assert_eq!(tool["inputSchema"]["properties"][CALLER_MODEL_ARG]["type"], json!("string"));
        let required = tool["inputSchema"]["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == CALLER_MODEL_ARG));
        assert!(telemetry.is_none(), "tools/list response is not a tools/call, no telemetry expected");
    }

    #[test]
    fn inject_creates_schema_when_absent() {
        let mut tool = json!({ "name": "x" });
        inject_caller_model_schema(&mut tool);
        assert_eq!(tool["inputSchema"]["type"], json!("object"));
        assert_eq!(tool["inputSchema"]["required"][0], json!(CALLER_MODEL_ARG));
    }

    #[test]
    fn telemetry_computation_is_monotonic() {
        // AC3: larger payloads yield larger estimates
        let small = json!({"a": 1});
        let medium = json!({"a": 1, "b": "hello", "c": [1,2,3]});
        let large = json!({"a": 1, "b": "hello", "c": [1,2,3], "d": {"nested": "structure with more data"}});

        let (bytes_s, chars_s, tokens_s) = compute_payload_telemetry(&small);
        let (bytes_m, chars_m, tokens_m) = compute_payload_telemetry(&medium);
        let (bytes_l, chars_l, tokens_l) = compute_payload_telemetry(&large);

        assert!(bytes_s < bytes_m && bytes_m < bytes_l, "bytes should be monotonic");
        assert!(chars_s < chars_m && chars_m < chars_l, "chars should be monotonic");
        assert!(tokens_s < tokens_m && tokens_m < tokens_l, "tokens_estimated should be monotonic");
        
        // Verify the chars/4 relationship
        assert_eq!(tokens_s, chars_s / 4);
        assert_eq!(tokens_m, chars_m / 4);
        assert_eq!(tokens_l, chars_l / 4);
    }

    #[test]
    fn telemetry_computation_returns_nonzero() {
        // AC1/AC2: non-empty payloads yield non-zero counts
        let payload = json!({"method": "tools/call", "params": {"name": "read_file", "arguments": {}}});
        let (bytes, chars, tokens) = compute_payload_telemetry(&payload);
        
        assert!(bytes > 0, "bytes should be non-zero for non-empty payload");
        assert!(chars > 0, "chars should be non-zero for non-empty payload");
        assert!(tokens > 0, "tokens_estimated should be non-zero for non-empty payload");
    }

    #[test]
    fn allowed_call_emits_nonzero_tokens_estimated_on_response() {
        // (a) A forwarded (allowed) tools/call correlates its response by
        // JSON-RPC id and records a non-zero tokens_estimated derived from
        // the combined request+response payload.
        let g = test_gate();
        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        let req = call("some_unknown_tool", Some("gpt-5-mini"));
        let (action, telemetry) = handle_client_message(req, Some(&g), &mut p, &mut pc);
        assert!(telemetry.is_none(), "no telemetry until the response arrives");
        let forwarded = match action {
            ClientAction::Forward(v) => v,
            other => panic!("expected Forward, got {other:?}"),
        };
        let id = forwarded["id"].clone();

        let resp = json!({
            "jsonrpc": "2.0",
            "id": id,
            "result": { "content": [{ "type": "text", "text": "some tool output" }] }
        });
        let (_, telemetry) = handle_server_message(resp, Some(&g), &mut p, &mut pc);
        let telemetry = telemetry.expect("expected telemetry once the response is correlated");
        assert_eq!(telemetry.decision, "allow");
        assert_eq!(telemetry.tool_name, "some_unknown_tool");
        assert!(telemetry.response_bytes.unwrap_or(0) > 0, "response_bytes should be non-zero");
        assert!(telemetry.response_chars.unwrap_or(0) > 0, "response_chars should be non-zero");
        assert!(
            telemetry.tokens_estimated.unwrap_or(0) > 0,
            "tokens_estimated should be non-zero for a real intercepted tools/call"
        );
        assert_eq!(
            telemetry.tokens_estimated,
            Some((telemetry.request_chars.unwrap_or(0) + telemetry.response_chars.unwrap_or(0)) / 4)
        );
    }

    #[test]
    fn duration_ms_is_populated_for_forwarded_calls() {
        // (c) duration_ms measures wall-clock from forward to response receipt.
        let g = test_gate();
        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        let req = call("some_unknown_tool", Some("gpt-5-mini"));
        let (action, _) = handle_client_message(req, Some(&g), &mut p, &mut pc);
        let forwarded = match action {
            ClientAction::Forward(v) => v,
            other => panic!("expected Forward, got {other:?}"),
        };
        let id = forwarded["id"].clone();

        // Sleep a measurable span so duration_ms is guaranteed nonzero.
        std::thread::sleep(std::time::Duration::from_millis(5));

        let resp = json!({ "jsonrpc": "2.0", "id": id, "result": { "content": [] } });
        let (_, telemetry) = handle_server_message(resp, Some(&g), &mut p, &mut pc);
        let telemetry = telemetry.expect("expected telemetry");
        assert!(
            telemetry.duration_ms >= 5,
            "duration_ms should reflect the wall-clock span, got {}",
            telemetry.duration_ms
        );
    }

    #[test]
    fn refused_call_records_zero_duration_and_response_counts() {
        // (b/AC4 null-vs-zero): refused calls never reach the server, so
        // response counts and duration_ms are recorded as zero (measured),
        // not omitted — the call itself was still observed.
        let g = test_gate();
        let mut p = PendingList::default();
        let mut pc = PendingCalls::default();
        let (_, telemetry) =
            handle_client_message(call("read_file", None), Some(&g), &mut p, &mut pc);
        let telemetry = telemetry.expect("expected telemetry for the refused call");
        assert_eq!(telemetry.response_bytes, Some(0));
        assert_eq!(telemetry.response_chars, Some(0));
        assert_eq!(telemetry.duration_ms, 0);
        assert!(telemetry.request_chars.unwrap_or(0) > 0);
    }
}
