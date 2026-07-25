//! Model-aware cost gate: the transport-agnostic decision core.
//!
//! Rust port of `tools/model-prices/cost_gate.py`. It resolves a model's
//! `output_mtok` from the shared price table (`model_prices.json`) and decides
//! whether a given tool may run directly or must be delegated to a cheaper
//! sub-agent. Policy (see AGENTS.md "Model cost awareness & routing"):
//!
//! * Driving field: `output_mtok` (USD per 1M output tokens).
//! * Threshold `X = 15`; the gate fires when `output_mtok > X` (strict).
//! * Orchestrator-tier model + token-heavy tool ⇒ delegate; otherwise allow.

use std::path::Path;

use serde::Deserialize;

/// Default threshold on `output_mtok` (USD per 1M output tokens). Equivalent to
/// 1500 credits/1M at 100 credits = $1. Keep in sync with the AGENTS.md rule.
pub const DEFAULT_THRESHOLD_X: f64 = 15.0;

/// Tools that consume large amounts of context/output tokens when driven
/// directly by an expensive model. Matched as case-insensitive substrings so
/// provider-prefixed names (e.g. `mcp_ticket-mcp_get_ticket`) are covered.
pub const TOKEN_HEAVY_TOOL_SUBSTRINGS: &[&str] = &[
    "read_file",
    "read_notebook_cell_output",
    "semantic_search",
    "grep_search",
    "file_search",
    "list_dir",
    "fetch_webpage",
    "get_log",
    "query_logs",
    "search_all_logs",
    "get_source",
    "peek_read",
    "peek_grep",
    "peek_skeleton",
    "get_ticket_description",
    "spec_get",
    "spec_section_get",
    "session_peek_range",
    "session_peek_skeleton",
    "subgraph",
    "topgraph",
];

/// Tools always allowed even in orchestrator mode: planning, delegation, and
/// lightweight status/mutation calls. The sub-agent spawn primitive must always
/// be allowed so an orchestrator can delegate.
pub const ALWAYS_ALLOWED_TOOL_SUBSTRINGS: &[&str] = &[
    "runsubagent",
    "run_subagent",
    "board_check_in",
    "board_check_out",
    "board_heartbeat",
    "update_ticket",
    "workflow_set_status",
];

/// Classification of a tool for enforcement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolClass {
    AlwaysAllowed,
    TokenHeavy,
    Light,
}

/// Outcome of a gate evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Delegate { guidance: String },
}

#[derive(Debug, Deserialize)]
struct PriceRow {
    #[serde(default)]
    provider_id: String,
    #[serde(default)]
    model_id: String,
    #[serde(default)]
    output_mtok: Option<f64>,
}

#[derive(Debug, Deserialize)]
struct PriceTable {
    #[serde(default)]
    models: Vec<PriceRow>,
}

/// The loaded price table plus the active threshold.
#[derive(Debug)]
pub struct Gate {
    models: Vec<PriceRow>,
    x: f64,
}

impl Gate {
    /// Build a gate from parsed model rows.
    fn from_rows(models: Vec<PriceRow>, x: f64) -> Self {
        Self { models, x }
    }

    /// Load the price table from `path`. Returns an error string on failure so
    /// callers can decide to fail open.
    pub fn load(path: &Path, x: f64) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let table: PriceTable =
            serde_json::from_str(&text).map_err(|e| format!("bad JSON in {}: {e}", path.display()))?;
        Ok(Self::from_rows(table.models, x))
    }

    /// Resolve `output_mtok` for `model`, mirroring the Python gate: exact
    /// (case-insensitive) `model_id` match wins; otherwise a case-insensitive
    /// substring match on `provider_id`/`model_id`, taking the **maximum**
    /// price among matches so an ambiguous id is never cheaper than its most
    /// expensive variant. `None` when nothing matches.
    pub fn resolve_output_mtok(&self, model: &str) -> Option<f64> {
        let low = model.to_lowercase();

        let exact_max = self
            .models
            .iter()
            .filter(|r| r.model_id.to_lowercase() == low)
            .filter_map(|r| r.output_mtok)
            .fold(None, fold_max);
        if exact_max.is_some() {
            return exact_max;
        }

        self.models
            .iter()
            .filter(|r| {
                r.provider_id.to_lowercase().contains(&low)
                    || r.model_id.to_lowercase().contains(&low)
            })
            .filter_map(|r| r.output_mtok)
            .fold(None, fold_max)
    }

    /// True when `output_mtok` is strictly greater than the threshold.
    pub fn is_orchestrator(&self, output_mtok: f64) -> bool {
        output_mtok > self.x
    }

    /// Decide whether `model` may call `tool` directly.
    ///
    /// * below/at threshold ⇒ Allow;
    /// * above threshold + token-heavy ⇒ Delegate (with guidance);
    /// * above threshold + light/always-allowed ⇒ Allow.
    ///
    /// Unknown models are treated conservatively as orchestrators, so an
    /// unlisted expensive model is not silently allowed token-heavy work.
    pub fn evaluate(&self, model: &str, tool: &str) -> Decision {
        let orchestrator = match self.resolve_output_mtok(model) {
            Some(out) => self.is_orchestrator(out),
            None => true,
        };
        if !orchestrator {
            return Decision::Allow;
        }
        match classify_tool(tool) {
            ToolClass::AlwaysAllowed | ToolClass::Light => Decision::Allow,
            ToolClass::TokenHeavy => Decision::Delegate {
                guidance: delegation_guidance(model, tool, self.x),
            },
        }
    }
}

