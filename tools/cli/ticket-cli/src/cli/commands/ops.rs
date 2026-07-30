use std::{
    collections::BTreeMap,
    fs,
    path::Path,
};

use chrono::Utc;
use serde_json::{
    Value,
    json,
};
use uuid::Uuid;

use ticket_api::{
    TICKET_INDEX_AGENT_HOOK_PATH,
    TicketCatalogSource,
    error::StorageError,
    generate_ticket_catalog,
    storage::{
        TicketStore,
        ticket_fs::TicketFs,
    },
    workspace,
};

use crate::cli::{
    AddRootArgs,
    AttachArgs,
    BlockersArgs,
    CliRunError,
    FmtArgs,
    HealthArgs,
    IdArgs,
    NextArgs,
    ReadyOverviewArgs,
    ScanArgs,
    ServeCliArgs,
    StatusArgs,
    StoreIndexArgs,
    UnblockedByArgs,
    WatchArgs,
};

const STORE_DIR: &str = ".ticket";

mod health;
mod next;
mod status;

pub(crate) fn cmd_scan(
    args: ScanArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
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

pub(crate) fn cmd_attach(
    args: AttachArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let dest = store.attach(&id, &args.path, args.asset_name.as_deref())?;
    let title = store
        .get(&id)
        .ok()
        .and_then(|m| {
            m.extra
                .get("title")
                .and_then(Value::as_str)
                .map(String::from)
        })
        .unwrap_or_else(|| "-".to_string());
    Ok(json!({
        "command": "attach",
        "status": "ok",
        "id": id,
        "title": title,
        "asset_path": dest.display().to_string(),
    }))
}

pub(crate) fn cmd_assets(
    args: IdArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
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

/// Show the legal state-transition graph for a ticket.
///
/// Returns the current state, the states reachable in a single hop
/// (`allowed_next_states`), the full transition edge list, and the schema's
/// declared states, required intermediate states, and terminal states. This is
/// the inspection surface paired with the invalid-transition recovery error.
pub(crate) fn cmd_transitions(
    args: IdArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let indexed = store.get_indexed(&id)?.ok_or_else(|| {
        CliRunError::BadRequest(format!(
            "ticket '{id}' was not found in the active workspace. Retry with --workspace-root <workspace-path> or --index-root <path-to-.ticket>."
        ))
    })?;
    let type_id = indexed.type_id.clone();
    let current_state =
        indexed.state.clone().unwrap_or_else(|| "open".to_string());
    let schema = store.schema_registry().get(&type_id).ok_or_else(|| {
        CliRunError::BadRequest(format!(
            "no schema registered for ticket type '{type_id}'"
        ))
    })?;

    let allowed_next = schema.allowed_next_states(&current_state);
    let transitions: Vec<Value> = schema
        .transitions
        .iter()
        .map(|t| json!({ "from": t.from, "to": t.to }))
        .collect();

    Ok(json!({
        "command": "transitions",
        "status": "ok",
        "id": id,
        "type": type_id,
        "current_state": current_state,
        "allowed_next_states": allowed_next,
        "states": schema.states,
        "transitions": transitions,
        "required_states": schema.required_states,
        "terminal_states": schema.terminal_states,
    }))
}

pub(crate) fn cmd_audit(store: &TicketStore) -> Result<Value, CliRunError> {
    let all = store.list(None, None, None)?;

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
        "total": all.len(),
        "by_state": state_counts,
        "by_type": type_counts,
    }))
}

