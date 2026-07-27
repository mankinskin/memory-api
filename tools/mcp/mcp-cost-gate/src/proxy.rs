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

use crate::gate::{
    Decision,
    Gate,
};

/// The argument name injected into every tool schema and required on each call.
pub const CALLER_MODEL_ARG: &str = "caller_model";

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
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallTelemetry {
    pub timestamp: String,
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caller_model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grant_id: Option<String>,
    pub decision: String,
    pub request_bytes: u64,
    pub request_chars: u64,
    pub response_bytes: u64,
    pub response_chars: u64,
    pub tokens_estimated: u64,
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
/// * Everything else is forwarded unchanged.
///
/// When `gate` is `None` (fail-open, e.g. price table missing) the message is
/// forwarded unchanged.
pub fn handle_client_message(
    mut msg: Value,
    gate: Option<&Gate>,
    pending: &mut PendingList,
) -> ClientAction {
    let Some(gate) = gate else {
        return ClientAction::Forward(msg);
    };

    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    match method {
        "tools/list" => {
            if let Some(id) = msg.get("id") {
                pending.record(id);
            }
            ClientAction::Forward(msg)
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

            if caller_model.is_empty() {
                return ClientAction::Respond(error_result(
                    &id,
                    &format!(
                        "Missing required '{CALLER_MODEL_ARG}' argument. Every tool \
                         call must declare the id of the model issuing it (e.g. \
                         claude-opus-4-8) so price-awareness enforcement can run."
                    ),
                ));
            }

            match gate.evaluate(&caller_model, &tool, grant_id.as_deref()) {
                Decision::Reject { guidance } | Decision::Delegate { guidance } => {
                    ClientAction::Respond(error_result(&id, &guidance))
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
                    ClientAction::Forward(msg)
                }
            }
        }
        _ => ClientAction::Forward(msg),
    }
}

/// Handle a server→client message: if it is the response to a recorded
/// `tools/list` request, inject a required `caller_model` argument into every
/// advertised tool's `inputSchema`. Otherwise pass through unchanged.
pub fn handle_server_message(mut msg: Value, pending: &mut PendingList) -> Value {
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
        return msg;
    }

    if let Some(tools) = msg
        .get_mut("result")
        .and_then(|r| r.get_mut("tools"))
        .and_then(Value::as_array_mut)
    {
        for tool in tools.iter_mut() {
            inject_caller_model_schema(tool);
        }
    }
    msg
}

