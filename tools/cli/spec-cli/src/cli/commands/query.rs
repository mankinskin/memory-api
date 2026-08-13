use serde_json::{
    Value,
    json,
};

use memory_kernel::{
    error::StorageError,
    model::filesystem::ScanRoot,
};
use spec_api::SpecStore;

use crate::cli::{
    AddRootArgs,
    CliRunError,
    HealthArgs,
    ScanArgs,
    SearchArgs,
};

pub(crate) fn cmd_search(
    args: SearchArgs,
    store: &SpecStore,
) -> Result<Value, CliRunError> {
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

pub(crate) fn cmd_scan(
    args: ScanArgs,
    store: &mut SpecStore,
) -> Result<Value, CliRunError> {
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

pub(crate) fn cmd_add_root(
    args: AddRootArgs,
    store: &SpecStore,
) -> Result<Value, CliRunError> {
    std::fs::create_dir_all(&args.path).map_err(StorageError::Io)?;
    let path =
        std::fs::canonicalize(&args.path).unwrap_or_else(|_| args.path.clone());
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

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn add_root_creates_missing_directory() {
        let dir = tempdir().unwrap();
        let index_root = dir.path().join(".spec");
        let store = SpecStore::init(&index_root).unwrap();
        let root = index_root.join("specs");

        cmd_add_root(
            AddRootArgs {
                path: root.clone(),
                label: None,
            },
            &store,
        )
        .unwrap();

        assert!(root.is_dir());
    }
}

pub(crate) fn cmd_health(
    args: HealthArgs,
    store: &SpecStore,
) -> Result<Value, CliRunError> {
    let report = if args.all {
        store.health_all()?
    } else if let Some(id) = &args.id {
        store.health(id)?
    } else {
        return Err(CliRunError::BadRequest(
            "provide spec ID or --all".to_string(),
        ));
    };

    Ok(json!({
        "command": "health",
        "status": "ok",
        "specs_checked": report.specs_checked,
        "issues_count": report.issues_count(),
        "issues": report.issues,
    }))
}
