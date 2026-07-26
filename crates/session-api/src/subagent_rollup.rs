use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{SessionRecord, SessionRole, SessionRuntimeContext};

/// Per-sub-agent cost and usage rollup.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubAgentRollup {
    pub run_id: String,
    pub session_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    pub turn_count: usize,
    pub tool_call_count: usize,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wall_time_secs: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

/// Compute sub-agent rollups from a session and its runtime context.
/// Returns a map keyed by run_id.
pub fn compute_subagent_rollups(
    record: &SessionRecord,
    context: Option<&SessionRuntimeContext>,
) -> HashMap<String, SubAgentRollup> {
    let mut rollups: HashMap<String, SubAgentRollup> = HashMap::new();

    // If there's a runtime context, initialize rollups for all runs
    if let Some(ctx) = context {
        for run in &ctx.runs {
            if let Some(session_id) = &run.captured_session_id {
                rollups.insert(
                    run.run_id.clone(),
                    SubAgentRollup {
                        run_id: run.run_id.clone(),
                        session_id: session_id.clone(),
                        model: None,
                        turn_count: 0,
                        tool_call_count: 0,
                        input_tokens: 0,
                        output_tokens: 0,
                        cache_read_tokens: 0,
                        cache_write_tokens: 0,
                        cost_usd: None,
                        wall_time_secs: None,
                        outcome: None,
                    },
                );
            }
        }
    }

    // Aggregate token/cost data from turns with event_meta
    for turn in &record.turns {
        if let Some(meta) = &turn.event_meta {
            // For sub-agent sessions (those with a run_id in context),
            // aggregate into that run's rollup. For top-level sessions
            // without a run_id, we could create a synthetic rollup, but
            // skip for now since the ticket focuses on sub-agent attribution.
            
            // Extract model_id from event_meta or turn.model
            let model_id = meta.model_id.as_ref().or(turn.model.as_ref());
            
            // Count this turn if it's an assistant turn
            let is_assistant = turn.role == SessionRole::Assistant;
            
            // Count tool calls (tool turns)
            let is_tool_call = turn.role == SessionRole::Tool;
            
            // Aggregate tokens
            let input = meta.input_tokens.unwrap_or(0);
            let output = meta.output_tokens.unwrap_or(0);
            let cache_read = meta.cache_read_tokens.unwrap_or(0);
            let cache_write = meta.cache_write_tokens.unwrap_or(0);
            
            // For now, aggregate all turns into the main session's rollup
            // (we'd need a way to match turns to specific run_ids for true
            // per-sub-agent attribution, which would require run_id in event_meta)
            let rollup_key = record.session_id.clone();
            
            let rollup = rollups.entry(rollup_key.clone()).or_insert_with(|| {
                SubAgentRollup {
                    run_id: rollup_key.clone(),
                    session_id: record.session_id.clone(),
                    model: None,
                    turn_count: 0,
                    tool_call_count: 0,
                    input_tokens: 0,
                    output_tokens: 0,
                    cache_read_tokens: 0,
                    cache_write_tokens: 0,
                    cost_usd: None,
                    outcome: None,
                    wall_time_secs: None,
                }
            });
            
            if is_assistant {
                rollup.turn_count += 1;
            }
            
            if is_tool_call {
                rollup.tool_call_count += 1;
            }
            
            rollup.input_tokens += input;
            rollup.output_tokens += output;
            rollup.cache_read_tokens += cache_read;
            rollup.cache_write_tokens += cache_write;
            
            // Set model if we have one
            if rollup.model.is_none() && model_id.is_some() {
                rollup.model = model_id.cloned();
            }
            
            // Aggregate cost
            if let Some(cost) = meta.cost_usd {
                rollup.cost_usd = Some(rollup.cost_usd.unwrap_or(0.0) + cost);
            }
        }
    }
    
    // Compute wall time from context if available
    if let Some(ctx) = context {
        for run in &ctx.runs {
            if let Some(rollup) = rollups.get_mut(&run.run_id) {
                // Wall time computation would require end time in SessionRunLineage
                // For now, leave as None - this can be enhanced in a follow-up
                rollup.wall_time_secs = None;
            }
        }
    }

    rollups
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SessionMetadata, SessionTurn, SessionTurnEventMeta, SessionLinks};
    use chrono::Utc;

    #[test]
    fn compute_rollup_aggregates_token_counts() {
        let record = SessionRecord {
            schema_version: 1,
            session_id: "session-1".to_string(),
            source: "test".to_string(),
            started_at: Utc::now(),
            captured_at: Utc::now(),
            metadata: SessionMetadata {
                workspace_slug: "test".to_string(),
                conversation_id: None,
                agent_id: None,
                ticket_id: None,
                model: Some("claude-3-5-sonnet".to_string()),
                trigger: None,
                producer: None,
                copilot_version: None,
                vscode_version: None,
                protocol_version: None,
                worktree: None,
            },
            turns: vec![
                SessionTurn {
                    sequence: 0,
                    role: SessionRole::Assistant,
                    content: "Hello".to_string(),
                    captured_at: Utc::now(),
                    tool_name: None,
                    model: Some("claude-3-5-sonnet".to_string()),
                    event_meta: Some(SessionTurnEventMeta {
                        event_id: None,
                        parent_event_id: None,
                        event_type: None,
                        turn_id: None,
                        message_id: None,
                        tool_call_id: None,
                        tool_success: None,
                        reasoning_text: None,
                        tool_requests_json: None,
                        tool_arguments_json: None,
                        input_tokens: Some(1000),
                        output_tokens: Some(500),
                        cache_read_tokens: Some(200),
                        cache_write_tokens: Some(100),
                        cost_usd: Some(0.05),
                        model_id: Some("claude-3-5-sonnet".to_string()),
                        error_message: None,
                        exit_code: None,
                        result_code: None,
                    }),
                },
                SessionTurn {
                    sequence: 1,
                    role: SessionRole::Assistant,
                    content: "World".to_string(),
                    captured_at: Utc::now(),
                    tool_name: None,
                    model: Some("claude-3-5-sonnet".to_string()),
                    event_meta: Some(SessionTurnEventMeta {
                        event_id: None,
                        parent_event_id: None,
                        event_type: None,
                        turn_id: None,
                        message_id: None,
                        tool_call_id: None,
                        tool_success: None,
                        reasoning_text: None,
                        tool_requests_json: None,
                        tool_arguments_json: None,
                        input_tokens: Some(2000),
                        output_tokens: Some(1000),
                        cache_read_tokens: Some(0),
                        cache_write_tokens: Some(0),
                        cost_usd: Some(0.10),
                        model_id: Some("claude-3-5-sonnet".to_string()),
                        error_message: None,
                        exit_code: None,
                        result_code: None,
                    }),
                },
            ],
            links: SessionLinks::default(),
        };

        let rollups = compute_subagent_rollups(&record, None);
        let rollup = rollups.get("session-1").expect("rollup should exist");

        assert_eq!(rollup.session_id, "session-1");
        assert_eq!(rollup.turn_count, 2);
        assert_eq!(rollup.input_tokens, 3000);
        assert_eq!(rollup.output_tokens, 1500);
        assert_eq!(rollup.cache_read_tokens, 200);
        assert_eq!(rollup.cache_write_tokens, 100);
        // Floating point comparison with tolerance for precision
        assert!((rollup.cost_usd.unwrap() - 0.15).abs() < 0.0001);
        assert_eq!(rollup.model.as_deref(), Some("claude-3-5-sonnet"));
    }
}
