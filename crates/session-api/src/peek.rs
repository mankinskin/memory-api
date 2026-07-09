use serde::{
    Deserialize,
    Serialize,
};
use std::collections::HashMap;

use crate::{
    SessionRecord,
    SessionRole,
    SessionTurn,
};

/// Default number of preview characters retained per turn in a skeleton view.
pub const DEFAULT_SKELETON_PREVIEW_CHARS: usize = 120;
/// Default content length after which turns are summarized.
pub const DEFAULT_PROMPT_SUMMARIZE_THRESHOLD_CHARS: usize = 600;

/// A bounded window of transcript turns for a single session.
///
/// This is a read-side view transform over an already-persisted
/// [`SessionRecord`]; it does not mutate or re-persist any session state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionTurnRange {
    pub session_id: String,
    pub total_turns: usize,
    /// Inclusive start index (0-based) of the returned window.
    pub start: usize,
    /// Exclusive end index (0-based) of the returned window.
    pub end: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub turns: Vec<SessionTurn>,
}

/// A single compact entry in a [`SessionSkeleton`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSkeletonEntry {
    pub sequence: usize,
    pub role: SessionRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    /// Truncated single-line preview of the turn content.
    pub preview: String,
    /// Full character length of the original turn content.
    pub content_len: usize,
}

/// A compact, body-stripped overview of a session transcript.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionSkeleton {
    pub session_id: String,
    pub total_turns: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<SessionSkeletonEntry>,
}

/// Classification for whether a turn should reach model-facing prompt context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PromptInclusion {
    Retain,
    Summarize,
    ReferenceOnly,
    DropFromPrompt,
}

/// A compact prompt-facing entry produced by guard classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPromptPackEntry {
    pub sequence: usize,
    pub role: SessionRole,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    pub inclusion: PromptInclusion,
    pub reason: String,
    pub preview: String,
    pub content_len: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_pointer: Option<String>,
}

/// Prompt-facing compact session view after applying guard classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionPromptPack {
    pub session_id: String,
    pub total_turns: usize,
    pub retained_turns: usize,
    pub summarized_turns: usize,
    pub reference_only_turns: usize,
    pub dropped_turns: usize,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub entries: Vec<SessionPromptPackEntry>,
}

/// Options used by prompt-pack classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptPackOptions {
    pub preview_chars: usize,
    pub summarize_threshold_chars: usize,
}

impl Default for PromptPackOptions {
    fn default() -> Self {
        Self {
            preview_chars: DEFAULT_SKELETON_PREVIEW_CHARS,
            summarize_threshold_chars: DEFAULT_PROMPT_SUMMARIZE_THRESHOLD_CHARS,
        }
    }
}

/// Return a bounded window of turns from `record`.
///
/// `start` is clamped to `[0, total]`. `end` defaults to `total` and is clamped
/// to `[start, total]`, so callers always receive a valid, non-panicking slice.
pub fn peek_turn_range(
    record: &SessionRecord,
    start: usize,
    end: Option<usize>,
) -> SessionTurnRange {
    let total = record.turns.len();
    let start = start.min(total);
    let end = end.unwrap_or(total).clamp(start, total);

    SessionTurnRange {
        session_id: record.session_id.clone(),
        total_turns: total,
        start,
        end,
        turns: record.turns[start..end].to_vec(),
    }
}

/// Return a compact skeleton of `record`, stripping turn bodies to a single
/// truncated preview line each.
///
/// `preview_chars` bounds the preview length; a value of `0` yields empty
/// previews while still reporting `content_len`.
pub fn peek_skeleton(
    record: &SessionRecord,
    preview_chars: usize,
) -> SessionSkeleton {
    let entries = record
        .turns
        .iter()
        .map(|turn| SessionSkeletonEntry {
            sequence: turn.sequence,
            role: turn.role.clone(),
            tool_name: turn.tool_name.clone(),
            preview: preview_line(&turn.content, preview_chars),
            content_len: turn.content.chars().count(),
        })
        .collect();

    SessionSkeleton {
        session_id: record.session_id.clone(),
        total_turns: record.turns.len(),
        entries,
    }
}

