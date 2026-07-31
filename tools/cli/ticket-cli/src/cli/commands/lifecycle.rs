use serde_json::{
    Value,
    json,
};
use uuid::Uuid;

use ticket_api::{
    storage::TicketStore,
    workspace,
};

use crate::cli::{
    CancelArgs,
    ClaimArgs,
    CliRunError,
    CloseArgs,
    MoveArgs,
    UnclaimArgs,
};

enum MoveMode {
    Resume,
    Rollback,
    PlanOrExecute,
}

fn resolve_author(explicit: Option<&str>) -> Option<String> {
    explicit.map(str::to_string).or_else(|| {
        std::env::var("TICKET_AUTHOR")
            .ok()
            .filter(|s| !s.is_empty())
    })
}

pub(crate) fn cmd_close(
    args: CloseArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let author = resolve_author(args.author.as_deref());
    let (manifest, path) =
        store.close(&id, &args.to_state, author.as_deref())?;
    let title = manifest
        .extra
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("-");
    Ok(json!({
        "command": "close",
        "status": "ok",
        "id": manifest.id,
        "title": title,
        "target_state": args.to_state,
        "traversed_states": path,
    }))
}

pub(crate) fn cmd_cancel(
    args: CancelArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let author = resolve_author(args.author.as_deref());
    let (manifest, path) = store.close(&id, "cancelled", author.as_deref())?;
    let title = manifest
        .extra
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("-");
    Ok(json!({
        "command": "cancel",
        "status": "ok",
        "id": manifest.id,
        "title": title,
        "traversed_states": path,
    }))
}

pub(crate) fn cmd_move(
    args: MoveArgs,
    store: &TicketStore,
    global_dry_run: bool,
) -> Result<Value, CliRunError> {
    let _span_guard = tracing::debug_span!(
        target: "ticket_cli::transport",
        "ticket_cli_move",
        mode = tracing::field::Empty,
        ticket_id = tracing::field::Empty,
        journal_id = tracing::field::Empty,
        dry_run = global_dry_run || args.dry_run,
    )
    .entered();
    match resolve_move_mode(&args)? {
        MoveMode::Resume => return handle_move_resume(&args, store),
        MoveMode::Rollback => return handle_move_rollback(&args, store),
        MoveMode::PlanOrExecute => {},
    }

    let id = args.id.as_deref().ok_or_else(|| {
        CliRunError::BadRequest(
            "move requires <id> unless --resume/--rollback is used".to_string(),
        )
    })?;
    tracing::Span::current().record(
        "mode",
        if global_dry_run || args.dry_run {
            "plan"
        } else {
            "execute"
        },
    );
    let to_workspace_root =
        args.to_workspace_root.as_deref().ok_or_else(|| {
            CliRunError::BadRequest(
                "move requires --to-workspace-root in plan/execute mode"
                    .to_string(),
            )
        })?;

    let ticket_id = super::resolve_uuid_prefix(id, store)?;
    tracing::Span::current().record("ticket_id", ticket_id.to_string());
    let requested_workspace_root =
        workspace::canonicalize_workspace_root_strict(std::path::Path::new(
            to_workspace_root,
        ))
        .map_err(|error| {
            CliRunError::BadRequest(format!(
                "workspace root canonicalization failed for '{}': {error}",
                to_workspace_root.display()
            ))
        })?;

    let target_store_root = workspace::resolve_store_root_from(
        &requested_workspace_root,
        workspace::TICKET_INDEX_DIR,
    );
    let target_workspace_root =
        workspace::resolve_workspace_root_from_store_root(
            &target_store_root,
            workspace::TICKET_INDEX_DIR,
        );

    let report =
        store.plan_move_preflight(&ticket_id, &target_workspace_root)?;
    let dry_run = global_dry_run || args.dry_run;

    if dry_run || !report.supported() {
        tracing::debug!(
            target: "ticket_cli::transport",
            ticket_id = %ticket_id,
            supported = report.supported(),
            blockers = report.blockers.len(),
            "ticket_cli_move_complete"
        );
        return Ok(json!({
            "command": "move",
            "status": if report.supported() { "ok" } else { "blocked" },
            "mode": "plan",
            "dry_run": true,
            "ticket_id": ticket_id,
            "plan": move_plan_json(&report)?,
            "recovery": recovery_hint(),
        }));
    }

    let outcome = store.execute_move_with_journal(&report)?;
    tracing::Span::current()
        .record("journal_id", outcome.journal.id.to_string());
    tracing::debug!(
        target: "ticket_cli::transport",
        ticket_id = %ticket_id,
        journal_id = %outcome.journal.id,
        phase = ?outcome.journal.phase,
        resumed = outcome.resumed,
        rolled_back = outcome.rolled_back,
        "ticket_cli_move_complete"
    );
    Ok(json!({
        "command": "move",
        "status": "ok",
        "mode": "execute",
        "ticket_id": ticket_id,
        "plan": move_plan_json(&report)?,
        "outcome": move_outcome_json(&outcome),
        "recovery": recovery_hint(),
    }))
}

