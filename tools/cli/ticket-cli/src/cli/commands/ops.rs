use std::collections::BTreeMap;

use chrono::Utc;
use serde_json::{Value, json};

use ticket_api::error::StorageError;
use ticket_api::storage::TicketStore;
use ticket_api::storage::ticket_fs::TicketFs;

use crate::cli::{
    AddRootArgs, AttachArgs, CliRunError, FmtArgs, HealthArgs, IdArgs, NextArgs, ReadyOverviewArgs,
    ScanArgs, ServeCliArgs, StatusArgs, WatchArgs,
};

mod health;
mod next;
mod status;

pub(crate) fn cmd_scan(args: ScanArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let reindex = args.reindex || args.force;
    let report = store.scan(reindex)?;
    let diags: Vec<Value> = report
        .diagnostics
        .iter()
        .map(|d| json!({ "path": d.path, "reason": d.reason }))
        .collect();
    let mut result = json!({
        "command": "scan",
        "status": "ok",
        "integrated": report.integrated,
        "diagnostics": diags,
    });
    if args.force {
        result["force"] = json!(true);
        result["reconciled"] = json!(report.integrated);
        result["pruned"] = json!(report.pruned);
    }
    Ok(result)
}

pub(crate) fn cmd_attach(args: AttachArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let dest = store.attach(&id, &args.path, args.asset_name.as_deref())?;
    let title = store.get(&id).ok()
        .and_then(|m| m.extra.get("title").and_then(Value::as_str).map(String::from))
        .unwrap_or_else(|| "-".to_string());
    Ok(json!({
        "command": "attach",
        "status": "ok",
        "id": id,
        "title": title,
        "asset_path": dest.display().to_string(),
    }))
}

pub(crate) fn cmd_assets(args: IdArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let names = store.list_assets(&id)?;
    Ok(json!({
        "command": "assets",
        "status": "ok",
        "id": id,
        "count": names.len(),
        "assets": names,
    }))
}

pub(crate) fn cmd_audit(store: &TicketStore) -> Result<Value, CliRunError> {
    let all = store.list(None, None, None)?;
    let deleted = store
        .list_extended(None, None, None, true, &[])?
        .into_iter()
        .filter(|t| t.deleted)
        .count();
    let total = all.len() + deleted;

    let mut state_counts = BTreeMap::new();
    for t in &all {
        let state = t.state.as_deref().unwrap_or("unknown");
        *state_counts.entry(state.to_string()).or_insert(0usize) += 1;
    }

    let mut type_counts = BTreeMap::new();
    for t in &all {
        *type_counts.entry(t.type_id.clone()).or_insert(0usize) += 1;
    }

    Ok(json!({
        "command": "audit",
        "status": "ok",
        "total": total,
        "active": all.len(),
        "deleted": deleted,
        "by_state": state_counts,
        "by_type": type_counts,
    }))
}

pub(crate) fn cmd_add_root(args: AddRootArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    use ticket_api::model::filesystem::ScanRoot;
    let path = args.path.canonicalize().unwrap_or(args.path.clone());
    std::fs::create_dir_all(&path).map_err(StorageError::Io)?;
    store.add_scan_root(ScanRoot {
        path: path.clone(),
        label: args.label.clone(),
    })?;
    Ok(json!({
        "command": "add_root",
        "status": "ok",
        "path": path,
        "label": args.label,
    }))
}

pub(crate) fn cmd_serve(args: ServeCliArgs, store: TicketStore) -> Result<Value, CliRunError> {
    use ticket_api::workspace::WorkspaceConfig;
    use ticket_http::serve::{ServeConfig, WorkspaceRegistry, serve};

    let registry = if args.workspace.is_some() {
        WorkspaceRegistry::single_opened(std::sync::Arc::new(store))
    } else {
        let config = WorkspaceConfig::load();
        if config.workspaces.is_empty() {
            WorkspaceRegistry::single_opened(std::sync::Arc::new(store))
        } else {
            WorkspaceRegistry::from_config(&config)
        }
    };

    let config = ServeConfig {
        host: args.host,
        port: args.port,
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| CliRunError::BadRequest(format!("failed to start tokio runtime: {e}")))?;

    rt.block_on(async {
        serve(config, registry)
            .await
            .map_err(|e| CliRunError::BadRequest(e.to_string()))
    })?;

    Err(CliRunError::BadRequest("server exited unexpectedly".into()))
}

pub(crate) fn cmd_watch(args: WatchArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    use ticket_api::watcher::reconciler::{run_watch_loop, start_watcher};
    eprintln!(
        "Starting filesystem watcher (debounce={}ms). Press Ctrl+C to stop.",
        args.debounce_ms
    );
    let handle = start_watcher(store).map_err(CliRunError::Storage)?;
    run_watch_loop(&handle, store, args.debounce_ms);
    Ok(json!({ "command": "watch", "status": "stopped" }))
}

pub(crate) fn cmd_status(args: StatusArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    status::run(args, store)
}

pub(crate) fn cmd_ready_overview(
    args: ReadyOverviewArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let status_payload = cmd_status(
        StatusArgs {
            filter: args.filter.clone(),
            show_blocked: true,
        },
        store,
    )?;

    let scope = args
        .scope
        .unwrap_or_else(|| "ready tickets currently open in the active index".to_string());

    Ok(json!({
        "command": "ready_overview",
        "status": "ok",
        "date": Utc::now().format("%Y-%m-%d").to_string(),
        "scope": scope,
        "summary": status_payload["summary"],
        "ready": status_payload["ready"],
        "ready_count": status_payload["summary"]["ready"],
    }))
}

pub(crate) fn cmd_next(args: NextArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    next::run(args, store)
}

pub(crate) fn cmd_health(args: HealthArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    health::run(args, store)
}

// ── fmt (canonical field ordering) ────────────────────────────────────────────

pub(crate) fn cmd_fmt(args: FmtArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    use ticket_api::model::filesystem::TICKET_MANIFEST_FILE;
    use ticket_api::model::manifest_format;

    // Use the same ticket enumeration as `health --all`: iterate via the index
    // so we pick up every non-deleted ticket regardless of scan-root registration.
    let tickets = store.list(None, None, None)?;

    let mut checked = 0u64;
    let mut reformatted = 0u64;
    let mut already_ok = 0u64;
    let mut errors: Vec<Value> = Vec::new();

    for t in &tickets {
        checked += 1;
        let manifest_path = t.path.join(TICKET_MANIFEST_FILE);

        // Read raw TOML to determine whether reformatting is needed.
        let raw = match std::fs::read_to_string(&manifest_path) {
            Ok(r) => r,
            Err(e) => {
                errors.push(json!({
                    "id": t.id,
                    "path": manifest_path,
                    "error": e.to_string(),
                }));
                continue;
            }
        };

        if manifest_format::is_canonically_ordered(&raw) {
            already_ok += 1;
            continue;
        }

        if args.check {
            // Check-only mode: count but don't write.
            reformatted += 1;
        } else {
            match TicketFs::reformat(&t.path) {
                Ok(()) => reformatted += 1,
                Err(e) => {
                    errors.push(json!({
                        "id": t.id,
                        "path": manifest_path,
                        "error": e.to_string(),
                    }));
                }
            }
        }
    }

    let status = if args.check && reformatted > 0 {
        "needs_formatting"
    } else {
        "ok"
    };

    Ok(json!({
        "command": "fmt",
        "status": status,
        "check_only": args.check,
        "checked": checked,
        "reformatted": reformatted,
        "already_ok": already_ok,
        "errors": errors,
    }))
}
