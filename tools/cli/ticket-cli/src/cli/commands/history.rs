use std::collections::BTreeMap;

use serde_json::{Value, json};

use ticket_api::storage::TicketStore;

use crate::cli::{CliRunError, DiffArgs, HistoryArgs, RevertArgs};

pub(crate) fn cmd_history(
    args: HistoryArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let mut revisions = store.get_history(&id)?;
    revisions.reverse();
    revisions.truncate(args.limit);
    let entries: Vec<Value> = revisions
        .into_iter()
        .map(|r| json!({ "rev": r.rev, "ts": r.ts, "fields": r.fields }))
        .collect();
    Ok(json!({
        "command": "history",
        "status": "ok",
        "id": id,
        "count": entries.len(),
        "entries": entries
    }))
}

fn parse_rev_spec(spec: &str, max_rev: u64) -> Option<u64> {
    if spec == "latest" {
        return Some(max_rev);
    }
    spec.parse::<u64>().ok()
}

pub(crate) fn cmd_diff(args: DiffArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let revisions = store.get_history(&id)?;
    if revisions.is_empty() {
        return Err(CliRunError::BadRequest(
            "no history available for this ticket".into(),
        ));
    }
    let max_rev = revisions.last().map(|r| r.rev).unwrap_or(0);
    let from_rev = parse_rev_spec(&args.from, max_rev).ok_or_else(|| {
        CliRunError::BadRequest(format!("invalid revision specifier: {}", args.from))
    })?;
    let to_rev = parse_rev_spec(&args.to, max_rev).ok_or_else(|| {
        CliRunError::BadRequest(format!("invalid revision specifier: {}", args.to))
    })?;

    let find_rev = |n: u64| revisions.iter().find(|r| r.rev == n).cloned();
    let from = find_rev(from_rev)
        .ok_or_else(|| CliRunError::BadRequest(format!("revision {} not found", from_rev)))?;
    let to = find_rev(to_rev)
        .ok_or_else(|| CliRunError::BadRequest(format!("revision {} not found", to_rev)))?;

    let mut added: BTreeMap<&str, &Value> = BTreeMap::new();
    let mut removed: BTreeMap<&str, &Value> = BTreeMap::new();
    let mut changed: BTreeMap<&str, (&Value, &Value)> = BTreeMap::new();

    for (k, v) in &to.fields {
        match from.fields.get(k) {
            None => {
                added.insert(k.as_str(), v);
            }
            Some(old) if old != v => {
                changed.insert(k.as_str(), (old, v));
            }
            _ => {}
        }
    }
    for (k, v) in &from.fields {
        if !to.fields.contains_key(k) {
            removed.insert(k.as_str(), v);
        }
    }

    let changed_json: serde_json::Map<String, Value> = changed
        .into_iter()
        .map(|(k, (old, new))| (k.to_string(), json!({ "from": old, "to": new })))
        .collect();

    Ok(json!({
        "command": "diff",
        "status": "ok",
        "id": id,
        "from_rev": from_rev,
        "to_rev": to_rev,
        "added": added,
        "removed": removed,
        "changed": changed_json
    }))
}

pub(crate) fn cmd_revert(args: RevertArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let revisions = store.get_history(&id)?;
    if revisions.is_empty() {
        return Err(CliRunError::BadRequest(
            "no history available for this ticket".into(),
        ));
    }
    let max_rev = revisions.last().map(|r| r.rev).unwrap_or(0);
    let target_rev = parse_rev_spec(&args.to_sha, max_rev).ok_or_else(|| {
        CliRunError::BadRequest(format!("invalid revision specifier: {}", args.to_sha))
    })?;
    let snapshot = revisions
        .iter()
        .find(|r| r.rev == target_rev)
        .cloned()
        .ok_or_else(|| CliRunError::BadRequest(format!("revision {} not found", target_rev)))?;

    let new_rev = store.apply_revert(&id, snapshot.fields)?;
    let updated = store.get(&id)?;

    Ok(json!({
        "command": "revert",
        "status": "ok",
        "id": id,
        "reverted_to": target_rev,
        "new_rev": new_rev,
        "ticket": { "fields": updated.extra }
    }))
}
