use std::collections::HashMap;

use chrono::Utc;
use serde_json::{
    Value,
    json,
};
use uuid::Uuid;

use ticket_api::storage::{
    TicketStore,
    board::{
        BoardConfig,
        BoardEntry,
        BoardEntryStatus,
        BoardError,
        BoardHistorySnapshot,
        BoardSnapshot,
    },
};

use crate::cli::{
    BoardArgs,
    BoardCleanCommand,
    BoardCommand,
    CliRunError,
    NextArgs,
};

use super::resolve_uuid_prefix;

mod render;

use self::render::{
    BoardDisplay,
    BoardDisplayEntry,
    BoardHistoryDisplay,
    BoardRecommendation,
    board_display_entry_to_json,
    board_recommendation_to_json,
    config_to_json,
    entry_status,
    entry_to_json,
    heartbeat_age_secs,
    render_board_history_human,
    render_board_human,
};

const BOARD_RECOMMENDATIONS_LIMIT: usize = 10;

// ── entry point ────────────────────────────────────────────────────────────────

pub(crate) fn cmd_board(
    args: BoardArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    match args.command {
        BoardCommand::Show { agent } => cmd_board_show(agent.as_deref(), store),
        BoardCommand::History { agent } =>
            cmd_board_history(agent.as_deref(), store),
        BoardCommand::CheckIn {
            id,
            agent,
            intent,
            files,
            ttl_secs,
        } => cmd_board_check_in(id, agent, intent, files, ttl_secs, store),
        BoardCommand::CheckOut { id, agent, reason } =>
            cmd_board_check_out(id, agent, reason, store),
        BoardCommand::Heartbeat { entry_id } =>
            cmd_board_heartbeat(entry_id, store),
        BoardCommand::Configure {
            max_wip,
            stale_after_secs,
            completed_audit_window_secs,
        } => cmd_board_configure(
            max_wip,
            stale_after_secs,
            completed_audit_window_secs,
            store,
        ),
        BoardCommand::Clean(clean_args) => match clean_args.command {
            BoardCleanCommand::Preview { include_stale } =>
                cmd_board_clean_preview(include_stale, store),
            BoardCleanCommand::Apply {
                token,
                include_stale,
            } => cmd_board_clean_apply(token, include_stale, store),
        },
        BoardCommand::UpdateFiles {
            id,
            agent,
            add,
            remove,
        } => cmd_board_update_files(id, agent, add, remove, store),
        BoardCommand::RenameFile {
            id,
            agent,
            from,
            to,
        } => cmd_board_rename_file(id, agent, from, to, store),
    }
}

// ── show ──────────────────────────────────────────────────────────────────────

fn cmd_board_show(
    agent: Option<&str>,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let mut snap = store.board_show(agent)?;

    // When an agent is supplied, also refresh heartbeats for that agent's active
    // entries so the show itself acts as a heartbeat signal, then re-snapshot.
    if let Some(agent_id) = agent {
        let active_entry_ids: Vec<Uuid> = snap
            .caller_entries
            .iter()
            .filter(|entry| entry.status == BoardEntryStatus::Active)
            .map(|entry| entry.entry_id)
            .collect();

        for entry_id in &active_entry_ids {
            // Non-fatal: stale entries may already be gone.
            let _ = store.board_heartbeat(entry_id);
        }

        if !active_entry_ids.is_empty() {
            snap = store.board_show(Some(agent_id))?;
        }
    }

    let entries: Vec<Value> = snap
        .entries
        .iter()
        .map(|entry| entry_to_json(entry, &snap.config))
        .collect();
    let display = build_board_display(&snap, store)?;
    let current_work: Vec<Value> = display
        .current_work
        .iter()
        .map(board_display_entry_to_json)
        .collect();
    let recommended_next: Vec<Value> = display
        .recommended_next
        .iter()
        .map(board_recommendation_to_json)
        .collect();
    let actions = display.actions.clone();
    let human = render_board_human(&snap, &display);
    let file_ownership: Value = json!(snap.file_ownership);

    Ok(json!({
        "command": "board_show",
        "status": "ok",
        "captured_at": snap.captured_at,
        "active_count": snap.active_count,
        "stale_count": snap.stale_count,
        "conflict_count": snap.conflict_count,
        "wip_limit_reached": snap.wip_limit_reached,
        "config": config_to_json(&snap.config),
        "entries": entries,
        "current_work": current_work,
        "recommended_next": recommended_next,
        "actions": actions,
        "warnings": snap.warnings,
        "file_ownership": file_ownership,
        "human": human,
    }))
}