fn resolve_move_mode(args: &MoveArgs) -> Result<MoveMode, CliRunError> {
    match (args.resume.is_some(), args.rollback.is_some()) {
        (true, true) => Err(CliRunError::BadRequest(
            "move accepts only one of --resume or --rollback".to_string(),
        )),
        (true, false) => Ok(MoveMode::Resume),
        (false, true) => Ok(MoveMode::Rollback),
        (false, false) => Ok(MoveMode::PlanOrExecute),
    }
}

fn validate_move_mode_args(
    args: &MoveArgs,
    mode_flag: &str,
) -> Result<(), CliRunError> {
    if args.id.is_some() || args.to_workspace_root.is_some() {
        return Err(CliRunError::BadRequest(format!(
            "{mode_flag} cannot be combined with id or --to-workspace-root"
        )));
    }
    Ok(())
}

fn parse_move_journal_uuid(
    value: Option<&str>,
    mode_flag: &str,
) -> Result<Uuid, CliRunError> {
    let value = value.expect("mode-specific UUID should exist");
    value.parse::<Uuid>().map_err(|error| {
        CliRunError::BadRequest(format!(
            "invalid {mode_flag} journal UUID: {error}"
        ))
    })
}

fn handle_move_resume(
    args: &MoveArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    tracing::Span::current().record("mode", "resume");
    validate_move_mode_args(args, "--resume")?;
    let journal_id =
        parse_move_journal_uuid(args.resume.as_deref(), "--resume")?;
    let outcome = store.resume_move_with_journal(journal_id)?;
    tracing::Span::current()
        .record("journal_id", outcome.journal.id.to_string());
    tracing::debug!(
        target: "ticket_cli::transport",
        journal_id = %outcome.journal.id,
        phase = ?outcome.journal.phase,
        resumed = outcome.resumed,
        "ticket_cli_move_complete"
    );
    Ok(json!({
        "command": "move",
        "status": "ok",
        "mode": "resume",
        "outcome": move_outcome_json(&outcome),
        "recovery": recovery_hint(),
    }))
}

fn handle_move_rollback(
    args: &MoveArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    tracing::Span::current().record("mode", "rollback");
    validate_move_mode_args(args, "--rollback")?;
    let journal_id =
        parse_move_journal_uuid(args.rollback.as_deref(), "--rollback")?;
    let outcome = store.rollback_move_with_journal(journal_id)?;
    tracing::Span::current()
        .record("journal_id", outcome.journal.id.to_string());
    tracing::debug!(
        target: "ticket_cli::transport",
        journal_id = %outcome.journal.id,
        phase = ?outcome.journal.phase,
        rolled_back = outcome.rolled_back,
        "ticket_cli_move_complete"
    );
    Ok(json!({
        "command": "move",
        "status": "ok",
        "mode": "rollback",
        "outcome": move_outcome_json(&outcome),
        "recovery": recovery_hint(),
    }))
}

fn move_plan_json(
    report: &ticket_api::storage::move_planner::MovePreflightReport
) -> Result<Value, CliRunError> {
    Ok(json!({
        "supported": report.supported(),
        "source_workspace_root": normalize_display_path(&report.source_workspace_root)?,
        "target_workspace_root": normalize_display_path(&report.target_workspace_root)?,
        "source_store_root": normalize_display_path(&report.source_store_root)?,
        "target_store_root": normalize_display_path(&report.target_store_root)?,
        "source_ticket_path": normalize_display_path(&report.source_entity_path)?,
        "destination_ticket_path": normalize_display_path(&report.destination_entity_path)?,
        "path_reference_files": report.path_reference_files
            .iter()
            .map(|path| normalize_display_path(path))
            .collect::<Result<Vec<_>, _>>()?,
        "reference_visibility": report.reference_visibility,
        "active_board_entries": report.active_board_entries,
        "historical_board_entries": report.historical_board_entries,
        "active_leases": report.active_leases,
        "blockers": report.blockers,
        "captured_at": report.captured_at,
    }))
}

