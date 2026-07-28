//! Delegation cost analyzer (ticket b7c61f0e).
//!
//! Promotes the ad-hoc `tmp/subagent_cost_probe.py` analysis into a supported,
//! tested report: per-sub-agent tool histograms, path-normalized duplicate
//! reads, duplicate command detection, failure classification, and — once
//! real usage flows through `data_json.usage` (ticket 9d527ad1) — real token
//! and cost figures per sub-agent instead of derived estimates.
//!
//! Sub-agent span attribution is resolved at capture time in
//! [`crate::hook::transcript`] via `parent_event_id` ancestry and stamped onto
//! [`crate::SessionTurnEventMeta::subagent_run_id`]. This means every event
//! belongs to exactly one owning span regardless of how parallel sub-agent
//! spans interleave in the flat transcript, so this module can group turns by
//! that key directly without re-deriving ancestry and without double-counting
//! overlapping spans.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{SessionRecord, SessionRole};

/// Normalize a file path spelling for cross-agent duplicate-read detection.
///
/// Converts backslashes to forward slashes and lowercases a leading Windows
/// drive letter, so `C:\foo\bar` and `c:/foo/bar` dedupe to the same key.
pub fn normalize_path_for_dedup(path: &str) -> String {
    let unified = path.replace('\\', "/");
    let mut chars = unified.chars();
    match (chars.next(), chars.next()) {
        (Some(drive), Some(':')) if drive.is_ascii_alphabetic() => {
            let rest: String = chars.collect();
            format!("{}:{}", drive.to_ascii_lowercase(), rest)
        }
        _ => unified,
    }
}

fn normalize_command(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ")
}

const READ_TOOL_NAMES: &[&str] = &[
    "read_file",
    "peek_read",
    "peek_grep",
    "peek_count",
    "peek_skeleton",
];

const TERMINAL_TOOL_NAME: &str = "run_in_terminal";

/// A repeated read or command within a single sub-agent's own span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepeatCount {
    pub key: String,
    pub count: u64,
}

/// A failed tool call observed within a sub-agent's span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationFailure {
    pub tool_name: String,
    pub summary: String,
}

/// A duplicate artifact (file read or command) shared across more than one
/// sub-agent span.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrossAgentDuplicate {
    pub key: String,
    pub agent_count: usize,
    pub total_count: u64,
}

/// Per-sub-agent cost and waste attribution for a single delegation span.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SubAgentDelegationReport {
    /// The `tool_call_id` of the `runSubagent` invocation that opened this
    /// span; stable per delegation within the session.
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// The model requested for this delegation, when declared by the caller.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub declared_model: Option<String>,
    pub tool_call_count: u64,
    pub tools: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeat_reads: Vec<RepeatCount>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub repeat_commands: Vec<RepeatCount>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failures: Vec<DelegationFailure>,
    /// Real token/cost attribution (ticket 9d527ad1), summed from turns whose
    /// `subagent_run_id` matches this span.
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub cache_write_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd: Option<f64>,
}

/// Per-session delegation cost report: the promoted equivalent of the
/// throwaway `tmp/subagent_cost_probe.py` analysis.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DelegationCostReport {
    pub session_id: String,
    pub subagent_count: usize,
    pub parent_tool_call_count: u64,
    pub parent_tools: BTreeMap<String, u64>,
    pub subagents: Vec<SubAgentDelegationReport>,
    /// Files read by more than one distinct sub-agent, path-normalization
    /// safe.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cross_agent_duplicate_reads: Vec<CrossAgentDuplicate>,
    /// Commands run more than twice in total across sub-agents.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cross_agent_duplicate_commands: Vec<CrossAgentDuplicate>,
}

const PARENT_BUCKET: &str = "__parent__";

