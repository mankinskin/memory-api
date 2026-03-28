use std::collections::BTreeMap;
use std::collections::{HashMap, HashSet};

use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use ticket_api::error::StorageError;
use ticket_api::storage::TicketStore;

use crate::cli::{
    AddRootArgs, AttachArgs, CliRunError, HealthArgs, IdArgs, NextArgs, ReadyOverviewArgs,
    ScanArgs, ServeCliArgs, StatusArgs, WatchArgs,
};

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

const DONE_STATES: &[&str] = &["done", "cancelled"];
const ACTIVE_STATES: &[&str] = &[
    "in-refinement",
    "ready",
    "in-implementation",
    "in-review",
    "in-validation",
];

pub(crate) fn cmd_status(args: StatusArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let all = store.list(None, None, None)?;

    let tickets: Vec<_> = if let Some(ref prefix) = args.filter {
        all.into_iter()
            .filter(|t| {
                t.title
                    .as_deref()
                    .unwrap_or("")
                    .starts_with(prefix.as_str())
            })
            .collect()
    } else {
        all
    };

    let done_ids: HashSet<Uuid> = tickets
        .iter()
        .filter(|t| {
            t.state
                .as_deref()
                .map(|s| DONE_STATES.contains(&s))
                .unwrap_or(false)
        })
        .map(|t| t.id)
        .collect();

    let all_edges = store.list_all_edges()?;
    let mut blockers: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for edge in &all_edges {
        if edge.kind == "depends_on" && !done_ids.contains(&edge.to) {
            blockers.entry(edge.from).or_default().push(edge.to);
        }
    }

    let mut active = Vec::new();
    let mut ready = Vec::new();
    let mut blocked_list = Vec::new();
    let mut done_count = 0usize;
    let mut total = 0usize;

    for t in &tickets {
        total += 1;
        let state = t.state.as_deref().unwrap_or("new");

        if DONE_STATES.contains(&state) {
            done_count += 1;
            continue;
        }

        let is_active = ACTIVE_STATES.contains(&state);
        let unresolved = blockers.get(&t.id).cloned().unwrap_or_default();
        let is_blocked = !unresolved.is_empty();

        let entry = json!({
            "id": t.id,
            "title": t.title,
            "state": state,
            "component": t.type_id,
        });

        if is_active {
            active.push(entry);
        } else if is_blocked {
            if args.show_blocked {
                let dep_entries: Vec<Value> = unresolved
                    .iter()
                    .map(|dep_id| {
                        let title = tickets
                            .iter()
                            .find(|t| t.id == *dep_id)
                            .and_then(|t| t.title.clone())
                            .unwrap_or_else(|| dep_id.to_string());
                        let dep_state = tickets
                            .iter()
                            .find(|t| t.id == *dep_id)
                            .and_then(|t| t.state.clone())
                            .unwrap_or_else(|| "unknown".to_string());
                        json!({ "id": dep_id, "title": title, "state": dep_state })
                    })
                    .collect();
                blocked_list.push(json!({
                    "id": t.id,
                    "title": t.title,
                    "state": state,
                    "waiting_on": dep_entries
                }));
            }
        } else {
            ready.push(entry);
        }
    }

    let mut by_component: HashMap<String, Vec<&Value>> = HashMap::new();
    for entry in &ready {
        let comp = entry["component"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();
        by_component.entry(comp).or_default().push(entry);
    }

    let parallel_groups: Vec<Value> = by_component
        .into_iter()
        .map(|(component, entries)| {
            json!({
                "component": component,
                "count": entries.len(),
                "tickets": entries
            })
        })
        .collect();

    Ok(json!({
        "command": "status",
        "status": "ok",
        "summary": {
            "total": total,
            "done": done_count,
            "active": active.len(),
            "ready": ready.len(),
            "blocked": if args.show_blocked { blocked_list.len() } else { total - done_count - active.len() - ready.len() }
        },
        "active": active,
        "ready": ready,
        "blocked": blocked_list,
        "parallel_groups": parallel_groups
    }))
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

/// Priority weight for sorting. Lower value = higher priority.
fn priority_weight(p: &str) -> u8 {
    match p {
        "critical" => 0,
        "high" => 1,
        "medium" => 2,
        "low" => 3,
        _ => 4,
    }
}

pub(crate) fn cmd_next(args: NextArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let all = store.list(None, None, None)?;

    let tickets: Vec<_> = if let Some(ref prefix) = args.filter {
        all.into_iter()
            .filter(|t| {
                t.title
                    .as_deref()
                    .unwrap_or("")
                    .starts_with(prefix.as_str())
            })
            .collect()
    } else {
        all
    };

    // Collect IDs of done/cancelled tickets.
    let done_ids: HashSet<Uuid> = tickets
        .iter()
        .filter(|t| {
            t.state
                .as_deref()
                .map(|s| DONE_STATES.contains(&s))
                .unwrap_or(false)
        })
        .map(|t| t.id)
        .collect();

    // Build map of unresolved blockers per ticket.
    let all_edges = store.list_all_edges()?;
    let mut blockers: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for edge in &all_edges {
        if edge.kind == "depends_on" && !done_ids.contains(&edge.to) {
            blockers.entry(edge.from).or_default().push(edge.to);
        }
    }

    // Build state-index map from schemas for state-progress sorting.
    // Higher index = further along the workflow = higher sort rank.
    let mut state_index: HashMap<String, usize> = HashMap::new();
    for type_id in store.schema_registry().type_ids() {
        if let Some(schema) = store.schema_registry().get(type_id) {
            for (i, s) in schema.states.iter().enumerate() {
                state_index.entry(s.clone()).or_insert(i);
            }
        }
    }

    // Find tickets in any non-terminal state with all dependencies satisfied.
    let mut candidates: Vec<_> = tickets
        .iter()
        .filter(|t| {
            t.state
                .as_deref()
                .map(|s| !DONE_STATES.contains(&s))
                .unwrap_or(true)
        })
        .filter(|t| blockers.get(&t.id).map_or(true, |b| b.is_empty()))
        .collect();

    // Read priority from manifest for sorting.
    let mut priority_map: HashMap<Uuid, String> = HashMap::new();
    for t in &candidates {
        if let Ok(manifest) = ticket_api::storage::ticket_fs::TicketFs::read(&t.path) {
            if let Some(p) = manifest.extra.get("priority").and_then(|v| v.as_str()) {
                priority_map.insert(t.id, p.to_string());
            }
        }
    }

    // Sort by state progress (highest index first), then priority, then oldest first.
    candidates.sort_by(|a, b| {
        let sa = a.state.as_deref().unwrap_or("");
        let sb = b.state.as_deref().unwrap_or("");
        let si_a = state_index.get(sa).copied().unwrap_or(0);
        let si_b = state_index.get(sb).copied().unwrap_or(0);
        si_b.cmp(&si_a) // higher index = closer to done = first
            .then_with(|| {
                let pa = priority_map.get(&a.id).map(|s| s.as_str()).unwrap_or("");
                let pb = priority_map.get(&b.id).map(|s| s.as_str()).unwrap_or("");
                priority_weight(pa).cmp(&priority_weight(pb))
            })
            .then_with(|| a.created_at.cmp(&b.created_at))
    });

    candidates.truncate(args.limit);

    // Build dependency count per candidate (total depends_on edges, resolved or not).
    let dep_count: HashMap<Uuid, usize> = {
        let mut m: HashMap<Uuid, usize> = HashMap::new();
        for edge in &all_edges {
            if edge.kind == "depends_on" {
                *m.entry(edge.from).or_default() += 1;
            }
        }
        m
    };

    let items: Vec<Value> = candidates
        .iter()
        .enumerate()
        .map(|(rank, t)| {
            let prio = priority_map
                .get(&t.id)
                .cloned()
                .unwrap_or_else(|| "none".to_string());
            json!({
                "rank": rank + 1,
                "id": t.id,
                "title": t.title,
                "state": t.state,
                "type": t.type_id,
                "priority": prio,
                "dependency_count": dep_count.get(&t.id).copied().unwrap_or(0),
                "created_at": t.created_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(json!({
        "command": "next",
        "status": "ok",
        "count": items.len(),
        "items": items,
    }))
}

// ── health checks ──────────────────────────────────────────────────────────────

use std::collections::VecDeque;
use ticket_api::storage::ticket_fs::TicketFs;

use crate::cli::helpers::parse_fields;

pub(crate) fn cmd_health(args: HealthArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let all_edges = store.list_all_edges()?;

    // Parse --where filters up-front.
    let field_filters: Vec<(String, String)> = parse_fields(&args.where_clauses)?
        .into_iter()
        .collect();

    // Collect tickets in scope via --stdin, --all, or BFS subgraph.
    let tickets: Vec<_> = if args.stdin {
        // Read newline-delimited UUIDs from stdin.
        use std::io::BufRead;
        let stdin = std::io::stdin();
        let mut ids: Vec<Uuid> = Vec::new();
        for line in stdin.lock().lines() {
            let line = line.map_err(ticket_api::error::StorageError::Io)?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            ids.push(super::resolve_uuid_prefix(trimmed, store)?);
        }
        ids.iter()
            .filter_map(|id| store.get_indexed(id).ok().flatten())
            .filter(|t| !t.deleted)
            .collect()
    } else if args.all {
        store.list(None, None, None)?
    } else {
        let root_str = args.root.as_ref().expect("clap ensures root is present when --all/--stdin is not set");
        let root = super::resolve_uuid_prefix(root_str, store)?;
        let depth_limit = args.depth.min(8);

        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut collected_ids: Vec<Uuid> = Vec::new();
        let mut queue: VecDeque<(Uuid, usize)> = VecDeque::new();
        queue.push_back((root, 0));

        while let Some((current_id, depth)) = queue.pop_front() {
            if !visited.insert(current_id) {
                continue;
            }
            collected_ids.push(current_id);

            if depth >= depth_limit {
                continue;
            }

            for edge in &all_edges {
                let kind_ok = edge.kind == "depends_on" || edge.kind == "linked";
                if !kind_ok {
                    continue;
                }

                let (neighbor, is_outbound) = if edge.from == current_id {
                    (edge.to, true)
                } else if edge.to == current_id {
                    (edge.from, false)
                } else {
                    continue;
                };

                let dir_ok = match args.direction.as_str() {
                    "out" => is_outbound,
                    "in" => !is_outbound,
                    _ => true,
                };
                if dir_ok && !visited.contains(&neighbor) {
                    queue.push_back((neighbor, depth + 1));
                }
            }
        }

        // Resolve IndexedTickets for collected IDs.
        collected_ids
            .iter()
            .filter_map(|id| store.get_indexed(id).ok().flatten())
            .filter(|t| !t.deleted)
            .collect()
    };

    // Apply --where field filters if any.
    let tickets: Vec<_> = if field_filters.is_empty() {
        tickets
    } else {
        tickets
            .into_iter()
            .filter(|t| {
                field_filters.iter().all(|(key, expected)| {
                    // Check built-in fields first, then indexed extra fields.
                    let actual = match key.as_str() {
                        "state" => t.state.as_deref().map(String::from),
                        "type" => Some(t.type_id.clone()),
                        "title" => t.title.clone(),
                        _ => None,
                    };
                    actual.as_deref() == Some(expected.as_str())
                })
            })
            .collect()
    };

    // Build lookup sets for edge checks.
    let ticket_ids: HashSet<Uuid> = tickets.iter().map(|t| t.id).collect();
    let done_states: HashSet<&str> = ["done", "cancelled"].into_iter().collect();

    let done_ids: HashSet<Uuid> = tickets
        .iter()
        .filter(|t| {
            t.state
                .as_deref()
                .map(|s| done_states.contains(s))
                .unwrap_or(false)
        })
        .map(|t| t.id)
        .collect();

    // For each ticket, collect unresolved blockers (outbound depends_on to non-done).
    let mut unresolved_deps: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for edge in &all_edges {
        if edge.kind == "depends_on" && ticket_ids.contains(&edge.from) {
            if !done_ids.contains(&edge.to) {
                unresolved_deps.entry(edge.from).or_default().push(edge.to);
            }
        }
    }

    let mut findings: Vec<Value> = Vec::new();
    let mut summary = BTreeMap::new();

    for t in &tickets {
        // Skip done/cancelled — not actionable.
        if done_ids.contains(&t.id) {
            continue;
        }

        let short_id = &t.id.to_string()[..8];
        let title = t.title.as_deref().unwrap_or("?");

        // 1. Missing description file.
        let desc = TicketFs::read_description(&t.path);
        if desc.is_none() {
            *summary.entry("missing_description").or_insert(0u64) += 1;
            findings.push(json!({
                "ticket_id": t.id,
                "short_id": short_id,
                "title": title,
                "check": "missing_description",
                "severity": "warning",
                "message": "No description.md file — ticket lacks detailed context.",
            }));
        } else if let Some(ref body) = desc {
            // 2. Empty or very short description.
            let trimmed_len = body.trim().len();
            if trimmed_len < 50 {
                *summary.entry("short_description").or_insert(0u64) += 1;
                findings.push(json!({
                    "ticket_id": t.id,
                    "short_id": short_id,
                    "title": title,
                    "check": "short_description",
                    "severity": "info",
                    "message": format!("description.md is very short ({trimmed_len} chars) — consider adding more detail."),
                }));
            }
        }

        // 3. Missing title.
        if t.title.is_none() || t.title.as_deref() == Some("") {
            *summary.entry("missing_title").or_insert(0u64) += 1;
            findings.push(json!({
                "ticket_id": t.id,
                "short_id": short_id,
                "title": "(none)",
                "check": "missing_title",
                "severity": "error",
                "message": "Ticket has no title.",
            }));
        }

        // 4. Has unresolved deps but not in new state.
        let state = t.state.as_deref().unwrap_or("");
        let has_unresolved = unresolved_deps.contains_key(&t.id);
        if has_unresolved && state != "new" {
            let dep_count = unresolved_deps[&t.id].len();
            *summary.entry("unblocked_with_deps").or_insert(0u64) += 1;
            findings.push(json!({
                "ticket_id": t.id,
                "short_id": short_id,
                "title": title,
                "check": "unblocked_with_deps",
                "severity": "info",
                "message": format!("Ticket is '{state}' but has {dep_count} unresolved dependency/ies — may need state review."),
            }));
        }

        // 6. Dangling dependency edges (points to deleted or missing ticket).
        for edge in &all_edges {
            if edge.from == t.id && edge.kind == "depends_on" {
                let target_exists = store
                    .get_indexed(&edge.to)
                    .ok()
                    .flatten()
                    .map(|tgt| !tgt.deleted)
                    .unwrap_or(false);
                if !target_exists {
                    let target_short = &edge.to.to_string()[..8];
                    *summary.entry("dangling_edge").or_insert(0u64) += 1;
                    findings.push(json!({
                        "ticket_id": t.id,
                        "short_id": short_id,
                        "title": title,
                        "check": "dangling_edge",
                        "severity": "error",
                        "message": format!("depends_on edge points to {target_short} which is deleted or missing."),
                    }));
                }
            }
        }
    }

    let total_checked = tickets.iter().filter(|t| !done_ids.contains(&t.id)).count();

    Ok(json!({
        "command": "health",
        "status": "ok",
        "tickets_checked": total_checked,
        "finding_count": findings.len(),
        "summary": summary,
        "findings": findings,
    }))
}