/// Build a compact prompt-facing view of `record` by classifying each turn as
/// retain/summarize/reference-only/drop-from-prompt.
///
/// The resulting `entries` vector contains only non-dropped items.
pub fn peek_prompt_pack(
    record: &SessionRecord,
    options: PromptPackOptions,
) -> SessionPromptPack {
    let mut entries = Vec::new();
    let mut retain = 0;
    let mut summarize = 0;
    let mut reference_only = 0;
    let mut dropped = 0;
    let mut seen_signatures: HashMap<String, usize> = HashMap::new();

    for turn in &record.turns {
        let content_len = turn.content.chars().count();
        let normalized = normalize_for_signature(&turn.content);
        let signature = format!(
            "{:?}|{}|{}",
            turn.role,
            turn.tool_name.as_deref().unwrap_or(""),
            normalized
        );

        if content_len == 0 {
            dropped += 1;
            continue;
        }

        if is_routine_retry_narration(turn, &normalized) {
            dropped += 1;
            continue;
        }

        if let Some(previous_sequence) = seen_signatures.get(&signature) {
            if is_repeated_state_check(turn) || turn.role == SessionRole::Tool {
                let _ = previous_sequence;
                dropped += 1;
                continue;
            }
        }

        seen_signatures.insert(signature, turn.sequence);

        let preview = preview_line(&turn.content, options.preview_chars);
        let reference_pointer = extract_reference_pointer(&turn.content);
        let (inclusion, reason) = if reference_pointer.is_some() {
            (
                PromptInclusion::ReferenceOnly,
                "artifact-pointer-detected".to_string(),
            )
        } else if content_len > options.summarize_threshold_chars {
            (
                PromptInclusion::Summarize,
                "oversized-content".to_string(),
            )
        } else {
            (PromptInclusion::Retain, "durable-content".to_string())
        };

        match inclusion {
            PromptInclusion::Retain => retain += 1,
            PromptInclusion::Summarize => summarize += 1,
            PromptInclusion::ReferenceOnly => reference_only += 1,
            PromptInclusion::DropFromPrompt => dropped += 1,
        }

        entries.push(SessionPromptPackEntry {
            sequence: turn.sequence,
            role: turn.role.clone(),
            tool_name: turn.tool_name.clone(),
            inclusion,
            reason,
            preview,
            content_len,
            reference_pointer,
        });
    }

    SessionPromptPack {
        session_id: record.session_id.clone(),
        total_turns: record.turns.len(),
        retained_turns: retain,
        summarized_turns: summarize,
        reference_only_turns: reference_only,
        dropped_turns: dropped,
        entries,
    }
}

/// Build a single-line, character-bounded preview of `content`.
fn preview_line(
    content: &str,
    preview_chars: usize,
) -> String {
    let first_line = content
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("");

    let mut preview: String = first_line.chars().take(preview_chars).collect();
    if first_line.chars().count() > preview_chars {
        preview.push('\u{2026}');
    }
    preview
}

fn normalize_for_signature(content: &str) -> String {
    let mut normalized = String::with_capacity(content.len().min(512));
    let mut saw_space = false;

    for ch in content.chars() {
        if ch.is_whitespace() {
            if !saw_space {
                normalized.push(' ');
                saw_space = true;
            }
            continue;
        }
        saw_space = false;
        normalized.push(ch.to_ascii_lowercase());
        if normalized.len() >= 512 {
            break;
        }
    }

    normalized.trim().to_string()
}

fn is_repeated_state_check(turn: &SessionTurn) -> bool {
    matches!(
        turn.tool_name.as_deref(),
        Some(
            "run_in_terminal"
                | "get_terminal_output"
                | "terminal_last_command"
                | "file_search"
                | "grep_search"
                | "list_dir"
                | "get_changed_files"
                | "get_errors"
        )
    )
}

fn is_routine_retry_narration(
    turn: &SessionTurn,
    normalized_content: &str,
) -> bool {
    if turn.role != SessionRole::Assistant {
        return false;
    }

    if normalized_content.len() > 220 {
        return false;
    }

    let retry_markers = [
        "retry",
        "re-run",
        "rerun",
        "try again",
        "run again",
        "checking again",
    ];

    retry_markers
        .iter()
        .any(|marker| normalized_content.contains(marker))
}

