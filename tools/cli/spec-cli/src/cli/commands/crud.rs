use std::collections::BTreeMap;

use serde_json::{Value, json};

use spec_api::{SpecManifest, SpecStore};

use crate::cli::{CliRunError, CreateArgs, GetArgs, IdArgs, ListArgs, UpdateArgs};

pub(crate) fn cmd_create(args: CreateArgs, store: &mut SpecStore) -> Result<Value, CliRunError> {
    let mut manifest = SpecManifest::new(&args.slug, &args.title, &args.component);
    if let Some(parent) = &args.parent {
        let parent_id = store.resolve_id(parent)?;
        manifest.set_parent(&parent_id.to_string());
    }
    if let Some(scope) = &args.scope {
        manifest.set_scope(scope);
    }
    let body = args
        .body_file
        .map(|p| {
            std::fs::read_to_string(&p)
                .map_err(|e| CliRunError::BadRequest(format!("cannot read body-file: {e}")))
        })
        .transpose()?
        .unwrap_or_default();

    let id = store.create(&manifest, &body, args.target_root.as_deref())?;
    Ok(json!({
        "command": "create",
        "status": "ok",
        "id": id,
        "slug": args.slug,
        "title": args.title,
        "component": args.component,
        "state": "draft",
    }))
}

pub(crate) fn cmd_get(args: GetArgs, store: &SpecStore) -> Result<Value, CliRunError> {
    if args.full {
        let (spec, body) = store.get_full(&args.id)?;
        let sections = store.list_sections(&args.id)?;
        Ok(json!({
            "command": "get",
            "status": "ok",
            "spec": {
                "id": spec.id,
                "created_at": spec.created_at,
                "fields": spec.extra,
                "code_refs": spec.code_refs,
            },
            "body": body,
            "sections": sections,
        }))
    } else {
        let spec = store.get(&args.id)?;
        Ok(json!({
            "command": "get",
            "status": "ok",
            "spec": {
                "id": spec.id,
                "created_at": spec.created_at,
                "fields": spec.extra,
                "code_refs": spec.code_refs,
            },
        }))
    }
}

pub(crate) fn cmd_update(args: UpdateArgs, store: &mut SpecStore) -> Result<Value, CliRunError> {
    let mut patch = BTreeMap::new();
    for f in &args.fields {
        let (k, v) = f
            .split_once('=')
            .ok_or_else(|| CliRunError::BadRequest(format!("invalid field patch: {f}")))?;
        patch.insert(k.to_string(), Value::String(v.to_string()));
    }

    // Update body if provided
    if let Some(body_file) = &args.body_file {
        let content = std::fs::read_to_string(body_file)
            .map_err(|e| CliRunError::BadRequest(format!("cannot read body-file: {e}")))?;
        store.update_body(&args.id, &content)?;
    }

    let spec = store.update(&args.id, patch, args.to_state.as_deref())?;
    Ok(json!({
        "command": "update",
        "status": "ok",
        "id": spec.id,
        "fields": spec.extra,
    }))
}

pub(crate) fn cmd_delete(args: IdArgs, store: &mut SpecStore) -> Result<Value, CliRunError> {
    let id = store.resolve_id(&args.id)?;
    store.delete(&args.id)?;
    Ok(json!({
        "command": "delete",
        "status": "ok",
        "id": id,
    }))
}

pub(crate) fn cmd_list(args: ListArgs, store: &SpecStore) -> Result<Value, CliRunError> {
    let all = store.entity_store().list_indexed(false)?;
    let mut items: Vec<Value> = Vec::new();

    'outer: for indexed in &all {
        let spec = match store.get(&indexed.id.to_string()) {
            Ok(s) => s,
            Err(_) => continue,
        };

        for clause in &args.where_clauses {
            if let Some((k, v)) = clause.split_once('=') {
                let field_val = spec.extra.get(k).and_then(|fv| fv.as_str());
                if field_val != Some(v) {
                    continue 'outer;
                }
            }
        }

        items.push(json!({
            "id": indexed.id,
            "slug": spec.slug(),
            "title": spec.title(),
            "state": spec.state(),
            "component": spec.component(),
        }));

        if let Some(limit) = args.limit {
            if items.len() >= limit {
                break;
            }
        }
    }

    Ok(json!({
        "command": "list",
        "status": "ok",
        "count": items.len(),
        "items": items,
    }))
}
