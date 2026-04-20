use serde_json::{Value, json};

use memory_api::model::filesystem::ScanRoot;
use spec_api::SpecStore;

use crate::cli::{AddRootArgs, CliRunError, HealthArgs, ScanArgs, SearchArgs};

pub(crate) fn cmd_search(args: SearchArgs, store: &SpecStore) -> Result<Value, CliRunError> {
    let results = store.entity_store().search(&args.query, args.limit)?;
    let items: Vec<Value> = results
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "title": r.title,
                "state": r.state,
                "type": r.ticket_type,
                "score": r.score,
                "snippet": r.snippet,
            })
        })
        .collect();
    Ok(json!({
        "command": "search",
        "status": "ok",
        "query": args.query,
        "count": items.len(),
        "items": items,
    }))
}

pub(crate) fn cmd_scan(args: ScanArgs, store: &mut SpecStore) -> Result<Value, CliRunError> {
    let report = store.scan(args.force)?;
    Ok(json!({
        "command": "scan",
        "status": "ok",
        "force": args.force,
        "integrated": report.integrated,
        "pruned": report.pruned,
        "diagnostics_count": report.diagnostics.len(),
    }))
}

pub(crate) fn cmd_add_root(args: AddRootArgs, store: &SpecStore) -> Result<Value, CliRunError> {
    let path = std::fs::canonicalize(&args.path).unwrap_or_else(|_| args.path.clone());
    let label = args.label.unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("specs")
            .to_string()
    });
    store.entity_store().add_scan_root(ScanRoot {
        path: path.clone(),
        label: label.clone(),
    })?;
    Ok(json!({
        "command": "add_root",
        "status": "ok",
        "path": path,
        "label": label,
    }))
}

pub(crate) fn cmd_health(args: HealthArgs, store: &SpecStore) -> Result<Value, CliRunError> {
    let specs = if args.all {
        let all = store.entity_store().list_indexed(false)?;
        all.iter()
            .filter_map(|e| store.get(&e.id.to_string()).ok())
            .collect::<Vec<_>>()
    } else if let Some(id) = &args.id {
        vec![store.get(id)?]
    } else {
        return Err(CliRunError::BadRequest(
            "provide spec ID or --all".to_string(),
        ));
    };

    let mut issues = Vec::new();
    for spec in &specs {
        if spec.slug().is_none() {
            issues.push(json!({"id": spec.id, "issue": "missing slug"}));
        }
        if spec.title().is_none() {
            issues.push(json!({"id": spec.id, "issue": "missing title"}));
        }
        if spec.component().is_none() {
            issues.push(json!({"id": spec.id, "issue": "missing component"}));
        }
    }

    Ok(json!({
        "command": "health",
        "status": "ok",
        "specs_checked": specs.len(),
        "issues_count": issues.len(),
        "issues": issues,
    }))
}
