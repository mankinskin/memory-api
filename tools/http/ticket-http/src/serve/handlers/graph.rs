use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::time::Instant;

// Adjacency list entry: (neighbor_id, edge_from, edge_to, edge_kind)
type AdjEntry = (Uuid, Uuid, Uuid, String);
use uuid::Uuid;

use ticket_api::storage::ticket_fs::TicketFs;
use viewer_api::error::RequestIdExt;
use crate::serve::{error::storage_err, AppState};

#[derive(Deserialize)]
pub struct SubgraphQuery {
    pub workspace: String,
    pub root: Uuid,
    pub direction: Option<String>,
    pub edge_kind: Option<String>,
    #[serde(default = "default_depth")]
    pub depth: usize,
    #[serde(default = "default_limit_nodes")]
    pub limit_nodes: usize,
    #[serde(default = "default_limit_edges")]
    pub limit_edges: usize,
}

fn default_depth() -> usize { 2 }
fn default_limit_nodes() -> usize { 500 }
fn default_limit_edges() -> usize { 2000 }

#[derive(Serialize)]
pub struct NodeItem {
    pub id: String,
    pub title: Option<String>,
    pub state: Option<String>,
    pub depth: usize,
}

#[derive(Serialize)]
pub struct EdgeItem {
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Serialize)]
pub struct SubgraphStats {
    pub nodes_returned: usize,
    pub edges_returned: usize,
    pub max_depth_reached: usize,
    pub phase1_edges_ms: u128,
    pub phase2_bfs_ms: u128,
    pub phase3_meta_ms: u128,
    pub total_ms: u128,
}

#[derive(Serialize)]
pub struct SubgraphResponse {
    pub request_id: String,
    pub workspace: String,
    pub nodes: Vec<NodeItem>,
    pub edges: Vec<EdgeItem>,
    pub truncated: bool,
    pub next_cursor: Option<String>,
    pub stats: SubgraphStats,
}