/// Compute the delegation cost report for a single captured session.
///
/// A supported, tested replacement for the ad-hoc probe script: reproduces
/// per-sub-agent tool histograms, cross-agent duplicate-read detection
/// (path-normalization safe), duplicate-command detection, failure
/// classification, and real per-sub-agent token/cost totals when available.
pub fn compute_delegation_cost_report(record: &SessionRecord) -> DelegationCostReport {
    // Discover agent_name/description/declared_model per run_id from the
    // `runSubagent` wrapper's own completion turn. That turn's own
    // `subagent_run_id` is its *parent's* span (it is a call the parent
    // made), but its `tool_call_id` names the span it opens for descendants.
    let mut agent_info: BTreeMap<String, (Option<String>, Option<String>, Option<String>)> =
        BTreeMap::new();
    for turn in &record.turns {
        if turn.tool_name.as_deref() != Some("runSubagent") {
            continue;
        }
        let Some(meta) = &turn.event_meta else {
            continue;
        };
        let Some(run_id) = &meta.tool_call_id else {
            continue;
        };
        let args = meta.tool_arguments_json.as_ref();
        let agent_name = args
            .and_then(|v| v.get("agentName"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let description = args
            .and_then(|v| v.get("description"))
            .and_then(|v| v.as_str())
            .map(String::from);
        let declared_model = args
            .and_then(|v| v.get("model"))
            .and_then(|v| v.as_str())
            .map(String::from);
        agent_info.insert(run_id.clone(), (agent_name, description, declared_model));
    }

    let mut per_run: BTreeMap<String, SubAgentDelegationReport> = BTreeMap::new();
    let mut parent_tool_call_count = 0u64;
    let mut parent_tools: BTreeMap<String, u64> = BTreeMap::new();

    // path/command -> run bucket (PARENT_BUCKET or a run_id) -> count, used
    // for both within-agent repeat detection and cross-agent duplicates.
    let mut reads_by_key: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();
    let mut commands_by_key: BTreeMap<String, BTreeMap<String, u64>> = BTreeMap::new();

    for turn in &record.turns {
        if turn.role != SessionRole::Tool {
            continue;
        }
        let Some(tool_name) = &turn.tool_name else {
            continue;
        };
        // The runSubagent wrapper call itself is a structural marker, not a
        // real tool call performed by an agent; exclude it from all tallies
        // (its span boundary is tracked separately via agent_info).
        if tool_name == "runSubagent" {
            continue;
        }

        let meta = turn.event_meta.as_ref();
        let run_id = meta.and_then(|m| m.subagent_run_id.clone());
        let bucket = run_id.clone().unwrap_or_else(|| PARENT_BUCKET.to_string());

        match &run_id {
            None => {
                parent_tool_call_count += 1;
                *parent_tools.entry(tool_name.clone()).or_insert(0) += 1;
            }
            Some(rid) => {
                let entry = per_run.entry(rid.clone()).or_insert_with(|| {
                    let (agent_name, description, declared_model) = agent_info
                        .get(rid)
                        .cloned()
                        .unwrap_or((None, None, None));
                    SubAgentDelegationReport {
                        run_id: rid.clone(),
                        agent_name,
                        description,
                        declared_model,
                        ..Default::default()
                    }
                });
                entry.tool_call_count += 1;
                *entry.tools.entry(tool_name.clone()).or_insert(0) += 1;
            }
        }

        let is_failure = meta
            .map(|m| {
                matches!(
                    m.result_code.as_deref(),
                    Some("error") | Some("timeout") | Some("hang")
                ) || m.tool_success == Some(false)
            })
            .unwrap_or(false);
        if is_failure {
            if let Some(rid) = &run_id {
                let summary = meta
                    .and_then(|m| m.error_message.clone())
                    .unwrap_or_else(|| "tool call failed".to_string());
                if let Some(entry) = per_run.get_mut(rid) {
                    entry.failures.push(DelegationFailure {
                        tool_name: tool_name.clone(),
                        summary,
                    });
                }
            }
        }

        let args = meta.and_then(|m| m.tool_arguments_json.as_ref());
        if READ_TOOL_NAMES.contains(&tool_name.as_str()) {
            if let Some(raw_path) = args
                .and_then(|a| a.get("filePath").or_else(|| a.get("path")))
                .and_then(|v| v.as_str())
            {
                let key = normalize_path_for_dedup(raw_path);
                *reads_by_key
                    .entry(key)
                    .or_default()
                    .entry(bucket.clone())
                    .or_insert(0) += 1;
            }
        } else if tool_name == TERMINAL_TOOL_NAME {
            if let Some(raw_command) = args.and_then(|a| a.get("command")).and_then(|v| v.as_str())
            {
                let key = normalize_command(raw_command);
                *commands_by_key
                    .entry(key)
                    .or_default()
                    .entry(bucket)
                    .or_insert(0) += 1;
            }
        }
    }

    // Token/cost attribution: usage is recorded per-turn (typically on
    // assistant turns), so walk every turn regardless of role. A span may
    // consist entirely of assistant turns with no tool calls, so ensure its
    // entry exists here rather than assuming the tool-call loop above
    // already created it.
    for turn in &record.turns {
        let Some(meta) = &turn.event_meta else {
            continue;
        };
        let Some(rid) = &meta.subagent_run_id else {
            continue;
        };
        let entry = per_run.entry(rid.clone()).or_insert_with(|| {
            let (agent_name, description, declared_model) =
                agent_info.get(rid).cloned().unwrap_or((None, None, None));
            SubAgentDelegationReport {
                run_id: rid.clone(),
                agent_name,
                description,
                declared_model,
                ..Default::default()
            }
        });
        entry.input_tokens += meta.input_tokens.unwrap_or(0);
        entry.output_tokens += meta.output_tokens.unwrap_or(0);
        entry.cache_read_tokens += meta.cache_read_tokens.unwrap_or(0);
        entry.cache_write_tokens += meta.cache_write_tokens.unwrap_or(0);
        if let Some(cost) = meta.cost_usd {
            entry.cost_usd = Some(entry.cost_usd.unwrap_or(0.0) + cost);
        }
    }

    // Within-agent repeats (count > 1 for that agent's own bucket).
    for (path, by_bucket) in &reads_by_key {
        for (bucket, count) in by_bucket {
            if *count > 1 && bucket != PARENT_BUCKET {
                if let Some(entry) = per_run.get_mut(bucket) {
                    entry.repeat_reads.push(RepeatCount {
                        key: path.clone(),
                        count: *count,
                    });
                }
            }
        }
    }
    for (command, by_bucket) in &commands_by_key {
        for (bucket, count) in by_bucket {
            if *count > 1 && bucket != PARENT_BUCKET {
                if let Some(entry) = per_run.get_mut(bucket) {
                    entry.repeat_commands.push(RepeatCount {
                        key: command.clone(),
                        count: *count,
                    });
                }
            }
        }
    }
    for entry in per_run.values_mut() {
        entry
            .repeat_reads
            .sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
        entry
            .repeat_commands
            .sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
    }

    // Cross-agent duplicates: read/run by more than one distinct sub-agent.
    let mut cross_agent_duplicate_reads = Vec::new();
    for (path, by_bucket) in &reads_by_key {
        let agent_buckets = by_bucket
            .iter()
            .filter(|(bucket, _)| bucket.as_str() != PARENT_BUCKET)
            .count();
        if agent_buckets > 1 {
            let total_count: u64 = by_bucket
                .iter()
                .filter(|(bucket, _)| bucket.as_str() != PARENT_BUCKET)
                .map(|(_, count)| *count)
                .sum();
            cross_agent_duplicate_reads.push(CrossAgentDuplicate {
                key: path.clone(),
                agent_count: agent_buckets,
                total_count,
            });
        }
    }
    cross_agent_duplicate_reads.sort_by(|a, b| {
        b.agent_count
            .cmp(&a.agent_count)
            .then_with(|| b.total_count.cmp(&a.total_count))
            .then_with(|| a.key.cmp(&b.key))
    });

    let mut cross_agent_duplicate_commands = Vec::new();
    for (command, by_bucket) in &commands_by_key {
        let agent_count = by_bucket
            .iter()
            .filter(|(bucket, _)| bucket.as_str() != PARENT_BUCKET)
            .count();
        let total_count: u64 = by_bucket.values().sum();
        if total_count > 2 {
            cross_agent_duplicate_commands.push(CrossAgentDuplicate {
                key: command.clone(),
                agent_count,
                total_count,
            });
        }
    }
    cross_agent_duplicate_commands
        .sort_by(|a, b| b.total_count.cmp(&a.total_count).then_with(|| a.key.cmp(&b.key)));

    let subagents: Vec<_> = per_run.into_values().collect();

    DelegationCostReport {
        session_id: record.session_id.clone(),
        subagent_count: subagents.len(),
        parent_tool_call_count,
        parent_tools,
        subagents,
        cross_agent_duplicate_reads,
        cross_agent_duplicate_commands,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SessionLinks, SessionMetadata, SessionTurn, SessionTurnEventMeta};
    use chrono::Utc;

    fn base_meta() -> SessionTurnEventMeta {
        SessionTurnEventMeta::default()
    }

    fn record_with_turns(turns: Vec<SessionTurn>) -> SessionRecord {
        SessionRecord {
            schema_version: 1,
            session_id: "sess-1".to_string(),
            source: "test".to_string(),
            started_at: Utc::now(),
            captured_at: Utc::now(),
            metadata: SessionMetadata {
                workspace_slug: "test".to_string(),
                conversation_id: None,
                agent_id: None,
                ticket_id: None,
                model: None,
                trigger: None,
                producer: None,
                copilot_version: None,
                vscode_version: None,
                protocol_version: None,
                worktree: None,
            },
            turns,
            links: SessionLinks::default(),
            track_id: None,
            anchor_ticket_id: None,
            parent_session_id: None,
            spawned_session_id: None,
        }
    }

    fn tool_turn(
        sequence: usize,
        tool_name: &str,
        run_id: Option<&str>,
        args: serde_json::Value,
    ) -> SessionTurn {
        SessionTurn {
            sequence,
            role: SessionRole::Tool,
            content: "ok".to_string(),
            captured_at: Utc::now(),
            tool_name: Some(tool_name.to_string()),
            model: None,
            event_meta: Some(SessionTurnEventMeta {
                tool_success: Some(true),
                tool_arguments_json: Some(args),
                subagent_run_id: run_id.map(String::from),
                ..base_meta()
            }),
        }
    }

    #[test]
    fn normalizes_backslash_and_drive_letter_case_for_dedup() {
        assert_eq!(
            normalize_path_for_dedup("C:\\foo\\bar.md"),
            normalize_path_for_dedup("c:/foo/bar.md")
        );
        assert_eq!(normalize_path_for_dedup("c:/foo/bar.md"), "c:/foo/bar.md");
    }

    #[test]
    fn parallel_spans_are_attributed_without_double_counting() {
        // Two sub-agent spans dispatched in parallel: their tool calls
        // interleave in the flat transcript, but each turn's own
        // subagent_run_id (stamped from parent_event_id ancestry, not
        // index-range overlap) unambiguously identifies its owner.
        let turns = vec![
            tool_turn(
                0,
                "runSubagent",
                None,
                serde_json::json!({"agentName": "Explore", "description": "probe A"}),
            ),
            tool_turn(
                1,
                "runSubagent",
                None,
                serde_json::json!({"agentName": "Explore", "description": "probe B"}),
            ),
            tool_turn(2, "read_file", Some("call-a"), serde_json::json!({"filePath": "x.rs"})),
            tool_turn(3, "read_file", Some("call-b"), serde_json::json!({"filePath": "y.rs"})),
            tool_turn(4, "read_file", Some("call-a"), serde_json::json!({"filePath": "z.rs"})),
        ];
        // Re-key the wrapper turns' own tool_call_id so agent_info resolves.
        let mut turns = turns;
        turns[0].event_meta.as_mut().unwrap().tool_call_id = Some("call-a".to_string());
        turns[1].event_meta.as_mut().unwrap().tool_call_id = Some("call-b".to_string());

        let record = record_with_turns(turns);
        let report = compute_delegation_cost_report(&record);

        assert_eq!(report.subagent_count, 2);
        let call_a = report
            .subagents
            .iter()
            .find(|s| s.run_id == "call-a")
            .expect("call-a span");
        let call_b = report
            .subagents
            .iter()
            .find(|s| s.run_id == "call-b")
            .expect("call-b span");
        assert_eq!(call_a.tool_call_count, 2);
        assert_eq!(call_b.tool_call_count, 1);
        // No tool call is double-counted or dropped: 3 real reads total.
        assert_eq!(
            call_a.tool_call_count + call_b.tool_call_count + report.parent_tool_call_count,
            3
        );
    }

    #[test]
    fn duplicate_reads_are_path_normalization_safe() {
        let turns = vec![
            tool_turn(
                0,
                "read_file",
                Some("call-a"),
                serde_json::json!({"filePath": "C:\\repo\\notes.md"}),
            ),
            tool_turn(
                1,
                "read_file",
                Some("call-b"),
                serde_json::json!({"filePath": "c:/repo/notes.md"}),
            ),
        ];
        let record = record_with_turns(turns);
        let report = compute_delegation_cost_report(&record);

        assert_eq!(report.cross_agent_duplicate_reads.len(), 1);
        let dup = &report.cross_agent_duplicate_reads[0];
        assert_eq!(dup.agent_count, 2);
        assert_eq!(dup.total_count, 2);
        assert_eq!(dup.key, "c:/repo/notes.md");
    }

    #[test]
    fn failures_are_attributed_to_the_owning_span() {
        let mut turn = tool_turn(
            0,
            "run_in_terminal",
            Some("call-a"),
            serde_json::json!({"command": "cargo test"}),
        );
        turn.event_meta.as_mut().unwrap().tool_success = Some(false);
        turn.event_meta.as_mut().unwrap().result_code = Some("error".to_string());
        turn.event_meta.as_mut().unwrap().error_message = Some("compile error".to_string());

        let record = record_with_turns(vec![turn]);
        let report = compute_delegation_cost_report(&record);

        let span = &report.subagents[0];
        assert_eq!(span.failures.len(), 1);
        assert_eq!(span.failures[0].summary, "compile error");
    }

    #[test]
    fn real_token_and_cost_totals_flow_per_span() {
        let mut assistant_turn = SessionTurn {
            sequence: 0,
            role: SessionRole::Assistant,
            content: "work".to_string(),
            captured_at: Utc::now(),
            tool_name: None,
            model: Some("gpt-5".to_string()),
            event_meta: Some(SessionTurnEventMeta {
                input_tokens: Some(1000),
                output_tokens: Some(200),
                cost_usd: Some(0.05),
                subagent_run_id: Some("call-a".to_string()),
                ..base_meta()
            }),
        };
        assistant_turn.event_meta.as_mut().unwrap().model_id = Some("gpt-5".to_string());

        let record = record_with_turns(vec![assistant_turn]);
        let report = compute_delegation_cost_report(&record);

        assert_eq!(report.subagent_count, 1);
        let span = &report.subagents[0];
        assert_eq!(span.input_tokens, 1000);
        assert_eq!(span.output_tokens, 200);
        assert!((span.cost_usd.unwrap() - 0.05).abs() < 1e-9);
    }
}