pub(crate) fn cmd_store_index(
    args: StoreIndexArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let workspace_root = workspace::resolve_workspace_root_from_store_root(
        &store.index_root,
        workspace::TICKET_INDEX_DIR,
    );

    let indexed = store.list(None, None, None)?;
    let mut sources = Vec::with_capacity(indexed.len());

    for ticket in indexed {
        let manifest = TicketFs::read(&ticket.path)?;
        let description =
            TicketFs::read_description(&ticket.path).unwrap_or_default();
        let source_path = memory_api::index_generator::to_relative_slash(
            &workspace_root,
            &ticket.path.join("ticket.toml"),
        );

        let title = manifest
            .extra
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let state = manifest
            .extra
            .get("state")
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let priority = manifest
            .extra
            .get("priority")
            .and_then(Value::as_str)
            .map(str::to_string);
        let component = manifest
            .extra
            .get("component")
            .and_then(Value::as_str)
            .map(str::to_string);

        sources.push(TicketCatalogSource {
            id: ticket.id,
            source_path,
            title,
            state,
            priority,
            component,
            description,
        });
    }

    let artifacts = generate_ticket_catalog(&sources, STORE_DIR);

    let readme_path = workspace_root.join(STORE_DIR).join("README.md");
    let sidecar_path = workspace_root.join(STORE_DIR).join("index.toon");
    let agent_hook_path = workspace_root.join(TICKET_INDEX_AGENT_HOOK_PATH);

    let sidecar_toon = artifacts
        .sidecar
        .encode_toon()
        .map_err(|e| CliRunError::BadRequest(e.to_string()))?;

    let readme_out = memory_api::generated_markdown::prepare_generated_output(
        &artifacts.readme_markdown,
        read_existing(&readme_path).as_deref(),
    );
    let sidecar_out = memory_api::generated_markdown::prepare_generated_output(
        &sidecar_toon,
        read_existing(&sidecar_path).as_deref(),
    );
    let agent_hook_out =
        memory_api::generated_markdown::prepare_generated_output(
            &artifacts.agent_hook_markdown,
            read_existing(&agent_hook_path).as_deref(),
        );

    let planned = [
        (&readme_path, &readme_out),
        (&sidecar_path, &sidecar_out),
        (&agent_hook_path, &agent_hook_out),
    ];

    if args.check {
        let drifted: Vec<String> = planned
            .iter()
            .filter(|(path, content)| {
                read_existing(path).as_deref() != Some(content.as_str())
            })
            .map(|(path, _)| display_path(path))
            .collect();

        if !drifted.is_empty() {
            return Err(CliRunError::BadRequest(format!(
                "ticket store-index is out of date; regenerate and re-stage: {}",
                drifted.join(", ")
            )));
        }

        return Ok(json!({
            "command": "store-index",
            "status": "ok",
            "check": true,
            "drift": false,
            "tickets": sources.len(),
        }));
    }

    let mut written = Vec::new();
    for (path, content) in planned {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(memory_api::error::StorageError::Io)?;
        }
        fs::write(path, content)
            .map_err(memory_api::error::StorageError::Io)?;
        written.push(display_path(path));
    }

    Ok(json!({
        "command": "store-index",
        "status": "ok",
        "check": false,
        "tickets": sources.len(),
        "written": written,
    }))
}