fn cmd_board_history(
    agent: Option<&str>,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let snap = store.board_history(agent)?;
    let display = build_board_history_display(&snap, store)?;
    let entries: Vec<Value> = display
        .entries
        .iter()
        .map(board_display_entry_to_json)
        .collect();
    let human = render_board_history_human(&snap, &display);

    Ok(json!({
        "command": "board_history",
        "status": "ok",
        "captured_at": snap.captured_at,
        "completed_count": snap.completed_count,
        "hidden_completed_count": snap.hidden_completed_count,
        "history_window_secs": snap.config.completed_audit_window_secs,
        "config": config_to_json(&snap.config),
        "entries": entries,
        "human": human,
    }))
}

struct TicketSummary {
    title: Option<String>,
    state: Option<String>,
}

fn build_board_display(
    snap: &BoardSnapshot,
    store: &TicketStore,
) -> Result<BoardDisplay, CliRunError> {
    let ticket_summaries = load_ticket_summaries(store)?;

    let mut current_work: Vec<&BoardEntry> = snap
        .entries
        .iter()
        .filter(|entry| is_current_work_status(&entry.status))
        .collect();
    current_work.sort_by(|left, right| {
        current_work_priority(&left.status)
            .cmp(&current_work_priority(&right.status))
            .then_with(|| right.checked_in_at.cmp(&left.checked_in_at))
    });

    let current_work: Vec<BoardDisplayEntry> = current_work
        .into_iter()
        .map(|entry| build_display_entry(entry, &snap.config, &ticket_summaries))
        .collect();

    let next_payload = super::cmd_next(
        NextArgs {
            limit: BOARD_RECOMMENDATIONS_LIMIT,
            filter: None,
            no_board: false,
        },
        store,
    )?;
    let recommended_next = next_payload["items"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(parse_board_recommendation)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let actions = build_actions(snap, &current_work, &recommended_next);

    Ok(BoardDisplay {
        current_work,
        recommended_next,
        actions,
    })
}

fn build_board_history_display(
    snap: &BoardHistorySnapshot,
    store: &TicketStore,
) -> Result<BoardHistoryDisplay, CliRunError> {
    let ticket_summaries = load_ticket_summaries(store)?;
    let entries = snap
        .entries
        .iter()
        .map(|entry| build_display_entry(entry, &snap.config, &ticket_summaries))
        .collect();

    Ok(BoardHistoryDisplay { entries })
}

fn load_ticket_summaries(
    store: &TicketStore,
) -> Result<HashMap<Uuid, TicketSummary>, CliRunError> {
    Ok(store
        .list(None, None, None)?
        .into_iter()
        .map(|ticket| {
            (
                ticket.id,
                TicketSummary {
                    title: ticket.title,
                    state: ticket.state,
                },
            )
        })
        .collect())
}

fn build_display_entry(
    entry: &BoardEntry,
    config: &BoardConfig,
    ticket_summaries: &HashMap<Uuid, TicketSummary>,
) -> BoardDisplayEntry {
    let age_secs = heartbeat_age_secs(entry, Utc::now());
    let summary = ticket_summaries.get(&entry.ticket_id);

    BoardDisplayEntry {
        entry_id: entry.entry_id,
        ticket_id: entry.ticket_id,
        title: summary
            .and_then(|ticket| ticket.title.clone())
            .unwrap_or_else(|| "(untitled ticket)".to_string()),
        state: summary.and_then(|ticket| ticket.state.clone()),
        agent_id: entry.agent_id.clone(),
        intent: non_empty_or_default(&entry.intent, "no intent recorded"),
        status: entry_status(entry, config, age_secs).to_string(),
        heartbeat_age_secs: age_secs,
        owned_files: entry.owned_files.clone(),
        handoff_reason: entry.handoff_reason.clone(),
        completed_at: history_completed_at(entry),
    }
}

fn parse_board_recommendation(value: &Value) -> Option<BoardRecommendation> {
    Some(BoardRecommendation {
        rank: value.get("rank")?.as_u64()? as usize,
        ticket_id: value.get("id")?.as_str()?.to_string(),
        title: value
            .get("title")
            .and_then(Value::as_str)
            .filter(|title| !title.trim().is_empty())
            .unwrap_or("(untitled ticket)")
            .to_string(),
        state: value
            .get("state")
            .and_then(Value::as_str)
            .map(str::to_string),
        priority: value
            .get("priority")
            .and_then(Value::as_str)
            .unwrap_or("none")
            .to_string(),
        dependency_count: value
            .get("dependency_count")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize,
        dependees: value
            .get("dependees")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize,
        created_at: value
            .get("created_at")
            .and_then(Value::as_str)
            .unwrap_or("-")
            .to_string(),
    })
}

fn build_actions(
    snap: &BoardSnapshot,
    current_work: &[BoardDisplayEntry],
    recommended_next: &[BoardRecommendation],
) -> Vec<String> {
    let mut actions = Vec::new();

    if snap.conflict_count > 0 {
        actions.push(format!(
            "Resolve {} conflicting board entr{} before taking more work.",
            snap.conflict_count,
            plural_suffix(snap.conflict_count)
        ));
    }

    if snap.stale_count > 0 {
        actions.push(format!(
            "Review {} stale entr{} now. Heartbeat live work or run 'ticket board clean preview --include-stale' if the ownership is abandoned.",
            snap.stale_count,
            plural_suffix(snap.stale_count)
        ));
    }

    if snap.wip_limit_reached {
        actions.push(format!(
            "WIP is full at {}/{} active or stale entries. Reduce the board before starting additional work.",
            snap.active_count + snap.stale_count,
            snap.config.max_wip
        ));
    } else if let Some(next) = recommended_next.first() {
        let short_ticket = short_ticket_value(&next.ticket_id);
        if current_work.is_empty() {
            actions.push(format!(
                "Board is clear. Start {short_ticket} {} next.",
                next.title
            ));
        } else {
            actions.push(format!(
                "When you free capacity, start {short_ticket} {} next.",
                next.title
            ));
        }
    } else if current_work.is_empty() {
        actions.push(
            "Board is clear, but there are no unblocked tickets ready right now."
                .to_string(),
        );
    } else {
        actions.push(
            "No additional unblocked tickets are ready once the current board work finishes."
                .to_string(),
        );
    }

    actions
}

fn is_current_work_status(status: &BoardEntryStatus) -> bool {
    matches!(
        status,
        BoardEntryStatus::Active | BoardEntryStatus::Stale | BoardEntryStatus::Conflict
    )
}

fn current_work_priority(status: &BoardEntryStatus) -> u8 {
    match status {
        BoardEntryStatus::Conflict => 0,
        BoardEntryStatus::Stale => 1,
        BoardEntryStatus::Active => 2,
        BoardEntryStatus::Completed => 3,
    }
}

fn non_empty_or_default(
    value: &str,
    default: &str,
) -> String {
    if value.trim().is_empty() {
        default.to_string()
    } else {
        value.to_string()
    }
}

fn plural_suffix(count: u32) -> &'static str {
    if count == 1 { "y" } else { "ies" }
}

