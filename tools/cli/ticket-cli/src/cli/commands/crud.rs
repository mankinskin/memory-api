use std::collections::BTreeMap;

use serde_json::{
    Map,
    Value,
    json,
};

use ticket_api::storage::{
    DescriptionUpdateMode,
    TicketStore,
    ticket_fs::TicketFs,
};

use crate::cli::{
    CliRunError,
    CreateArgs,
    IdArgs,
    ListArgs,
    ReproArgs,
    UpdateArgs,
    commands::ticket_workspace_metadata_for_path,
    current_git_commit,
    default_repro_summary,
    normalize_repro_timestamp,
    parse_fields,
    parse_fields_to_json,
    repro_summary_from_fields,
};

fn effort_from_ticket(
    _store: &TicketStore,
    ticket: &ticket_api::storage::indexed::IndexedTicket,
) -> Option<u64> {
    TicketFs::read(&ticket.path)
        .ok()
        .and_then(|manifest| {
            manifest
                .extra
                .get("effort")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .as_deref()
        .and_then(ticket_api::workflow::parse_effort)
}

fn resolve_author(explicit: Option<&str>) -> Option<String> {
    explicit.map(str::to_string).or_else(|| {
        std::env::var("TICKET_AUTHOR")
            .ok()
            .filter(|s| !s.is_empty())
    })
}

pub(crate) fn cmd_create(
    args: CreateArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let type_id = args.ticket_type.as_deref().unwrap_or("tracker-improvement");
    let extra = parse_fields_to_json(&args.fields)?;

    let body = args
        .body_file
        .map(|p| {
            std::fs::read_to_string(&p).map_err(|e| {
                CliRunError::InvalidFieldPatch(format!(
                    "cannot read body-file: {e}"
                ))
            })
        })
        .transpose()?;

    let id = store.create(
        args.id,
        type_id,
        args.title.as_deref(),
        args.state.as_deref(),
        extra,
        None,
        body.as_deref(),
    )?;

    let manifest = store.get(&id)?;
    let title = manifest
        .extra
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("-");
    let state = manifest
        .extra
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("open");

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

pub(crate) fn cmd_get(
    args: IdArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let manifest = match store.get(&id) {
        Ok(manifest) => manifest,
        Err(ticket_api::error::StorageError::NotFound(_)) => {
            return Err(CliRunError::BadRequest(format!(
                "ticket '{id}' was not found in the active workspace. Retry with --workspace-root <workspace-path> or --index-root <path-to-.ticket>."
            )));
        },
        Err(error) => return Err(CliRunError::Storage(error)),
    };
    let path = store
        .get_indexed(&id)?
        .map(|ticket| ticket.path.display().to_string());
    let workspace = store
        .get_indexed(&id)?
        .map(|ticket| ticket_workspace_metadata_for_path(store, &ticket.path));
    Ok(json!({
        "command": "get",
        "status": "ok",
        "ticket": {
            "id": manifest.id,
            "path": path,
            "created_at": manifest.created_at,
            "fields": manifest.extra,
            "workspace": workspace,
        }
    }))
}

pub(crate) fn cmd_update(
    args: UpdateArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let author = resolve_author(args.author.as_deref());

    if args.undo {
        if args.to_state.is_some()
            || !args.transition_states.is_empty()
            || !args.fields.is_empty()
        {
            return Err(CliRunError::BadRequest(
                "--undo cannot be combined with --to-state, --transition-state, or --field".into(),
            ));
        }
        let revisions = store.get_history(&id)?;
        if revisions.len() < 2 {
            return Err(CliRunError::BadRequest(
                "cannot undo: not enough history revisions".into(),
            ));
        }
        let prev = &revisions[revisions.len() - 2];
        let prev_rev = prev.rev;
        let mut revert_fields = prev.fields.clone();
        if let Some(desc_val) = revisions[revisions.len() - 1]
            .fields
            .get(ticket_api::storage::DESCRIPTION_HISTORY_KEY)
        {
            revert_fields.insert(
                ticket_api::storage::DESCRIPTION_HISTORY_KEY.to_string(),
                desc_val.clone(),
            );
        }
        let new_rev =
            store.apply_revert(&id, revert_fields, author.as_deref())?;
        let updated = store.get(&id)?;
        return Ok(json!({
            "command": "update",
            "status": "ok",
            "undo": true,
            "reverted_to": prev_rev,
            "new_rev": new_rev,
            "id": id,
            "ticket": { "fields": updated.extra }
        }));
    }

    let description_mode = match args.description_mode.as_str() {
        "replace" => DescriptionUpdateMode::Replace,
        "append" => DescriptionUpdateMode::Append,
        other => {
            return Err(CliRunError::BadRequest(format!(
                "invalid --description-mode '{other}': expected 'replace' or 'append'"
            )));
        },
    };
    let patch = parse_fields_to_json(&args.fields)?;
    let manifest = store.update_with_options(
        &id,
        patch,
        Some(args.transition_states.as_slice()),
        args.to_state.as_deref(),
        args.description.as_deref(),
        description_mode,
        author.as_deref(),
        args.single_hop,
    )?;
    let title = manifest
        .extra
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("-");
    let state = manifest
        .extra
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("open");

    // Optional board check-in after a successful update.
    let board_entry_json: Option<Value> = if args.board_check_in {
        match args.board_agent {
            None => {
                return Err(CliRunError::BadRequest(
                    "--board-check-in requires --board-agent".into(),
                ));
            },
            Some(ref agent) => {
                let ttl = args.board_ttl_secs.unwrap_or(3600);
                let intent = args.board_intent.as_deref().unwrap_or("");
                let entry = store
                    .board_check_in(
                        &id,
                        agent,
                        ttl,
                        intent,
                        args.board_files.clone(),
                    )
                    .map_err(|e| CliRunError::Board(e))?;
                Some(json!({
                    "entry_id": entry.entry_id,
                    "ticket_id": entry.ticket_id,
                    "agent_id": entry.agent_id,
                    "intent": entry.intent,
                    "owned_files": entry.owned_files,
                    "checked_in_at": entry.checked_in_at,
                    "ttl_secs": entry.ttl_secs,
                }))
            },
        }
    } else {
        None
    };

    let mut response = json!({
        "command": "update",
        "status": "ok",
        "id": manifest.id,
        "title": title,
        "state": state,
        "ticket": {
            "id": manifest.id,
            "fields": manifest.extra,
        }
    });

    if let (Some(entry), Some(obj)) =
        (board_entry_json, response.as_object_mut())
    {
        obj.insert("board_entry".to_string(), entry);
    }

    Ok(response)
}

pub(crate) fn cmd_repro(
    args: ReproArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
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
    patch.insert("last_reproduced_commit".to_string(), Value::String(commit));
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

    let updated = store.update(&id, patch, None, None, None, None)?;
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

pub(crate) fn cmd_list(
    args: ListArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let field_filters: Vec<(String, String)> =
        parse_fields(&args.where_clauses)?.into_iter().collect();
    let items = store.list_extended(
        args.state.as_deref(),
        args.ticket_type.as_deref(),
        args.limit,
        &field_filters,
    )?;
    let mut items = items;
    items.sort_by(|left, right| {
        effort_from_ticket(store, left)
            .unwrap_or(u64::MAX)
            .cmp(&effort_from_ticket(store, right).unwrap_or(u64::MAX))
            .then_with(|| right.updated_at.cmp(&left.updated_at))
            .then_with(|| {
                left.title
                    .as_deref()
                    .unwrap_or("")
                    .cmp(right.title.as_deref().unwrap_or(""))
            })
            .then_with(|| left.id.cmp(&right.id))
    });
    let items_json: Vec<Value> = items
        .iter()
        .map(|t| {
            let mut item = json!({
                "id": t.id,
                "type": t.type_id,
                "title": t.title,
                "state": t.state,
                "effort": effort_from_ticket(store, t),
                "updated_at": t.updated_at,
                "workspace": ticket_workspace_metadata_for_path(store, &t.path),
            });

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

pub(crate) fn cmd_delete(
    args: IdArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let manifest = store.get(&id)?;
    let title = manifest
        .extra
        .get("title")
        .and_then(Value::as_str)
        .unwrap_or("-")
        .to_string();
    let ticket_type = manifest
        .extra
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("-")
        .to_string();
    store.delete(&id)?;
    Ok(json!({
        "command": "delete",
        "status": "ok",
        "id": id,
        "title": title,
        "type": ticket_type,
    }))
}

pub(crate) fn cmd_describe(
    args: IdArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let indexed = store.get_indexed(&id)?.ok_or_else(|| {
        CliRunError::BadRequest(format!("ticket not found: {}", id))
    })?;
    let description = TicketFs::read_description(&indexed.path);
    Ok(json!({
        "command": "describe",
        "status": "ok",
        "id": id.to_string(),
        "description": description,
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::cmd_get;
    use crate::cli::IdArgs;
    use ticket_api::storage::store::TicketStore;

    #[test]
    fn cmd_get_includes_authoritative_ticket_folder_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = TicketStore::init(dir.path()).expect("open store");
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("path output regression"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create ticket");
        let indexed = store
            .get_indexed(&id)
            .expect("indexed get")
            .expect("indexed ticket");

        let payload = cmd_get(IdArgs { id: id.to_string() }, &store)
            .expect("cmd_get succeeds");

        assert_eq!(
            payload["ticket"]["path"].as_str(),
            Some(indexed.path.display().to_string().as_str())
        );
    }
}