pub async fn subgraph(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<SubgraphQuery>,
) -> Response {
    let root_str = params.root.to_string();
    tracing::debug!(
        workspace = %params.workspace,
        root = %root_str,
        depth = params.depth,
        request_id = %rid.0,
        "subgraph request received"
    );
    let request_id = rid.0;
    let workspace = params.workspace;
    let root = params.root;
    let direction = params.direction.unwrap_or_else(|| "both".to_string());
    let edge_kind = params.edge_kind;
    let depth = params.depth;
    let limit_nodes = params.limit_nodes;
    let limit_edges = params.limit_edges;
    tokio::task::spawn_blocking(move || bfs_graph(
        state,
        &request_id,
        workspace,
        root,
        &direction,
        edge_kind.as_deref(),
        depth,
        limit_nodes,
        limit_edges,
    ))
    .await
    .unwrap_or_else(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

#[derive(Deserialize)]
pub struct TopgraphQuery {
    pub workspace: String,
    pub root: Uuid,
    pub direction: Option<String>,
    pub edge_kind: Option<String>,
    #[serde(default = "default_depth")]
    pub depth: usize,
    #[serde(default = "default_limit_nodes")]
    pub limit_nodes: usize,
    #[serde(default = "default_limit_edges")]
    pub limit_edges: usize,
}

pub async fn topgraph(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<TopgraphQuery>,
) -> Response {
    let request_id = rid.0;
    let workspace = params.workspace;
    let root = params.root;
    let direction = params.direction.unwrap_or_else(|| "in".to_string());
    let edge_kind = params.edge_kind;
    let depth = params.depth;
    let limit_nodes = params.limit_nodes;
    let limit_edges = params.limit_edges;
    tokio::task::spawn_blocking(move || bfs_graph(
        state,
        &request_id,
        workspace,
        root,
        &direction,
        edge_kind.as_deref(),
        depth,
        limit_nodes,
        limit_edges,
    ))
    .await
    .unwrap_or_else(|_| axum::http::StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

fn bfs_graph(
    state: AppState,
    request_id: &str,
    workspace: String,
    root: Uuid,
    direction: &str,
    edge_kind: Option<&str>,
    depth: usize,
    limit_nodes: usize,
    limit_edges: usize,
) -> Response {
    let t0 = Instant::now();
    let store = match state.ensure_workspace_runtime(&workspace) {
        Some(s) => s,
        None => {
            return viewer_api::error::ApiError::not_found("workspace", request_id)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    let depth_limit = depth.min(8);
    let edge_kind_filter = edge_kind.unwrap_or("all");
    let t_phase = Instant::now();

    // ── Phase 1: load all edges once, build adjacency map ─────────────────
    // The edge table is needed for BFS topology. Loading it once avoids
    // per-node DB calls. Ticket metadata is NOT loaded upfront — we only
    // fetch data for the ~20-100 nodes actually in the subgraph, not all
    // 360+ tickets in the workspace.
    let all_edges = match store.list_all_edges() {
        Ok(e) => e,
        Err(e) => return storage_err(e, request_id),
    };

    // adj[node] = Vec<(neighbor, edge_from, edge_to, edge_kind)>
    let mut adj: HashMap<Uuid, Vec<AdjEntry>> = HashMap::new();
    for edge in &all_edges {
        let kind_ok = edge_kind_filter == "all" || edge.kind == edge_kind_filter;
        if !kind_ok {
            continue;
        }
        adj.entry(edge.from).or_default().push((edge.to, edge.from, edge.to, edge.kind.clone()));
        adj.entry(edge.to).or_default().push((edge.from, edge.from, edge.to, edge.kind.clone()));
    }

    // ── Phase 2: BFS — collect visited node IDs and edges ─────────────────
    // No DB calls here; only the in-memory adjacency map is used.
    let t_bfs_start = t_phase.elapsed().as_millis();
    let mut visited: HashMap<Uuid, usize> = HashMap::new(); // node → depth
    let mut edges_set: Vec<EdgeItem> = Vec::new();
    let mut truncated = false;
    let mut max_depth_reached = 0;

    let mut queue: VecDeque<(Uuid, usize)> = VecDeque::new();
    queue.push_back((root, 0));

    while let Some((current_id, depth)) = queue.pop_front() {
        if visited.contains_key(&current_id) {
            continue;
        }
        if visited.len() >= limit_nodes {
            truncated = true;
            break;
        }

        visited.insert(current_id, depth);
        max_depth_reached = max_depth_reached.max(depth);

        if depth >= depth_limit {
            continue;
        }

        if let Some(neighbors) = adj.get(&current_id) {
            for (neighbor, edge_from, edge_to, edge_kind) in neighbors {
                let is_outbound = *edge_from == current_id;
                let dir_ok = match direction {
                    "out" => is_outbound,
                    "in" => !is_outbound,
                    _ => true,
                };
                if !dir_ok {
                    continue;
                }
                if edges_set.len() < limit_edges {
                    edges_set.push(EdgeItem {
                        from: edge_from.to_string(),
                        to: edge_to.to_string(),
                        kind: edge_kind.clone(),
                    });
                }
                if !visited.contains_key(neighbor) {
                    queue.push_back((*neighbor, depth + 1));
                }
            }
        }
    }

    // ── Phase 3: fetch ticket metadata only for visited nodes ──────────────
    // A single read transaction fetches all N visited nodes at once —
    // orders of magnitude faster than N separate begin_read() calls.
    let t_meta_start = t_phase.elapsed().as_millis();
    let node_ids: Vec<Uuid> = visited.keys().copied().collect();
    let meta_map = match store.get_indexed_many(&node_ids) {
        Ok(m) => m,
        Err(e) => return storage_err(e, request_id),
    };

    let mut nodes: Vec<NodeItem> = visited
        .iter()
        .map(|(node_id, depth)| {
            if let Some(t) = meta_map.get(node_id) {
                NodeItem {
                    id: node_id.to_string(),
                    title: t.title.clone(),
                    state: t.state.clone(),
                    depth: *depth,
                }
            } else {
                NodeItem {
                    id: node_id.to_string(),
                    title: None,
                    state: None,
                    depth: *depth,
                }
            }
        })
        .collect();
    nodes.sort_by_key(|n| n.depth);

    // Deduplicate edges
    edges_set.dedup_by(|a, b| a.from == b.from && a.to == b.to && a.kind == b.kind);

    let t_end = t_phase.elapsed().as_millis();
    let stats = SubgraphStats {
        nodes_returned: nodes.len(),
        edges_returned: edges_set.len(),
        max_depth_reached,
        phase1_edges_ms: t_bfs_start,
        phase2_bfs_ms: t_meta_start - t_bfs_start,
        phase3_meta_ms: t_end - t_meta_start,
        total_ms: t0.elapsed().as_millis(),
    };

    tracing::info!(
        workspace = %workspace,
        root = %root,
        nodes = nodes.len(),
        edges = edges_set.len(),
        truncated,
        elapsed_ms = t0.elapsed().as_millis(),
        phase1_edges_ms = t_bfs_start,
        phase2_bfs_ms = t_meta_start - t_bfs_start,
        phase3_meta_ms = t_phase.elapsed().as_millis() - t_meta_start,
        "subgraph timing"
    );

    Json(SubgraphResponse {
        request_id: request_id.to_string(),
        workspace,
        nodes,
        edges: edges_set,
        truncated,
        next_cursor: None,
        stats,
    })
    .into_response()
}

// ── Health check endpoint ──────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct HealthCheckQuery {
    pub workspace: String,
    pub root: Option<Uuid>,
    #[serde(default)]
    pub all: Option<bool>,
    #[serde(default = "default_health_depth")]
    pub depth: usize,
    pub direction: Option<String>,
}

fn default_health_depth() -> usize { 6 }

#[derive(Serialize)]
pub struct HealthCheckResponse {
    pub request_id: String,
    pub workspace: String,
    pub tickets_checked: usize,
    pub finding_count: usize,
    pub summary: BTreeMap<String, u64>,
    pub findings: Vec<serde_json::Value>,
}

pub async fn health_check(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<HealthCheckQuery>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&params.workspace) {
        Some(s) => s,
        None => {
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    let all_edges = match store.list_all_edges() {
        Ok(e) => e,
        Err(e) => return storage_err(e, &rid.0),
    };

    let is_all = params.all.unwrap_or(false);

    // Collect tickets in scope.
    let tickets = if is_all {
        match store.list(None, None, None) {
            Ok(t) => t,
            Err(e) => return storage_err(e, &rid.0),
        }
    } else {
        let root = match params.root {
            Some(r) => r,
            None => {
                return viewer_api::error::ApiError::bad_request(
                    "missing_parameter",
                    "one of 'root' or 'all=true' is required",
                    &rid.0,
                )
                .into_response_with_status(StatusCode::BAD_REQUEST);
            }
        };
        let depth_limit = params.depth.min(8);
        let direction = params.direction.as_deref().unwrap_or("out");

        let mut visited: HashSet<Uuid> = HashSet::new();
        let mut collected_ids: Vec<Uuid> = Vec::new();
        let mut queue: VecDeque<(Uuid, usize)> = VecDeque::new();
        queue.push_back((root, 0));

        while let Some((current_id, d)) = queue.pop_front() {
            if !visited.insert(current_id) {
                continue;
            }
            collected_ids.push(current_id);
            if d >= depth_limit {
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
                let dir_ok = match direction {
                    "out" => is_outbound,
                    "in" => !is_outbound,
                    _ => true,
                };
                if dir_ok && !visited.contains(&neighbor) {
                    queue.push_back((neighbor, d + 1));
                }
            }
        }

        collected_ids
            .iter()
            .filter_map(|id| store.get_indexed(id).ok().flatten())
            .filter(|t| !t.deleted)
            .collect()
    };

    // Build lookup sets.
    let ticket_ids: HashSet<Uuid> = tickets.iter().map(|t| t.id).collect();
    let done_states: HashSet<&str> = ["done", "cancelled"].into_iter().collect();
    let done_ids: HashSet<Uuid> = tickets
        .iter()
        .filter(|t| t.state.as_deref().map(|s| done_states.contains(s)).unwrap_or(false))
        .map(|t| t.id)
        .collect();

    let mut unresolved_deps: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for edge in &all_edges {
        if edge.kind == "depends_on" && ticket_ids.contains(&edge.from) && !done_ids.contains(&edge.to) {
            unresolved_deps.entry(edge.from).or_default().push(edge.to);
        }
    }

    let mut findings: Vec<serde_json::Value> = Vec::new();
    let mut summary: BTreeMap<String, u64> = BTreeMap::new();

    for t in &tickets {
        if done_ids.contains(&t.id) {
            continue;
        }
        let short_id = &t.id.to_string()[..8];
        let title = t.title.as_deref().unwrap_or("?");

        let desc = TicketFs::read_description(&t.path);
        if desc.is_none() {
            *summary.entry("missing_description".into()).or_insert(0) += 1;
            findings.push(serde_json::json!({
                "ticket_id": t.id, "short_id": short_id, "title": title,
                "check": "missing_description", "severity": "warning",
                "message": "No description.md file — ticket lacks detailed context.",
            }));
        } else if let Some(ref body) = desc {
            let trimmed_len = body.trim().len();
            if trimmed_len < 50 {
                *summary.entry("short_description".into()).or_insert(0) += 1;
                findings.push(serde_json::json!({
                    "ticket_id": t.id, "short_id": short_id, "title": title,
                    "check": "short_description", "severity": "info",
                    "message": format!("description.md is very short ({trimmed_len} chars) — consider adding more detail."),
                }));
            }
        }

        if t.title.is_none() || t.title.as_deref() == Some("") {
            *summary.entry("missing_title".into()).or_insert(0) += 1;
            findings.push(serde_json::json!({
                "ticket_id": t.id, "short_id": short_id, "title": "(none)",
                "check": "missing_title", "severity": "error",
                "message": "Ticket has no title.",
            }));
        }

        let state = t.state.as_deref().unwrap_or("");
        let has_unresolved = unresolved_deps.contains_key(&t.id);
        if has_unresolved && state != "new" {
            let dep_count = unresolved_deps[&t.id].len();
            *summary.entry("unblocked_with_deps".into()).or_insert(0) += 1;
            findings.push(serde_json::json!({
                "ticket_id": t.id, "short_id": short_id, "title": title,
                "check": "unblocked_with_deps", "severity": "info",
                "message": format!("Ticket is '{state}' but has {dep_count} unresolved dependency/ies — may need state review."),
            }));
        }

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
                    *summary.entry("dangling_edge".into()).or_insert(0) += 1;
                    findings.push(serde_json::json!({
                        "ticket_id": t.id, "short_id": short_id, "title": title,
                        "check": "dangling_edge", "severity": "error",
                        "message": format!("depends_on edge points to {target_short} which is deleted or missing."),
                    }));
                }
            }
        }
    }

    let total_checked = tickets.iter().filter(|t| !done_ids.contains(&t.id)).count();

    Json(HealthCheckResponse {
        request_id: rid.0.clone(),
        workspace: params.workspace,
        tickets_checked: total_checked,
        finding_count: findings.len(),
        summary,
        findings,
    })
    .into_response()
}
