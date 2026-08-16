use chrono::{
    DateTime,
    Datelike,
    Timelike,
    Utc,
};
use serde_json::{
    Value,
    json,
};
use std::fmt::Write as FmtWrite;
use ticket_api::storage::board::{
    BoardConfig,
    BoardEntry,
    BoardEntryStatus,
    BoardHistorySnapshot,
    BoardSnapshot,
};
use uuid::Uuid;
pub(super) struct BoardDisplay {
    pub current_work: Vec<BoardDisplayEntry>,
    pub recommended_next: Vec<BoardRecommendation>,
    pub actions: Vec<String>,
}
pub(super) struct BoardHistoryDisplay {
    pub entries: Vec<BoardDisplayEntry>,
}
pub(super) struct BoardDisplayEntry {
    pub entry_id: Uuid,
    pub ticket_id: Uuid,
    pub title: String,
    pub state: Option<String>,
    pub agent_id: String,
    pub intent: String,
    pub status: String,
    pub heartbeat_age_secs: u64,
    pub owned_files: Vec<String>,
    pub handoff_reason: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    pub session_id: Option<String>,
    pub worktree_path: Option<String>,
    pub branch: Option<String>,
}
pub(crate) struct BoardRecommendation {
    pub rank: usize,
    pub ticket_id: String,
    pub title: String,
    pub state: Option<String>,
    pub priority: String,
    pub effort: Option<String>,
    pub dependency_count: usize,
    pub dependee_count: usize,
    pub became_actionable_at: Option<String>,
    pub created_at: String,
}
pub(super) fn entry_to_json(
    entry: &BoardEntry,
    config: &BoardConfig,
) -> Value {
    let age_secs = heartbeat_age_secs(entry, Utc::now());
    json!({
        "entry_id": entry.entry_id,
        "ticket_id": entry.ticket_id,
        "agent_id": entry.agent_id,
        "intent": entry.intent,
        "status": entry_status(entry, config, age_secs),
        "checked_in_at": entry.checked_in_at,
        "last_heartbeat": entry.last_heartbeat,
        "heartbeat_age_secs": age_secs,
        "ttl_secs": entry.ttl_secs,
        "owned_files": entry.owned_files,
        "handoff_reason": entry.handoff_reason,
        "session_id": entry.session_id,
        "worktree_path": entry.worktree_path,
        "branch": entry.branch,
    })
}
pub(super) fn config_to_json(config: &BoardConfig) -> Value {
    json!({
        "max_wip": config.max_wip,
        "stale_after_secs": config.stale_after_secs,
        "completed_audit_window_secs": config.completed_audit_window_secs,
    })
}
pub(super) fn board_display_entry_to_json(entry: &BoardDisplayEntry) -> Value {
    json!({
        "entry_id": entry.entry_id,
        "ticket_id": entry.ticket_id,
        "ticket_short": short_ticket_id(&entry.ticket_id),
        "title": entry.title,
        "state": entry.state,
        "agent_id": entry.agent_id,
        "intent": entry.intent,
        "status": entry.status,
        "heartbeat_age_secs": entry.heartbeat_age_secs,
        "owned_files": entry.owned_files,
        "owned_file_count": entry.owned_files.len(),
        "handoff_reason": entry.handoff_reason,
        "completed_at": entry.completed_at,
        "session_id": entry.session_id,
        "worktree_path": entry.worktree_path,
        "branch": entry.branch,
    })
}
pub(super) fn board_recommendation_to_json(
    recommendation: &BoardRecommendation
) -> Value {
    json!({
        "rank": recommendation.rank,
        "ticket_id": recommendation.ticket_id,
        "ticket_short": short_ticket_value(&recommendation.ticket_id),
        "title": recommendation.title,
        "state": recommendation.state,
        "priority": recommendation.priority,
        "effort": recommendation.effort,
        "dependency_count": recommendation.dependency_count,
        "dependee_count": recommendation.dependee_count,
        "became_actionable_at": recommendation.became_actionable_at,
        "created_at": recommendation.created_at,
    })
}
pub(super) fn render_board_human(
    snap: &BoardSnapshot,
    display: &BoardDisplay,
) -> String {
    let mut out = String::new();
    write_summary(&mut out, snap);
    write_actions(&mut out, &display.actions);
    write_current_work(&mut out, &display.current_work);
    write_next_up(&mut out, &display.recommended_next);
    write_warnings(&mut out, &snap.warnings);
    write_file_ownership(&mut out, &snap.file_ownership);
    out
}
pub(super) fn render_board_history_human(
    snap: &BoardHistorySnapshot,
    display: &BoardHistoryDisplay,
) -> String {
    let mut out = String::new();
    write_history_summary(&mut out, snap);
    write_history_entries(&mut out, &display.entries);
    out
}
fn write_summary(
    out: &mut String,
    snap: &BoardSnapshot,
) {
    let _ = writeln!(
        out,
        "Board: [{}/{} active] [{} stale{}] [{} conflict{}]",
        snap.active_count,
        snap.config.max_wip,
        snap.stale_count,
        warning_suffix(snap.stale_count),
        snap.conflict_count,
        warning_suffix(snap.conflict_count),
    );
}
fn warning_suffix(count: u32) -> &'static str {
    if count > 0 { " ⚠" } else { "" }
}
fn write_actions(
    out: &mut String,
    actions: &[String],
) {
    if actions.is_empty() {
        return;
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "Immediate Actions:");
    for (index, action) in actions.iter().enumerate() {
        let _ = writeln!(out, "  {}. {action}", index + 1);
    }
}
fn write_current_work(
    out: &mut String,
    current_work: &[BoardDisplayEntry],
) {
    let _ = writeln!(out);
    let _ = writeln!(out, "Current Work:");
    if current_work.is_empty() {
        let _ = writeln!(out, "  (no active board entries)");
        return;
    }
    let has_worktree_metadata = current_work
        .iter()
        .any(|entry| entry.worktree_path.is_some() || entry.branch.is_some());
    if has_worktree_metadata {
        let _ = writeln!(
            out,
            "  {:<10}  {:<8}  {:<24}  {:<14}  {:<16}  {:<16}  {:<16}  {:<18}  {:>6}",
            "STATUS",
            "TICKET",
            "TITLE",
            "AGENT",
            "SESSION",
            "BRANCH",
            "WORKTREE",
            "INTENT",
            "HB AGE"
        );
        let _ = writeln!(out, "  {}", "-".repeat(150));
        for entry in current_work {
            let worktree = entry
                .worktree_path
                .as_deref()
                .map(short_worktree_indicator)
                .unwrap_or_default();
            let _ = writeln!(
                out,
                "  {:<10}  {:<8}  {:<24}  {:<14}  {:<16}  {:<16}  {:<16}  {:<18}  {:>6}",
                truncate_field(&entry.status, 10),
                short_ticket_id(&entry.ticket_id),
                truncate_field(&entry.title, 24),
                truncate_field(&entry.agent_id, 14),
                truncate_field(entry.session_id.as_deref().unwrap_or(""), 16),
                truncate_field(entry.branch.as_deref().unwrap_or(""), 16),
                truncate_field(worktree, 16),
                truncate_field(&entry.intent, 18),
                entry.heartbeat_age_secs,
            );
        }
        return;
    }
    let _ = writeln!(
        out,
        "  {:<10}  {:<8}  {:<34}  {:<18}  {:<20}  {:>10}",
        "STATUS", "TICKET", "TITLE", "AGENT", "INTENT", "HB AGE"
    );
    let _ = writeln!(out, "  {}", "-".repeat(112));
    for entry in current_work {
        let _ = writeln!(
            out,
            "  {:<10}  {:<8}  {:<34}  {:<18}  {:<20}  {:>10}",
            truncate_field(&entry.status, 10),
            short_ticket_id(&entry.ticket_id),
            truncate_field(&entry.title, 34),
            truncate_field(&entry.agent_id, 18),
            truncate_field(&entry.intent, 20),
            entry.heartbeat_age_secs,
        );
    }
}

