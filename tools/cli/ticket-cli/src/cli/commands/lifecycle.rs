use serde_json::{Value, json};

use ticket_api::storage::TicketStore;

use crate::cli::{CancelArgs, ClaimArgs, CloseArgs, CliRunError, UnclaimArgs};

fn resolve_author(explicit: Option<&str>) -> Option<String> {
    explicit
        .map(str::to_string)
        .or_else(|| std::env::var("TICKET_AUTHOR").ok().filter(|s| !s.is_empty()))
}

pub(crate) fn cmd_close(args: CloseArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let author = resolve_author(args.author.as_deref());
    let (manifest, path) = store.close(&id, &args.to_state, author.as_deref())?;
    let title = manifest.extra.get("title").and_then(Value::as_str).unwrap_or("-");
    Ok(json!({
        "command": "close",
        "status": "ok",
        "id": manifest.id,
        "title": title,
        "target_state": args.to_state,
        "traversed_states": path,
    }))
}

pub(crate) fn cmd_cancel(args: CancelArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let author = resolve_author(args.author.as_deref());
    let (manifest, path) = store.close(&id, "cancelled", author.as_deref())?;
    let title = manifest.extra.get("title").and_then(Value::as_str).unwrap_or("-");
    Ok(json!({
        "command": "cancel",
        "status": "ok",
        "id": manifest.id,
        "title": title,
        "traversed_states": path,
    }))
}

pub(crate) fn cmd_claim(args: ClaimArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let entry = store.board_check_in(
        &id,
        &args.agent_id,
        args.ttl_secs,
        args.work_intent.as_deref().unwrap_or("claim"),
        vec![],
    )?;
    Ok(json!({
        "command": "claim",
        "status": "ok",
        "ticket_id": entry.ticket_id,
        "working_by": entry.agent_id,
        "entry_id": entry.entry_id,
    }))
}

pub(crate) fn cmd_unclaim(args: UnclaimArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let manifest = store.get(&id)?;
    let title = manifest.extra.get("title").and_then(Value::as_str).unwrap_or("-");
    // Determine agent_id from the active board snapshot (find any active entry for this ticket).
    let snap = store.board_show(None)?;
    let entry = snap
        .entries
        .iter()
        .find(|e| e.ticket_id == id && e.status == ticket_api::BoardEntryStatus::Active);
    if let Some(e) = entry {
        store.board_check_out(&id, &e.agent_id, args.reason.as_deref())?;
    }
    Ok(json!({
        "command": "unclaim",
        "status": "ok",
        "id": id,
        "title": title,
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
