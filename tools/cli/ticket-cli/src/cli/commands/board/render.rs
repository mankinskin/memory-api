use std::fmt::Write as FmtWrite;

use chrono::{
    DateTime,
    Utc,
};
use serde_json::{
    Value,
    json,
};

use ticket_api::storage::board::{
    BoardConfig,
    BoardEntry,
    BoardEntryStatus,
    BoardSnapshot,
};

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

pub(super) fn render_board_human(snap: &BoardSnapshot) -> String {
    let mut out = String::new();

    write_summary(&mut out, snap);
    if snap.entries.is_empty() {
        let _ = writeln!(out, "  (no board entries)");
        return out;
    }

    let now = Utc::now();
    write_table(&mut out, snap, now);
    write_warnings(&mut out, &snap.warnings);
    write_file_ownership(&mut out, &snap.file_ownership);

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

fn write_table(
    out: &mut String,
    snap: &BoardSnapshot,
    now: DateTime<Utc>,
) {
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "  {:<10}  {:<36}  {:<18}  {:<20}  {:>12}  {:<10}",
        "TICKET", "ENTRY ID", "AGENT", "INTENT", "HB AGE (s)", "STATUS"
    );
    let _ = writeln!(out, "  {}", "-".repeat(120));

    for entry in &snap.entries {
        write_entry_row(out, entry, &snap.config, now);
    }
}

fn write_entry_row(
    out: &mut String,
    entry: &BoardEntry,
    config: &BoardConfig,
    now: DateTime<Utc>,
) {
    let age_secs = heartbeat_age_secs(entry, now);
    let _ = writeln!(
        out,
        "  {:<10}  {:<36}  {:<18}  {:<20}  {:>12}  {:<10}",
        short_ticket(entry),
        entry.entry_id,
        truncate_field(&entry.agent_id, 18),
        truncate_field(&entry.intent, 20),
        age_secs,
        entry_status(entry, config, age_secs)
    );
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

fn heartbeat_age_secs(
    entry: &BoardEntry,
    now: DateTime<Utc>,
) -> u64 {
    (now - entry.last_heartbeat).num_seconds().max(0) as u64
}

fn entry_status(
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

fn short_ticket(entry: &BoardEntry) -> String {
    entry
        .ticket_id
        .simple()
        .to_string()
        .chars()
        .take(8)
        .collect()
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