fn fold_max(acc: Option<f64>, v: f64) -> Option<f64> {
    Some(match acc {
        Some(a) if a >= v => a,
        _ => v,
    })
}

/// Classify a tool name. `always_allowed` wins over `token_heavy`.
pub fn classify_tool(tool: &str) -> ToolClass {
    let low = tool.to_lowercase();
    if ALWAYS_ALLOWED_TOOL_SUBSTRINGS.iter().any(|s| low.contains(s)) {
        return ToolClass::AlwaysAllowed;
    }
    if TOKEN_HEAVY_TOOL_SUBSTRINGS.iter().any(|s| low.contains(s)) {
        return ToolClass::TokenHeavy;
    }
    ToolClass::Light
}

/// Delegation guidance returned when an orchestrator-tier model calls a
/// token-heavy tool.
pub fn delegation_guidance(model: &str, tool: &str, x: f64) -> String {
    format!(
        "Model '{model}' exceeds the orchestrator threshold (output_mtok > {x} \
         USD/1M). Do not call the token-heavy tool '{tool}' directly. Delegate \
         it to a cheaper sub-agent via runSubagent(model=<cheaper>, ...) and \
         aggregate the result. Reserve this model for strategic decisions, \
         code/change planning, and tool-call planning."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> Gate {
        let rows = vec![
            PriceRow { provider_id: "anthropic".into(), model_id: "claude-opus-4-1".into(), output_mtok: Some(75.0) },
            PriceRow { provider_id: "anthropic".into(), model_id: "claude-opus-4-5".into(), output_mtok: Some(25.0) },
            PriceRow { provider_id: "openai".into(), model_id: "o3".into(), output_mtok: Some(40.0) },
            PriceRow { provider_id: "anthropic".into(), model_id: "claude-sonnet-4-5".into(), output_mtok: Some(15.0) },
            PriceRow { provider_id: "openai".into(), model_id: "gpt-5".into(), output_mtok: Some(10.0) },
            PriceRow { provider_id: "openai".into(), model_id: "gpt-5-mini".into(), output_mtok: Some(2.0) },
        ];
        Gate::from_rows(rows, DEFAULT_THRESHOLD_X)
    }

    #[test]
    fn exact_match_and_case_insensitive() {
        assert_eq!(gate().resolve_output_mtok("claude-opus-4-1"), Some(75.0));
        assert_eq!(gate().resolve_output_mtok("GPT-5-MINI"), Some(2.0));
    }

    #[test]
    fn ambiguous_substring_takes_max() {
        assert_eq!(gate().resolve_output_mtok("claude-opus"), Some(75.0));
    }

    #[test]
    fn exact_wins_over_substring() {
        assert_eq!(gate().resolve_output_mtok("gpt-5"), Some(10.0));
    }

    #[test]
    fn unknown_model_none() {
        assert_eq!(gate().resolve_output_mtok("no-such"), None);
    }

    #[test]
    fn strict_threshold_boundary() {
        let g = gate();
        assert!(g.is_orchestrator(15.0001));
        assert!(!g.is_orchestrator(15.0));
    }

    #[test]
    fn classify() {
        assert_eq!(classify_tool("read_file"), ToolClass::TokenHeavy);
        assert_eq!(classify_tool("mcp_ticket-mcp_get_ticket_description"), ToolClass::TokenHeavy);
        assert_eq!(classify_tool("runSubagent"), ToolClass::AlwaysAllowed);
        assert_eq!(classify_tool("whatever"), ToolClass::Light);
    }

    #[test]
    fn evaluate_outcomes() {
        let g = gate();
        assert!(matches!(g.evaluate("claude-opus-4-1", "read_file"), Decision::Delegate { .. }));
        assert_eq!(g.evaluate("o3", "runSubagent"), Decision::Allow);
        // boundary sonnet: 15 is NOT strictly greater -> allow token-heavy.
        assert_eq!(g.evaluate("claude-sonnet-4-5", "read_file"), Decision::Allow);
        assert_eq!(g.evaluate("gpt-5-mini", "grep_search"), Decision::Allow);
        // unknown model: conservative -> delegate token-heavy.
        assert!(matches!(g.evaluate("mystery", "semantic_search"), Decision::Delegate { .. }));
    }
}