fn short_ticket_value(ticket_id: &str) -> String {
    ticket_id.chars().take(8).collect()
}

fn history_completed_at(entry: &BoardEntry) -> Option<chrono::DateTime<Utc>> {
    (entry.status == BoardEntryStatus::Completed)
        .then(|| entry.completed_at.unwrap_or(entry.last_heartbeat.max(entry.checked_in_at)))
}

// ── check-in ──────────────────────────────────────────────────────────────────

fn cmd_board_check_in(
    id: String,
    agent: String,
    intent: Option<String>,
    files: Vec<String>,
    ttl_secs: Option<u64>,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let ticket_id = resolve_uuid_prefix(&id, store)?;
    let ttl = ttl_secs.unwrap_or(3600);
    let intent_str = intent.as_deref().unwrap_or("");

    let entry = store
        .board_check_in(&ticket_id, &agent, ttl, intent_str, files)
        .map_err(board_err_to_cli)?;

    Ok(json!({
        "command": "board_check_in",
        "status": "ok",
        "entry_id": entry.entry_id,
        "ticket_id": entry.ticket_id,
        "agent_id": entry.agent_id,
        "intent": entry.intent,
        "owned_files": entry.owned_files,
        "checked_in_at": entry.checked_in_at,
        "ttl_secs": entry.ttl_secs,
    }))
}