fn extract_reference_pointer(content: &str) -> Option<String> {
    for line in content.lines() {
        let trimmed = line.trim();
        if let Some((_, right)) = trimmed.split_once("saved to:") {
            let pointer = right.trim();
            if !pointer.is_empty() {
                return Some(pointer.to_string());
            }
        }
        if let Some((_, right)) = trimmed.split_once("content at:") {
            let pointer = right.trim();
            if !pointer.is_empty() {
                return Some(pointer.to_string());
            }
        }
    }

    if content.contains("chat-session-resources") {
        return Some("chat-session-resource-pointer".to_string());
    }

    if content.contains(".session/sessions/") {
        return Some("session-store-pointer".to_string());
    }

    None
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::*;

    fn turn(
        sequence: usize,
        role: SessionRole,
        content: &str,
    ) -> SessionTurn {
        SessionTurn {
            sequence,
            role,
            content: content.to_string(),
            captured_at: Utc::now(),
            tool_name: None,
            event_meta: None,
        }
    }

    fn record_with(turns: Vec<SessionTurn>) -> SessionRecord {
        SessionRecord {
            schema_version: crate::SESSION_SCHEMA_VERSION,
            session_id: "sess-1".to_string(),
            source: "test".to_string(),
            started_at: Utc::now(),
            captured_at: Utc::now(),
            metadata: crate::SessionMetadata {
                workspace_slug: "default".to_string(),
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
            links: crate::SessionLinks::default(),
        }
    }

    #[test]
    fn peek_range_returns_full_window_by_default() {
        let record = record_with(vec![
            turn(0, SessionRole::User, "hello"),
            turn(1, SessionRole::Assistant, "world"),
        ]);

        let range = peek_turn_range(&record, 0, None);

        assert_eq!(range.total_turns, 2);
        assert_eq!(range.start, 0);
        assert_eq!(range.end, 2);
        assert_eq!(range.turns.len(), 2);
    }

    #[test]
    fn peek_range_clamps_out_of_bounds_indices() {
        let record = record_with(vec![
            turn(0, SessionRole::User, "a"),
            turn(1, SessionRole::Assistant, "b"),
            turn(2, SessionRole::User, "c"),
        ]);

        let range = peek_turn_range(&record, 5, Some(99));

        assert_eq!(range.start, 3);
        assert_eq!(range.end, 3);
        assert!(range.turns.is_empty());
    }

    #[test]
    fn peek_range_returns_inner_slice() {
        let record = record_with(vec![
            turn(0, SessionRole::User, "a"),
            turn(1, SessionRole::Assistant, "b"),
            turn(2, SessionRole::User, "c"),
        ]);

        let range = peek_turn_range(&record, 1, Some(2));

        assert_eq!(range.start, 1);
        assert_eq!(range.end, 2);
        assert_eq!(range.turns.len(), 1);
        assert_eq!(range.turns[0].content, "b");
    }

    #[test]
    fn peek_skeleton_strips_bodies_to_first_line() {
        let record = record_with(vec![turn(
            0,
            SessionRole::Assistant,
            "\n   first meaningful line\nsecond line\n",
        )]);

        let skeleton = peek_skeleton(&record, DEFAULT_SKELETON_PREVIEW_CHARS);

        assert_eq!(skeleton.total_turns, 1);
        assert_eq!(skeleton.entries[0].preview, "first meaningful line");
    }

    #[test]
    fn peek_skeleton_truncates_long_previews() {
        let long = "x".repeat(50);
        let record = record_with(vec![turn(0, SessionRole::User, &long)]);

        let skeleton = peek_skeleton(&record, 10);

        assert_eq!(skeleton.entries[0].content_len, 50);
        assert_eq!(skeleton.entries[0].preview.chars().count(), 11); // 10 + ellipsis
        assert!(skeleton.entries[0].preview.ends_with('\u{2026}'));
    }

    #[test]
    fn prompt_pack_drops_repeated_state_checks() {
        let mut first = turn(0, SessionRole::Tool, "status output");
        first.tool_name = Some("run_in_terminal".to_string());
        let mut second = turn(1, SessionRole::Tool, "status output");
        second.tool_name = Some("run_in_terminal".to_string());
        let record = record_with(vec![first, second]);

        let pack = peek_prompt_pack(&record, PromptPackOptions::default());

        assert_eq!(pack.total_turns, 2);
        assert_eq!(pack.entries.len(), 1);
        assert_eq!(pack.dropped_turns, 1);
        assert_eq!(pack.retained_turns, 1);
    }

    #[test]
    fn prompt_pack_marks_spill_paths_as_reference_only() {
        let mut tool = turn(
            0,
            SessionRole::Tool,
            "Large tool result written to file. Use the read_file tool to access the content at: /tmp/output.txt",
        );
        tool.tool_name = Some("run_in_terminal".to_string());
        let record = record_with(vec![tool]);

        let pack = peek_prompt_pack(&record, PromptPackOptions::default());

        assert_eq!(pack.reference_only_turns, 1);
        assert_eq!(pack.entries[0].inclusion, PromptInclusion::ReferenceOnly);
        assert_eq!(
            pack.entries[0].reference_pointer.as_deref(),
            Some("/tmp/output.txt")
        );
    }

    #[test]
    fn prompt_pack_summarizes_oversized_content() {
        let record = record_with(vec![turn(
            0,
            SessionRole::Assistant,
            &"x".repeat(800),
        )]);

        let pack = peek_prompt_pack(
            &record,
            PromptPackOptions {
                preview_chars: 40,
                summarize_threshold_chars: 120,
            },
        );

        assert_eq!(pack.summarized_turns, 1);
        assert_eq!(pack.entries[0].inclusion, PromptInclusion::Summarize);
    }

    #[test]
    fn prompt_pack_drops_routine_retry_narration() {
        let record = record_with(vec![turn(
            0,
            SessionRole::Assistant,
            "I will retry the same command and check again.",
        )]);

        let pack = peek_prompt_pack(&record, PromptPackOptions::default());

        assert_eq!(pack.entries.len(), 0);
        assert_eq!(pack.dropped_turns, 1);
    }

    #[test]
    fn prompt_pack_keeps_inline_blob_as_summarize_not_reference_only() {
        let record = record_with(vec![turn(
            0,
            SessionRole::Tool,
            &format!("inline payload: {}", "x".repeat(800)),
        )]);

        let pack = peek_prompt_pack(
            &record,
            PromptPackOptions {
                preview_chars: 40,
                summarize_threshold_chars: 120,
            },
        );

        assert_eq!(pack.summarized_turns, 1);
        assert_eq!(pack.reference_only_turns, 0);
        assert_eq!(pack.entries[0].inclusion, PromptInclusion::Summarize);
    }

    #[test]
    fn prompt_pack_drops_repeated_state_check_with_normalized_variants() {
        let mut first = turn(0, SessionRole::Tool, "Status   output\n");
        first.tool_name = Some("run_in_terminal".to_string());
        let mut second = turn(1, SessionRole::Tool, " status output ");
        second.tool_name = Some("run_in_terminal".to_string());
        let record = record_with(vec![first, second]);

        let pack = peek_prompt_pack(&record, PromptPackOptions::default());

        assert_eq!(pack.entries.len(), 1);
        assert_eq!(pack.dropped_turns, 1);
    }

    #[test]
    fn prompt_pack_retains_short_progress_preamble_variants() {
        let record = record_with(vec![
            turn(
                0,
                SessionRole::Assistant,
                "I will gather context and verify ticket drift status.",
            ),
            turn(
                1,
                SessionRole::Assistant,
                "Now I am checking spec and validation anchors.",
            ),
            turn(
                2,
                SessionRole::Assistant,
                "Next I will run tests after these edits.",
            ),
            turn(
                3,
                SessionRole::Assistant,
                "Durable finding: ambiguity markers should require explicit signals.",
            ),
        ]);

        let pack = peek_prompt_pack(&record, PromptPackOptions::default());

        assert_eq!(pack.total_turns, 4);
        assert_eq!(pack.dropped_turns, 0);
        assert_eq!(pack.entries.len(), 4);
    }

    #[test]
    fn prompt_pack_enforces_measurable_compactness_ratio_for_tool_output_noise() {
        let record = record_with(vec![
            turn(
                0,
                SessionRole::User,
                "Harden sync ambiguity labeling and add regression coverage.",
            ),
            turn(
                1,
                SessionRole::Tool,
                "status output",
            ),
            turn(
                2,
                SessionRole::Tool,
                "status output",
            ),
            turn(
                3,
                SessionRole::Tool,
                "status output",
            ),
            turn(
                4,
                SessionRole::Tool,
                "Large tool result written to file. Use the read_file tool to access the content at: /tmp/trace.txt",
            ),
            turn(
                5,
                SessionRole::Tool,
                &format!("inline payload: {}", "x".repeat(700)),
            ),
            turn(
                6,
                SessionRole::Assistant,
                "Durable finding: sync completions need explicit ambiguity signals.",
            ),
        ]);

        let mut record = record;
        record.turns[1].tool_name = Some("run_in_terminal".to_string());
        record.turns[2].tool_name = Some("run_in_terminal".to_string());
        record.turns[3].tool_name = Some("run_in_terminal".to_string());
        record.turns[4].tool_name = Some("run_in_terminal".to_string());
        record.turns[5].tool_name = Some("run_in_terminal".to_string());

        let pack = peek_prompt_pack(
            &record,
            PromptPackOptions {
                preview_chars: 80,
                summarize_threshold_chars: 120,
            },
        );

        let included = pack.entries.len();
        assert_eq!(pack.total_turns, 7);
        assert!(pack.dropped_turns >= 2);
        assert!(included <= 5);
        assert!(pack.reference_only_turns >= 1);
        assert!(pack.summarized_turns >= 1);
    }
}
