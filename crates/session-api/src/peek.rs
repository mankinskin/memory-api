use serde::{
    Deserialize,
    Serialize,
};

use crate::{
    SessionRecord,
    SessionRole,
    SessionTurn,
};

/// Default number of preview characters retained per turn in a skeleton view.
pub const DEFAULT_SKELETON_PREVIEW_CHARS: usize = 120;

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
        }
    }

    fn record_with(turns: Vec<SessionTurn>) -> SessionRecord {
        SessionRecord {
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
}