// ── check-out ─────────────────────────────────────────────────────────────────

fn cmd_board_check_out(
    id: String,
    agent: Option<String>,
    reason: Option<String>,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let ticket_id = resolve_uuid_prefix(&id, store)?;

    // Resolve agent: use supplied agent or fall back to any active agent on the ticket.
    let resolved_agent = if let Some(agent_id) = agent {
        agent_id
    } else {
        let snap = store.board_show(None)?;
        snap.entries
            .into_iter()
            .find(|entry| {
                entry.ticket_id == ticket_id
                    && entry.status == BoardEntryStatus::Active
            })
            .map(|entry| entry.agent_id)
            .ok_or_else(|| {
                CliRunError::BadRequest(format!(
                    "no active board entry found for ticket {ticket_id}; \
                     use --agent to specify the agent to check out"
                ))
            })?
    };

    let entry = store
        .board_check_out(&ticket_id, &resolved_agent, reason.as_deref())
        .map_err(board_err_to_cli)?;

    Ok(json!({
        "command": "board_check_out",
        "status": "ok",
        "entry_id": entry.entry_id,
        "ticket_id": entry.ticket_id,
        "agent_id": entry.agent_id,
        "handoff_reason": entry.handoff_reason,
        "status_field": "completed",
    }))
}

// ── heartbeat ─────────────────────────────────────────────────────────────────

fn cmd_board_heartbeat(
    entry_id: String,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let entry_id = entry_id.parse::<Uuid>().map_err(|_| {
        CliRunError::BadRequest(format!(
            "invalid entry_id '{entry_id}': expected a UUID"
        ))
    })?;

    let entry = store
        .board_heartbeat(&entry_id)
        .map_err(board_err_to_cli)?;

    Ok(json!({
        "command": "board_heartbeat",
        "status": "ok",
        "entry_id": entry.entry_id,
        "ticket_id": entry.ticket_id,
        "agent_id": entry.agent_id,
        "last_heartbeat": entry.last_heartbeat,
    }))
}

// ── configure ─────────────────────────────────────────────────────────────────

fn cmd_board_configure(
    max_wip: Option<u32>,
    stale_after_secs: Option<u64>,
    completed_audit_window_secs: Option<u64>,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let new_config = if max_wip.is_none()
        && stale_after_secs.is_none()
        && completed_audit_window_secs.is_none()
    {
        None
    } else {
        let current = store.board_configure(None).map_err(board_err_to_cli)?;
        Some(BoardConfig {
            max_wip: max_wip.unwrap_or(current.max_wip),
            stale_after_secs: stale_after_secs
                .unwrap_or(current.stale_after_secs),
            completed_audit_window_secs: completed_audit_window_secs
                .unwrap_or(current.completed_audit_window_secs),
        })
    };

    let config = store
        .board_configure(new_config)
        .map_err(board_err_to_cli)?;

    Ok(json!({
        "command": "board_configure",
        "status": "ok",
        "config": config_to_json(&config),
    }))
}

// ── clean preview ─────────────────────────────────────────────────────────────

