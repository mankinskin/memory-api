use std::fmt::Write as FmtWrite;

use chrono::{
    DateTime,
    Utc,
};
use serde_json::{
    Value,
    json,
};
use uuid::Uuid;

use ticket_api::storage::board::{
    BoardConfig,
    BoardEntry,
    BoardEntryStatus,
    BoardHistorySnapshot,
    BoardSnapshot,
};

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
}

pub(super) struct BoardRecommendation {
    pub rank: usize,
    pub ticket_id: String,
    pub title: String,
    pub state: Option<String>,
    pub priority: String,
    pub dependency_count: usize,
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
    })
}

pub(super) fn board_recommendation_to_json(
    recommendation: &BoardRecommendation,
) -> Value {
    json!({
        "rank": recommendation.rank,
        "ticket_id": recommendation.ticket_id,
        "ticket_short": short_ticket_value(&recommendation.ticket_id),
        "title": recommendation.title,
        "state": recommendation.state,
        "priority": recommendation.priority,
        "dependency_count": recommendation.dependency_count,
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

fn write_next_up(
    out: &mut String,
    recommended_next: &[BoardRecommendation],
) {
    let _ = writeln!(out);
    let _ = writeln!(out, "Next Up:");

    if recommended_next.is_empty() {
        let _ = writeln!(out, "  (no unblocked tickets ready right now)");
        return;
    }

    let _ = writeln!(
        out,
        "  {:<4}  {:<8}  {:<34}  {:<12}  {:<10}  {:>4}",
        "RANK", "TICKET", "TITLE", "STATE", "PRIORITY", "DEPS"
    );
    let _ = writeln!(out, "  {}", "-".repeat(86));

    for recommendation in recommended_next {
        let _ = writeln!(
            out,
            "  {:<4}  {:<8}  {:<34}  {:<12}  {:<10}  {:>4}",
            recommendation.rank,
            short_ticket_value(&recommendation.ticket_id),
            truncate_field(&recommendation.title, 34),
            truncate_field(recommendation.state.as_deref().unwrap_or("-"), 12),
            truncate_field(&recommendation.priority, 10),
            recommendation.dependency_count,
        );
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
            if snap.config.completed_audit_window_secs == 1 { "" } else { "s" }
        );
    }

    if snap.hidden_completed_count > 0 {
        let _ = writeln!(
            out,
            "Older hidden: {} completion{} outside the history window",
            snap.hidden_completed_count,
            if snap.hidden_completed_count == 1 { "" } else { "s" }
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
        let _ = writeln!(out, "  (no completed board history in the current window)");
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
    ticket_id
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect()
}

fn short_ticket_value(ticket_id: &str) -> String {
    ticket_id.chars().take(8).collect()
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
