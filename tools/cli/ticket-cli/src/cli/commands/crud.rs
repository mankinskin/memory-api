use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use ticket_api::storage::TicketStore;
use ticket_api::storage::ticket_fs::TicketFs;

use crate::cli::{
    CliRunError, CreateArgs, IdArgs, ListArgs, ReproArgs, UpdateArgs,
    current_git_commit, default_repro_summary, normalize_repro_timestamp,
    parse_fields, parse_fields_to_json, repro_summary_from_fields,
};

pub(crate) fn cmd_create(args: CreateArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let type_id = args.ticket_type.as_deref().unwrap_or("tracker-improvement");
    let extra = parse_fields_to_json(&args.fields)?;
    let target_root = args.target_root.as_deref();

    let body = args
        .body_file
        .map(|p| {
            std::fs::read_to_string(&p).map_err(|e| {
                CliRunError::InvalidFieldPatch(format!("cannot read body-file: {e}"))
            })
        })
        .transpose()?;

    let id = store.create(
        args.id,
        type_id,
        args.title.as_deref(),
        args.state.as_deref(),
        extra,
        target_root,
        body.as_deref(),
    )?;

    let manifest = store.get(&id)?;
    let title = manifest.extra.get("title").and_then(Value::as_str).unwrap_or("-");
    let state = manifest.extra.get("state").and_then(Value::as_str).unwrap_or("open");

    Ok(json!({
        "command": "create",
        "status": "ok",
        "id": id,
        "type": type_id,
        "title": title,
        "state": state,
        "created_at": manifest.created_at,
    }))
}

pub(crate) fn cmd_get(args: IdArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let manifest = store.get(&id)?;
    Ok(json!({
        "command": "get",
        "status": "ok",
        "ticket": {
            "id": manifest.id,
            "created_at": manifest.created_at,
            "fields": manifest.extra,
        }
    }))
}

pub(crate) fn cmd_update(args: UpdateArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let patch = parse_fields_to_json(&args.fields)?;
    let manifest = store.update(
        &id,
        patch,
        args.from_state.as_deref(),
        args.to_state.as_deref(),
    )?;
    let title = manifest.extra.get("title").and_then(Value::as_str).unwrap_or("-");
    let state = manifest.extra.get("state").and_then(Value::as_str).unwrap_or("open");
    Ok(json!({
        "command": "update",
        "status": "ok",
        "id": manifest.id,
        "title": title,
        "state": state,
        "ticket": {
            "id": manifest.id,
            "fields": manifest.extra,
        }
    }))
}

pub(crate) fn cmd_repro(args: ReproArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let manifest = store.get(&id)?;
    let mut reproductions = manifest
        .extra
        .get("reproductions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    let at = normalize_repro_timestamp(args.timestamp.as_deref())?;
    let commit = args
        .commit
        .or_else(current_git_commit)
        .unwrap_or_else(|| "unknown".to_string());
    let outcome = args.outcome.as_str().to_string();

    let mut entry = Map::new();
    entry.insert("at".to_string(), Value::String(at.clone()));
    entry.insert("commit".to_string(), Value::String(commit.clone()));
    entry.insert("outcome".to_string(), Value::String(outcome.clone()));
    if let Some(command) = args.command {
        entry.insert("command".to_string(), Value::String(command));
    }
    if let Some(note) = args.note {
        entry.insert("note".to_string(), Value::String(note));
    }

    reproductions.push(Value::Object(entry.clone()));

    let mut patch = BTreeMap::new();
    patch.insert("reproductions".to_string(), Value::Array(reproductions));
    patch.insert("last_reproduced_at".to_string(), Value::String(at));
    patch.insert(
        "last_reproduced_commit".to_string(),
        Value::String(commit),
    );
    patch.insert(
        "last_reproduction_outcome".to_string(),
        Value::String(outcome),
    );
    if let Some(note) = entry.get("note").cloned() {
        patch.insert("last_reproduction_note".to_string(), note);
    }
    if let Some(command) = entry.get("command").cloned() {
        patch.insert("last_reproduction_command".to_string(), command);
    }

    let updated = store.update(&id, patch, None, None)?;
    let reproduction_count = updated
        .extra
        .get("reproductions")
        .and_then(Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);

    Ok(json!({
        "command": "repro",
        "status": "ok",
        "id": updated.id,
        "reproduction_count": reproduction_count,
        "entry": Value::Object(entry),
    }))
}

pub(crate) fn cmd_list(args: ListArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let field_filters: Vec<(String, String)> = parse_fields(&args.where_clauses)?
        .into_iter()
        .collect();
    let items = store.list_extended(
        args.state.as_deref(),
        args.ticket_type.as_deref(),
        args.limit,
        args.include_deleted,
        &field_filters,
    )?;
    let items_json: Vec<Value> = items
        .iter()
        .map(|t| {
            let mut item = json!({
                "id": t.id,
                "type": t.type_id,
                "title": t.title,
                "state": t.state,
                "updated_at": t.updated_at,
            });

            if t.deleted {
                item["deleted"] = json!(true);
            }

            if args.with_repro {
                let repro = store
                    .get(&t.id)
                    .ok()
                    .map(|manifest| repro_summary_from_fields(&manifest.extra))
                    .unwrap_or_else(default_repro_summary);
                item["repro"] = repro;
            }

            item
        })
        .collect();
    Ok(json!({
        "command": "list",
        "status": "ok",
        "with_repro": args.with_repro,
        "count": items_json.len(),
        "items": items_json,
    }))
}

pub(crate) fn cmd_delete(args: IdArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let manifest = store.get(&id)?;
    let title = manifest.extra.get("title").and_then(Value::as_str).unwrap_or("-").to_string();
    let ticket_type = manifest.extra.get("type").and_then(Value::as_str).unwrap_or("-").to_string();
    store.delete(&id)?;
    Ok(json!({
        "command": "delete",
        "status": "ok",
        "id": id,
        "title": title,
        "type": ticket_type,
    }))
}

pub(crate) fn cmd_describe(args: IdArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let indexed = store
        .get_indexed(&id)?
        .ok_or_else(|| CliRunError::BadRequest(format!("ticket not found: {}", id)))?;
    if indexed.deleted {
        return Err(CliRunError::BadRequest(format!("ticket deleted: {}", id)));
    }
    let description = TicketFs::read_description(&indexed.path);
    Ok(json!({
        "command": "describe",
        "status": "ok",
        "id": id.to_string(),
        "description": description,
    }))
}