fn read_existing(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Resolve a `SpecRef.store_root` (repo-root-relative, e.g. ".spec" or
/// "memory-api/.spec") against the workspace root that `ticket validate-links`
/// was invoked against.
fn resolve_referenced_spec_root(
    workspace_root: &Path,
    store_root: &str,
) -> std::path::PathBuf {
    let candidate = Path::new(store_root);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        workspace_root.join(candidate)
    }
}

/// Attempt to read a spec at `root` by id. Returns `None` when the store
/// does not exist there or the id is not found.
fn try_get_spec(
    root: &Path,
    spec_id: Uuid,
) -> Option<spec_api::SpecManifest> {
    let store = spec_api::SpecStore::open(root).ok()?;
    store.get(&spec_id.to_string()).ok()
}

fn count_links_by_kind(findings: &[Value]) -> Value {
    let mut counts = serde_json::Map::new();
    for finding in findings {
        let kind = finding
            .get("kind")
            .and_then(|k| k.as_str())
            .unwrap_or("unknown")
            .to_string();
        let entry = counts.entry(kind).or_insert(json!(0));
        if let Some(n) = entry.as_u64() {
            *entry = json!(n + 1);
        }
    }
    Value::Object(counts)
}

/// Validate `related_specs` links: detect dangling spec refs, wrong-store
/// refs, and bidirectional inconsistencies against the referenced spec
/// store(s). Mirrors `spec validate-links` from the other direction.
pub(crate) fn cmd_validate_links(
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let workspace_root = workspace::resolve_workspace_root_from_store_root(
        &store.index_root,
        workspace::TICKET_INDEX_DIR,
    );
    let canonical_spec_root = workspace_root.join(".spec");

    let all = store.list(None, None, None)?;
    let mut findings: Vec<Value> = Vec::new();
    let mut checked = 0usize;

    for indexed in &all {
        let ticket = match store.get(&indexed.id) {
            Ok(ticket) => ticket,
            Err(_) => continue,
        };

        for spec_ref in ticket.related_specs() {
            checked += 1;
            let referenced_root = resolve_referenced_spec_root(
                &workspace_root,
                &spec_ref.store_root,
            );

            if let Some(spec) = try_get_spec(&referenced_root, spec_ref.spec_id)
            {
                let has_back_ref = spec
                    .related_tickets()
                    .iter()
                    .any(|ticket_ref| ticket_ref.ticket_id == ticket.id);
                if !has_back_ref {
                    findings.push(json!({
                        "kind": "bidirectional_inconsistency",
                        "ticket_id": ticket.id,
                        "spec_id": spec_ref.spec_id,
                        "workspace": spec_ref.workspace,
                        "store_root": spec_ref.store_root,
                        "message": format!(
                            "ticket {} links spec {} but the spec's related_tickets does not link back",
                            ticket.id, spec_ref.spec_id,
                        ),
                    }));
                }
                continue;
            }

            if referenced_root != canonical_spec_root
                && try_get_spec(&canonical_spec_root, spec_ref.spec_id).is_some()
            {
                findings.push(json!({
                    "kind": "wrong_store_ref",
                    "ticket_id": ticket.id,
                    "spec_id": spec_ref.spec_id,
                    "workspace": spec_ref.workspace,
                    "store_root": spec_ref.store_root,
                    "message": format!(
                        "spec {} exists but not under store_root '{}'; found under the workspace's canonical .spec store instead",
                        spec_ref.spec_id, spec_ref.store_root,
                    ),
                }));
                continue;
            }

            findings.push(json!({
                "kind": "dangling_spec_ref",
                "ticket_id": ticket.id,
                "spec_id": spec_ref.spec_id,
                "workspace": spec_ref.workspace,
                "store_root": spec_ref.store_root,
                "message": format!(
                    "spec {} not found under store_root '{}'",
                    spec_ref.spec_id, spec_ref.store_root,
                ),
            }));
        }
    }

    let counts = count_links_by_kind(&findings);

    Ok(json!({
        "command": "validate_links",
        "status": "ok",
        "workspace_root": workspace_root.display().to_string(),
        "checked": checked,
        "valid": findings.is_empty(),
        "counts": counts,
        "findings": findings,
    }))
}

#[cfg(test)]
mod validate_links_tests {
    use std::collections::BTreeMap;

    use memory_api::model::entity::SpecRef;
    use spec_api::{
        SpecManifest,
        SpecStore,
        TicketRef,
    };
    use tempfile::TempDir;

    use super::{
        TicketStore,
        cmd_validate_links,
    };

    /// Reproduces the nested-store bug: a ticket's `related_specs` entry
    /// carries a `store_root` that does not resolve to any store, while the
    /// referenced spec actually exists under the workspace's canonical
    /// `.spec` store. Before structured `SpecRef` carried an explicit store
    /// identifier, a relative-path prose link like this could silently
    /// resolve against the wrong (or a nonexistent) store instead of being
    /// flagged. `validate-links` must detect it as `wrong_store_ref`.
    #[test]
    fn detects_wrong_store_ref_for_nested_store_bug_scenario() {
        let workspace = TempDir::new().unwrap();
        let workspace_root = workspace.path();

        let ticket_store =
            TicketStore::init(&workspace_root.join(".ticket")).unwrap();
        let mut spec_store =
            SpecStore::init(&workspace_root.join(".spec")).unwrap();

        let spec_manifest = SpecManifest::new(
            "traceability/nested-store-bug",
            "Nested store bug spec",
            "ticket-api",
        );
        let spec_id = spec_manifest.id();
        spec_store.create(&spec_manifest, "body", None).unwrap();

        let ticket_id = ticket_store
            .create(
                None,
                "task",
                Some("Nested store bug ticket"),
                None,
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();

        // Wrong: the spec actually lives under the canonical `.spec` store,
        // but this ref claims a nonexistent nested store.
        let wrong_spec_ref = SpecRef {
            spec_id,
            workspace: "default".to_string(),
            store_root: "nested/.spec".to_string(),
        };
        let mut patch = BTreeMap::new();
        patch.insert(
            "related_specs".to_string(),
            serde_json::to_value(vec![wrong_spec_ref]).unwrap(),
        );
        ticket_store
            .update(&ticket_id, patch, None, None, None, None)
            .unwrap();

        let result = cmd_validate_links(&ticket_store).unwrap();

        assert_eq!(result["valid"], false);
        assert_eq!(result["checked"], 1);
        let findings = result["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["kind"], "wrong_store_ref");
        assert_eq!(findings[0]["spec_id"], spec_id.to_string());
    }

    #[test]
    fn detects_dangling_spec_ref() {
        let workspace = TempDir::new().unwrap();
        let workspace_root = workspace.path();

        let ticket_store =
            TicketStore::init(&workspace_root.join(".ticket")).unwrap();
        SpecStore::init(&workspace_root.join(".spec")).unwrap();

        let ticket_id = ticket_store
            .create(
                None,
                "task",
                Some("Dangling spec ref ticket"),
                None,
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();

        let dangling_ref = SpecRef {
            spec_id: uuid::Uuid::new_v4(),
            workspace: "default".to_string(),
            store_root: ".spec".to_string(),
        };
        let mut patch = BTreeMap::new();
        patch.insert(
            "related_specs".to_string(),
            serde_json::to_value(vec![dangling_ref]).unwrap(),
        );
        ticket_store
            .update(&ticket_id, patch, None, None, None, None)
            .unwrap();

        let result = cmd_validate_links(&ticket_store).unwrap();

        assert_eq!(result["valid"], false);
        let findings = result["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["kind"], "dangling_spec_ref");
    }

    #[test]
    fn detects_bidirectional_inconsistency() {
        let workspace = TempDir::new().unwrap();
        let workspace_root = workspace.path();

        let ticket_store =
            TicketStore::init(&workspace_root.join(".ticket")).unwrap();
        let mut spec_store =
            SpecStore::init(&workspace_root.join(".spec")).unwrap();

        let spec_manifest = SpecManifest::new(
            "traceability/no-back-ref",
            "No back ref spec",
            "ticket-api",
        );
        let spec_id = spec_manifest.id();
        spec_store.create(&spec_manifest, "body", None).unwrap();
        // Deliberately do not set the spec's related_tickets back-reference.

        let ticket_id = ticket_store
            .create(
                None,
                "task",
                Some("One-way link ticket"),
                None,
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();

        let spec_ref = SpecRef {
            spec_id,
            workspace: "default".to_string(),
            store_root: ".spec".to_string(),
        };
        let mut patch = BTreeMap::new();
        patch.insert(
            "related_specs".to_string(),
            serde_json::to_value(vec![spec_ref]).unwrap(),
        );
        ticket_store
            .update(&ticket_id, patch, None, None, None, None)
            .unwrap();

        let result = cmd_validate_links(&ticket_store).unwrap();

        assert_eq!(result["valid"], false);
        let findings = result["findings"].as_array().unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0]["kind"], "bidirectional_inconsistency");
    }

    #[test]
    fn valid_when_bidirectional_link_is_consistent() {
        let workspace = TempDir::new().unwrap();
        let workspace_root = workspace.path();

        let ticket_store =
            TicketStore::init(&workspace_root.join(".ticket")).unwrap();
        let mut spec_store =
            SpecStore::init(&workspace_root.join(".spec")).unwrap();

        let mut spec_manifest = SpecManifest::new(
            "traceability/consistent-link",
            "Consistent link spec",
            "ticket-api",
        );
        let spec_id = spec_manifest.id();

        let ticket_id = ticket_store
            .create(
                None,
                "task",
                Some("Consistent link ticket"),
                None,
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();

        spec_manifest.set_related_tickets(vec![TicketRef {
            ticket_id,
            workspace: "default".to_string(),
            store_root: ".ticket".to_string(),
        }]);
        spec_store.create(&spec_manifest, "body", None).unwrap();

        let spec_ref = SpecRef {
            spec_id,
            workspace: "default".to_string(),
            store_root: ".spec".to_string(),
        };
        let mut patch = BTreeMap::new();
        patch.insert(
            "related_specs".to_string(),
            serde_json::to_value(vec![spec_ref]).unwrap(),
        );
        ticket_store
            .update(&ticket_id, patch, None, None, None, None)
            .unwrap();

        let result = cmd_validate_links(&ticket_store).unwrap();

        assert_eq!(result["valid"], true);
        assert_eq!(result["findings"].as_array().unwrap().len(), 0);
    }
}

pub(crate) fn cmd_add_root(
    args: AddRootArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
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

pub(crate) fn cmd_serve(
    args: ServeCliArgs,
    store: TicketStore,
) -> Result<Value, CliRunError> {
    use ticket_http::serve::{
        ServeConfig,
        WorkspaceRegistry,
        serve,
    };

    let registry = WorkspaceRegistry::single_opened(std::sync::Arc::new(store));

    let config = ServeConfig {
        host: args.host,
        port: args.port,
    };

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            CliRunError::BadRequest(format!(
                "failed to start tokio runtime: {e}"
            ))
        })?;

    rt.block_on(async {
        serve(config, registry)
            .await
            .map_err(|e| CliRunError::BadRequest(e.to_string()))
    })?;

    Err(CliRunError::BadRequest("server exited unexpectedly".into()))
}

pub(crate) fn cmd_watch(
    args: WatchArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    use ticket_api::watcher::reconciler::{
        run_watch_loop,
        start_watcher,
    };
    eprintln!(
        "Starting filesystem watcher (debounce={}ms). Press Ctrl+C to stop.",
        args.debounce_ms
    );
    let handle = start_watcher(store).map_err(CliRunError::Storage)?;
    run_watch_loop(&handle, store, args.debounce_ms);
    Ok(json!({ "command": "watch", "status": "stopped" }))
}

pub(crate) fn cmd_status(
    args: StatusArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
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

    let scope = args.scope.unwrap_or_else(|| {
        "ready tickets currently open in the active index".to_string()
    });

    Ok(json!({
        "command": "ready_overview",
        "status": "ok",
        "date": Utc::now().format("%Y-%m-%d").to_string(),
        "scope": scope,
        "summary": status_payload["summary"],
        "planned": status_payload["planned"],
        "ready_count": status_payload["summary"]["planned"],
    }))
}

pub(crate) fn cmd_next(
    args: NextArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    next::run(args, store)
}

pub(crate) fn cmd_blockers(
    args: BlockersArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    next::run_blockers(args, store)
}

pub(crate) fn cmd_unblocked_by(
    args: UnblockedByArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    next::run_unblocked_by(args, store)
}

pub(crate) fn cmd_health(
    args: HealthArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    health::run(args, store)
}

// ── fmt (canonical field ordering) ────────────────────────────────────────────

pub(crate) fn cmd_fmt(
    args: FmtArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    use ticket_api::model::{
        filesystem::TICKET_MANIFEST_FILE,
        manifest_format,
    };

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
            },
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
                },
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