fn cmd_board_clean_preview(
    include_stale: bool,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let preview = store
        .board_clean_preview(include_stale)
        .map_err(board_err_to_cli)?;

    Ok(json!({
        "command": "board_clean_preview",
        "status": "ok",
        "token": preview.token,
        "entry_count": preview.entry_count,
        "entry_ids": preview.entry_ids,
        "include_stale": preview.include_stale,
        "generated_at": preview.generated_at,
    }))
}

// ── clean apply ───────────────────────────────────────────────────────────────

fn cmd_board_clean_apply(
    token: String,
    include_stale: bool,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let result = store
        .board_clean_apply(&token, include_stale)
        .map_err(board_err_to_cli)?;

    Ok(json!({
        "command": "board_clean_apply",
        "status": "ok",
        "removed_count": result.removed_count,
        "removed_entry_ids": result.removed_entry_ids,
    }))
}

// ── update-files ──────────────────────────────────────────────────────────────

fn cmd_board_update_files(
    id: String,
    agent: String,
    add: Vec<String>,
    remove: Vec<String>,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let ticket_id = resolve_uuid_prefix(&id, store)?;
    let entry = store
        .board_update_files(&ticket_id, &agent, add, remove)
        .map_err(board_err_to_cli)?;

    Ok(json!({
        "command": "board_update_files",
        "status": "ok",
        "entry_id": entry.entry_id,
        "ticket_id": entry.ticket_id,
        "agent_id": entry.agent_id,
        "owned_files": entry.owned_files,
    }))
}

// ── rename-file ───────────────────────────────────────────────────────────────

fn cmd_board_rename_file(
    id: String,
    agent: String,
    from: String,
    to: String,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let ticket_id = resolve_uuid_prefix(&id, store)?;
    let entry = store
        .board_rename_file(&ticket_id, &agent, &from, &to)
        .map_err(board_err_to_cli)?;

    Ok(json!({
        "command": "board_rename_file",
        "status": "ok",
        "entry_id": entry.entry_id,
        "ticket_id": entry.ticket_id,
        "agent_id": entry.agent_id,
        "owned_files": entry.owned_files,
    }))
}

// ── error mapping ─────────────────────────────────────────────────────────────

fn board_err_to_cli(err: BoardError) -> CliRunError {
    match &err {
        BoardError::WipLimitReached { current, max } => {
            CliRunError::BadRequest(format!(
                "WIP limit reached: {current}/{max} active entries — check out a ticket or raise the limit with `board configure --max-wip`"
            ))
        }
        BoardError::FileConflict {
            files,
            conflicting_agent,
            conflicting_ticket,
        } => CliRunError::BadRequest(format!(
            "file conflict: {files:?} already owned by agent '{conflicting_agent}' on ticket {conflicting_ticket}"
        )),
        BoardError::AlreadyCheckedIn { ticket_id, agent_id } => {
            CliRunError::BadRequest(format!(
                "agent '{agent_id}' is already checked in for ticket {ticket_id}"
            ))
        }
        BoardError::NotCheckedIn { ticket_id, agent_id } => {
            CliRunError::BadRequest(format!(
                "agent '{agent_id}' is not checked in for ticket {ticket_id}"
            ))
        }
        BoardError::TicketNotFound(ticket_id) => {
            CliRunError::BadRequest(format!("ticket not found: {ticket_id}"))
        }
        BoardError::EntryNotFound(entry_id) => {
            CliRunError::BadRequest(format!("board entry not found: {entry_id}"))
        }
        BoardError::StaleCleanToken => CliRunError::BadRequest(
            "clean token is stale: the board has changed since the preview was generated — \
             run `board clean preview` again to get a fresh token"
                .to_string(),
        ),
        BoardError::FileRenameConflict {
            path,
            conflicting_agent,
            conflicting_ticket,
        } => CliRunError::BadRequest(format!(
            "rename conflict: '{path}' is already owned by agent '{conflicting_agent}' on ticket {conflicting_ticket}"
        )),
        BoardError::Storage(_) => CliRunError::Board(err),
    }
}
