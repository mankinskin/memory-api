use serde_json::{
    Value,
    json,
};
use uuid::Uuid;

use ticket_api::storage::{
    TicketStore,
    board::{
        BoardConfig,
        BoardEntryStatus,
        BoardError,
    },
};

use crate::cli::{
    BoardArgs,
    BoardCleanCommand,
    BoardCommand,
    CliRunError,
};

use super::resolve_uuid_prefix;

mod render;

use self::render::{
    config_to_json,
    entry_to_json,
    render_board_human,
};

// ── entry point ────────────────────────────────────────────────────────────────

pub(crate) fn cmd_board(
    args: BoardArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    match args.command {
        BoardCommand::Show { agent } => cmd_board_show(agent.as_deref(), store),
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
            .filter(|e| e.status == BoardEntryStatus::Active)
            .map(|e| e.entry_id)
            .collect();

        for eid in &active_entry_ids {
            // Non-fatal: stale entries may already be gone.
            let _ = store.board_heartbeat(eid);
        }

        if !active_entry_ids.is_empty() {
            snap = store.board_show(Some(agent_id))?;
        }
    }

    let entries: Vec<Value> = snap
        .entries
        .iter()
        .map(|e| entry_to_json(e, &snap.config))
        .collect();

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
        "warnings": snap.warnings,
        "file_ownership": file_ownership,
        "human": render_board_human(&snap),
    }))
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
    let resolved_agent = if let Some(a) = agent {
        a
    } else {
        let snap = store.board_show(None)?;
        snap.entries
            .into_iter()
            .find(|e| {
                e.ticket_id == ticket_id && e.status == BoardEntryStatus::Active
            })
            .map(|e| e.agent_id)
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
    let eid = entry_id.parse::<Uuid>().map_err(|_| {
        CliRunError::BadRequest(format!(
            "invalid entry_id '{entry_id}': expected a UUID"
        ))
    })?;

    let entry = store.board_heartbeat(&eid).map_err(board_err_to_cli)?;

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
        // Read-only path.
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
        BoardError::WipLimitReached { current, max } => CliRunError::BadRequest(format!(
            "WIP limit reached: {current}/{max} active entries — check out a ticket or raise the limit with `board configure --max-wip`"
        )),
        BoardError::FileConflict { files, conflicting_agent, conflicting_ticket } => {
            CliRunError::BadRequest(format!(
                "file conflict: {files:?} already owned by agent '{conflicting_agent}' on ticket {conflicting_ticket}"
            ))
        }
        BoardError::AlreadyCheckedIn { ticket_id, agent_id } => CliRunError::BadRequest(format!(
            "agent '{agent_id}' is already checked in for ticket {ticket_id}"
        )),
        BoardError::NotCheckedIn { ticket_id, agent_id } => CliRunError::BadRequest(format!(
            "agent '{agent_id}' is not checked in for ticket {ticket_id}"
        )),
        BoardError::TicketNotFound(id) => {
            CliRunError::BadRequest(format!("ticket not found: {id}"))
        }
        BoardError::EntryNotFound(id) => {
            CliRunError::BadRequest(format!("board entry not found: {id}"))
        }
        BoardError::StaleCleanToken => CliRunError::BadRequest(
            "clean token is stale: the board has changed since the preview was generated — \
             run `board clean preview` again to get a fresh token"
                .to_string(),
        ),
        BoardError::FileRenameConflict { path, conflicting_agent, conflicting_ticket } => {
            CliRunError::BadRequest(format!(
                "rename conflict: '{path}' is already owned by agent '{conflicting_agent}' on ticket {conflicting_ticket}"
            ))
        }
        BoardError::Storage(_) => CliRunError::Board(err),
    }
}