fn move_outcome_json(
    outcome: &ticket_api::storage::move_execution::MoveExecutionOutcome
) -> Value {
    json!({
        "resumed": outcome.resumed,
        "rolled_back": outcome.rolled_back,
        "journal": {
            "id": outcome.journal.id,
            "ticket_id": outcome.journal.entity_id,
            "phase": outcome.journal.phase,
            "steps": outcome.journal.steps,
            "rollback_steps": outcome.journal.rollback_steps,
            "failure": outcome.journal.failure,
            "next_recovery_step": outcome.journal.next_recovery_step,
            "rewritten_path_files": outcome.journal.rewritten_path_files,
            "manual_followups": outcome.journal.manual_followups,
            "migrated_board_entries": outcome.journal.migrated_board_entries,
            "created_at": outcome.journal.created_at,
            "updated_at": outcome.journal.updated_at,
        }
    })
}

fn recovery_hint() -> Value {
    json!({
        "resume": "ticket move --resume <journal-uuid>",
        "rollback": "ticket move --rollback <journal-uuid>",
        "execute": "ticket move <ticket-id> --to-workspace-root <path>",
        "dry_run": "ticket move <ticket-id> --to-workspace-root <path> --dry-run",
    })
}

fn normalize_display_path(
    path: &std::path::Path
) -> Result<String, CliRunError> {
    memory_api::workspace::normalize_path_for_display_strict(path).map_err(
        |error| {
            CliRunError::BadRequest(format!(
                "path payload normalization failed for '{}': {error}",
                path.display()
            ))
        },
    )
}

pub(crate) fn cmd_claim(
    args: ClaimArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;

    // Enforce per-ticket exclusivity: `claim` is a single-agent lease, so a
    // different agent already holding the ticket must cause an explicit failure.
    let snap = store.board_show(None)?;
    if let Some(holder) = snap.entries.iter().find(|e| {
        e.ticket_id == id
            && e.status == ticket_api::BoardEntryStatus::Active
            && e.agent_id != args.agent_id
    }) {
        return Err(CliRunError::BadRequest(format!(
            "lease conflict: ticket already claimed by agent '{}' (entry {})",
            holder.agent_id, holder.entry_id,
        )));
    }

    let entry = store.board_check_in(
        &id,
        &args.agent_id,
        args.ttl_secs,
        args.work_intent.as_deref().unwrap_or("claim"),
        vec![],
        None,
        None,
        None,
    )?;
    Ok(json!({
        "command": "claim",
        "status": "ok",
        "ticket_id": entry.ticket_id,
        "working_by": entry.agent_id,
        "entry_id": entry.entry_id,
    }))
}

pub(crate) fn cmd_unclaim(
    args: UnclaimArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let manifest = store.get(&id)?;
    let title = manifest
        .extra
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("-");
    // Determine agent_id from the active board snapshot (find any active entry for this ticket).
    let snap = store.board_show(None)?;
    let entry = snap.entries.iter().find(|e| {
        e.ticket_id == id && e.status == ticket_api::BoardEntryStatus::Active
    });
    let requester = args
        .agent_id
        .clone()
        .or_else(|| entry.map(|e| e.agent_id.clone()))
        .unwrap_or_else(|| "cli".to_string());

    if let Some(e) = entry {
        store.board_check_out(&id, &e.agent_id, args.reason.as_deref())?;
    }
    store.release_lease(&id, &requester)?;

    Ok(json!({
        "command": "unclaim",
        "status": "ok",
        "id": id,
        "title": title,
        "requester": requester,
        "reason": args.reason,
    }))
}

pub(crate) fn cmd_leases(store: &TicketStore) -> Result<Value, CliRunError> {
    let leases = store.list_leases()?;
    let items: Vec<Value> = leases
        .iter()
        .map(|l| {
            json!({
                "ticket_id": l.ticket_id,
                "working_by": l.working_by,
                "expires_at": l.lease_expires_at,
                "expired": l.is_expired(),
                "intent": l.work_intent,
            })
        })
        .collect();
    Ok(json!({
        "command": "leases",
        "status": "ok",
        "count": items.len(),
        "leases": items,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::tempdir;

    fn run_git(
        repo_root: &std::path::Path,
        args: &[&str],
    ) {
        let status = Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .status()
            .expect("git command");
        assert!(status.success(), "git {args:?} failed: {status}");
    }

    #[test]
    fn cmd_move_dry_run_returns_preflight_plan() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let _target_store = TicketStore::init(&target_workspace).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("planned"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let payload = cmd_move(
            MoveArgs {
                id: Some(id.to_string()),
                to_workspace_root: Some(target_workspace),
                resume: None,
                rollback: None,
                dry_run: true,
            },
            &source_store,
            false,
        )
        .unwrap();

        assert_eq!(payload["command"], "move");
        assert_eq!(payload["mode"], "plan");
        assert_eq!(payload["dry_run"], true);
        assert!(payload["plan"]["source_ticket_path"].is_string());
        assert!(payload["plan"]["destination_ticket_path"].is_string());
    }
}