/// Ensure a single tool object requires a `caller_model` string argument.
pub fn inject_caller_model_schema(tool: &mut Value) {
    let Some(obj) = tool.as_object_mut() else {
        return;
    };
    let schema = obj
        .entry("inputSchema")
        .or_insert_with(|| json!({ "type": "object" }));
    let Some(schema_obj) = schema.as_object_mut() else {
        return;
    };
    schema_obj
        .entry("type")
        .or_insert_with(|| json!("object"));

    let props = schema_obj
        .entry("properties")
        .or_insert_with(|| json!({}));
    if let Some(props_obj) = props.as_object_mut() {
        props_obj.insert(
            CALLER_MODEL_ARG.to_string(),
            json!({
                "type": "string",
                "description": "Id of the model issuing this call (e.g. claude-opus-4-8). Required for price-awareness enforcement."
            }),
        );
    }

    let required = schema_obj
        .entry("required")
        .or_insert_with(|| json!([]));
    if let Some(arr) = required.as_array_mut() {
        if !arr.iter().any(|v| v.as_str() == Some(CALLER_MODEL_ARG)) {
            arr.push(json!(CALLER_MODEL_ARG));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::Gate;
    use std::path::Path;

    fn test_gate() -> Gate {
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
            crate::gate::ModelBudgetCalibration::default(),
            None,
            None,
        )
        .unwrap();
        let _ = std::fs::remove_file(&path);
        g
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
        match handle_client_message(call("read_file", None), Some(&g), &mut p) {
            ClientAction::Respond(v) => {
                assert_eq!(v["result"]["isError"], json!(true));
                let text = v["result"]["content"][0]["text"].as_str().unwrap();
                assert!(text.contains(CALLER_MODEL_ARG));
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
        let g = Gate::load(
            std::path::Path::new(&path),
            crate::gate::ModelBudgetCalibration::default(),
            Some(std::path::Path::new(&rollup_path)),
            None,
        )
        .unwrap();
        // Clean up temp files after loading
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&rollup_path);

        let mut p = PendingList::default();
        match handle_client_message(call("read_file", Some("claude-opus-4-1")), Some(&g), &mut p) {
            ClientAction::Respond(v) => {
                assert_eq!(v["result"]["isError"], json!(true));
                assert!(v["result"]["content"][0]["text"].as_str().unwrap().to_lowercase().contains("delegate"));
            }
            other => panic!("expected Respond, got {other:?}"),
        }
    }

    #[test]
    fn unmeasured_tool_fail_open() {
        // Without a rollup, even expensive models can call any tool (fail open)
        let g = test_gate();
        let mut p = PendingList::default();
        match handle_client_message(call("read_file", Some("claude-opus-4-1")), Some(&g), &mut p) {
            ClientAction::Forward(v) => {
                let args = &v["params"]["arguments"];
                assert!(args.get(CALLER_MODEL_ARG).is_none(), "caller_model must be stripped");
            }
            other => panic!("expected Forward (fail open), got {other:?}"),
        }
    }

    #[test]
    fn cheap_forwards_and_strips_caller_model() {
        let g = test_gate();
        let mut p = PendingList::default();
        // Use a light tool (cost 1) that gpt-5-mini (budget ~97) can afford
        match handle_client_message(call("some_unknown_tool", Some("gpt-5-mini")), Some(&g), &mut p) {
            ClientAction::Forward(v) => {
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
        match handle_client_message(call("runSubagent", Some("claude-opus-4-1")), Some(&g), &mut p) {
            ClientAction::Forward(_) => {}
            other => panic!("expected Forward, got {other:?}"),
        }
    }

    #[test]
    fn unknown_caller_model_is_rejected() {
        let g = test_gate();
        let mut p = PendingList::default();
        match handle_client_message(call("read_file", Some("github-copilot")), Some(&g), &mut p) {
            ClientAction::Respond(v) => {
                assert_eq!(v["result"]["isError"], json!(true));
                let text = v["result"]["content"][0]["text"].as_str().unwrap();
                assert!(text.to_lowercase().contains("unknown caller_model"));
            }
            other => panic!("expected Respond, got {other:?}"),
        }
    }

    #[test]
    fn no_gate_is_passthrough() {
        let mut p = PendingList::default();
        match handle_client_message(call("read_file", None), None, &mut p) {
            ClientAction::Forward(_) => {}
            other => panic!("expected Forward, got {other:?}"),
        }
    }

    #[test]
    fn tools_list_response_gets_schema_injected() {
        let mut p = PendingList::default();
        // Record the list request id.
        let req = json!({"jsonrpc":"2.0","id":7,"method":"tools/list","params":{}});
        let g = test_gate();
        let _ = handle_client_message(req, Some(&g), &mut p);

        let resp = json!({
            "jsonrpc": "2.0",
            "id": 7,
            "result": { "tools": [ { "name": "read_file", "inputSchema": { "type": "object", "properties": {}, "required": [] } } ] }
        });
        let out = handle_server_message(resp, &mut p);
        let tool = &out["result"]["tools"][0];
        assert_eq!(tool["inputSchema"]["properties"][CALLER_MODEL_ARG]["type"], json!("string"));
        let required = tool["inputSchema"]["required"].as_array().unwrap();
        assert!(required.iter().any(|v| v == CALLER_MODEL_ARG));
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
}