fn short_worktree_indicator(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}
pub(crate) fn write_next_up(
    out: &mut String,
    recommended_next: &[BoardRecommendation],
) {
    let _ = writeln!(out);
    let _ = writeln!(out, "Next Up:");
    if recommended_next.is_empty() {
        let _ = writeln!(out, "  (no unblocked tickets ready right now)");
        return;
    }
    for (index, recommendation) in recommended_next.iter().enumerate() {
        if index > 0 {
            let _ = writeln!(out);
        } else {
            let _ = writeln!(out);
        }
        let _ = writeln!(
            out,
            "  #{}  {}  {}",
            recommendation.rank,
            short_ticket_value(&recommendation.ticket_id),
            recommendation.title,
        );
        let _ = writeln!(
            out,
            "  state: {}  priority: {}  effort: {}  dependee_count: {}  dependency_count: {}",
            recommendation.state.as_deref().unwrap_or("-"),
            recommendation.priority,
            recommendation.effort.as_deref().unwrap_or("-"),
            recommendation.dependee_count,
            recommendation.dependency_count,
        );
        let _ = writeln!(
            out,
            "  created_at: {}",
            format_pretty_created_at(&recommendation.created_at),
        );
        let _ = writeln!(out, "  ticket_id: {}", recommendation.ticket_id,);
    }
}
fn write_history_summary(
    out: &mut String,
    snap: &BoardHistorySnapshot,
) {
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Board History: [{} completion{} in window]",
        snap.completed_count,
        if snap.completed_count == 1 { "" } else { "s" }
    );
    if snap.config.completed_audit_window_secs == 0 {
        let _ = writeln!(out, "Window: all recorded completion history");
    } else {
        let _ = writeln!(
            out,
            "Window: last {} second{}",
            snap.config.completed_audit_window_secs,
            if snap.config.completed_audit_window_secs == 1 {
                ""
            } else {
                "s"
            }
        );
    }
    if snap.hidden_completed_count > 0 {
        let _ = writeln!(
            out,
            "Older hidden: {} completion{} outside the history window",
            snap.hidden_completed_count,
            if snap.hidden_completed_count == 1 {
                ""
            } else {
                "s"
            }
        );
    }
}
fn write_history_entries(
    out: &mut String,
    entries: &[BoardDisplayEntry],
) {
    let _ = writeln!(out);
    let _ = writeln!(out, "Completed Work:");
    if entries.is_empty() {
        let _ = writeln!(
            out,
            "  (no completed board history in the current window)"
        );
        return;
    }
    let _ = writeln!(
        out,
        "  {:<8}  {:<34}  {:<18}  {:<20}  {:<36}",
        "TICKET", "TITLE", "AGENT", "COMPLETED", "HANDOFF"
    );
    let _ = writeln!(out, "  {}", "-".repeat(112));
    for entry in entries {
        let _ = writeln!(
            out,
            "  {:<8}  {:<34}  {:<18}  {:<20}  {:<36}",
            short_ticket_id(&entry.ticket_id),
            truncate_field(&entry.title, 34),
            truncate_field(&entry.agent_id, 18),
            truncate_field(&format_completed_at(entry.completed_at), 20),
            truncate_field(
                entry
                    .handoff_reason
                    .as_deref()
                    .unwrap_or("handoff reason not recorded"),
                36,
            ),
        );
    }
}
fn write_warnings(
    out: &mut String,
    warnings: &[String],
) {
    if warnings.is_empty() {
        return;
    }
    let _ = writeln!(out);
    for warning in warnings {
        let _ = writeln!(out, "  ⚠  {warning}");
    }
}
fn write_file_ownership(
    out: &mut String,
    file_ownership: &std::collections::BTreeMap<String, Vec<String>>,
) {
    if file_ownership.is_empty() {
        return;
    }
    let _ = writeln!(out);
    let _ = writeln!(out, "File Ownership:");
    for (path, agents) in file_ownership {
        let _ = writeln!(out, "  {path}  →  {}", agents.join(", "));
    }
}
pub(super) fn heartbeat_age_secs(
    entry: &BoardEntry,
    now: DateTime<Utc>,
) -> u64 {
    (now - entry.last_heartbeat).num_seconds().max(0) as u64
}
pub(super) fn entry_status(
    entry: &BoardEntry,
    config: &BoardConfig,
    age_secs: u64,
) -> &'static str {
    if entry.status == BoardEntryStatus::Active
        && age_secs > config.stale_after_secs
    {
        return "stale";
    }
    match &entry.status {
        BoardEntryStatus::Active => "active",
        BoardEntryStatus::Stale => "stale",
        BoardEntryStatus::Conflict => "conflict",
        BoardEntryStatus::Completed => "completed",
    }
}
fn short_ticket_id(ticket_id: &Uuid) -> String {
    ticket_id.simple().to_string().chars().take(8).collect()
}
fn short_ticket_value(ticket_id: &str) -> String {
    ticket_id.chars().take(8).collect()
}
fn format_pretty_created_at(created_at: &str) -> String {
    let Ok(timestamp) = DateTime::parse_from_rfc3339(created_at) else {
        return created_at.to_string();
    };
    let timestamp = timestamp.with_timezone(&Utc);
    let month = timestamp.format("%b");
    format!(
        "{month} {} {} {:02}:{:02} UTC",
        timestamp.day(),
        timestamp.year(),
        timestamp.hour(),
        timestamp.minute()
    )
}
fn truncate_field(
    value: &str,
    width: usize,
) -> String {
    if value.len() > width {
        format!("{}…", &value[..width - 1])
    } else {
        value.to_string()
    }
}
fn format_completed_at(completed_at: Option<DateTime<Utc>>) -> String {
    completed_at
        .map(|timestamp| timestamp.format("%Y-%m-%d %H:%M").to_string())
        .unwrap_or_else(|| "-".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_board_render_includes_session_id() {
        let ticket_id = Uuid::new_v4();
        let snapshot = BoardSnapshot {
            captured_at: Utc::now(),
            entries: Vec::new(),
            caller_entries: Vec::new(),
            config: BoardConfig::default(),
            active_count: 1,
            stale_count: 0,
            conflict_count: 0,
            wip_limit_reached: false,
            file_ownership: Default::default(),
            active_worktrees: Vec::new(),
            warnings: Vec::new(),
        };
        let display = BoardDisplay {
            current_work: vec![BoardDisplayEntry {
                entry_id: Uuid::new_v4(),
                ticket_id,
                title: "Session metadata".to_string(),
                state: None,
                agent_id: "agent-a".to_string(),
                intent: "work".to_string(),
                status: "active".to_string(),
                heartbeat_age_secs: 0,
                owned_files: Vec::new(),
                handoff_reason: None,
                completed_at: None,
                session_id: Some("session-a".to_string()),
                worktree_path: Some("/tmp/worktree-a".to_string()),
                branch: Some("agent/metadata".to_string()),
            }],
            recommended_next: Vec::new(),
            actions: Vec::new(),
        };

        let rendered = render_board_human(&snapshot, &display);

        assert!(rendered.contains("SESSION"));
        assert!(rendered.contains("session-a"));
    }
}
