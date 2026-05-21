use std::{
    collections::{
        HashMap,
        HashSet,
        VecDeque,
    },
    sync::Arc,
    time::Instant,
};

use axum::{
    http::StatusCode,
    response::{
        IntoResponse,
        Json,
        Response,
    },
};
use ticket_api::{
    model::edge::EdgeRecord,
    storage::{
        indexed::IndexedTicket,
        store::TicketStore,
        ticket_fs::TicketFs,
    },
};
use uuid::Uuid;

use crate::serve::{
    AppState,
    error::{
        storage_err,
        task_join_err,
    },
    handlers::tickets::{
        TicketRef,
        ticket_ref_from_indexed,
    },
    registry::ResolvedIndexedTicket,
};

use super::{
    EdgeItem,
    NodeItem,
    SubgraphQuery,
    SubgraphResponse,
    SubgraphStats,
    TopgraphQuery,
};

type AdjEntry = (Uuid, Uuid, Uuid, String);

#[derive(Clone)]
struct RawEdgeItem {
    from: Uuid,
    to: Uuid,
    kind: String,
}

struct GraphRequest {
    workspace: String,
    root: Uuid,
    direction: String,
    edge_kind: Option<String>,
    depth: usize,
    limit_nodes: usize,
    limit_edges: usize,
}

struct TraversalResult {
    visited: HashMap<Uuid, usize>,
    edges: Vec<RawEdgeItem>,
    truncated: bool,
    max_depth_reached: usize,
}

impl GraphRequest {
    fn from_subgraph(params: SubgraphQuery) -> Self {
        Self {
            workspace: params.workspace,
            root: params.root,
            direction: params.direction.unwrap_or_else(|| "both".to_string()),
            edge_kind: params.edge_kind,
            depth: params.depth,
            limit_nodes: params.limit_nodes,
            limit_edges: params.limit_edges,
        }
    }

    fn from_topgraph(params: TopgraphQuery) -> Self {
        Self {
            workspace: params.workspace,
            root: params.root,
            direction: params.direction.unwrap_or_else(|| "in".to_string()),
            edge_kind: params.edge_kind,
            depth: params.depth,
            limit_nodes: params.limit_nodes,
            limit_edges: params.limit_edges,
        }
    }

    fn depth_limit(&self) -> usize {
        self.depth.min(8)
    }

    fn edge_kind_filter(&self) -> &str {
        self.edge_kind.as_deref().unwrap_or("all")
    }
}

pub(super) async fn handle_subgraph(
    state: AppState,
    request_id: String,
    params: SubgraphQuery,
) -> Response {
    tracing::debug!(
        workspace = %params.workspace,
        root = %params.root,
        depth = params.depth,
        request_id = %request_id,
        "subgraph request received"
    );
    run_graph_request(state, request_id, GraphRequest::from_subgraph(params))
        .await
}

pub(super) async fn handle_topgraph(
    state: AppState,
    request_id: String,
    params: TopgraphQuery,
) -> Response {
    run_graph_request(state, request_id, GraphRequest::from_topgraph(params))
        .await
}

async fn run_graph_request(
    state: AppState,
    request_id: String,
    request: GraphRequest,
) -> Response {
    let task_request_id = request_id.clone();
    tokio::task::spawn_blocking(move || {
        bfs_graph(state, &task_request_id, request)
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "graph traversal request"))
}

fn bfs_graph(
    state: AppState,
    request_id: &str,
    request: GraphRequest,
) -> Response {
    let total_timer = Instant::now();
    let store =
        match resolve_workspace_store(&state, &request.workspace, request_id) {
            Ok(store) => store,
            Err(response) => return response,
        };

    let phase_timer = Instant::now();
    let all_edges = match store.list_all_edges() {
        Ok(edges) => edges,
        Err(error) => return storage_err(error, request_id),
    };
    let adjacency = build_adjacency(&all_edges, request.edge_kind_filter());
    let phase1_edges_ms = phase_timer.elapsed().as_millis();

    let traversal = traverse_graph(
        &adjacency,
        request.root,
        request.direction.as_str(),
        request.depth_limit(),
        request.limit_nodes,
        request.limit_edges,
    );
    let phase2_end_ms = phase_timer.elapsed().as_millis();

    let resolved = match resolve_graph_tickets(
        &state,
        &request.workspace,
        &traversal,
        request_id,
    ) {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };

    let nodes = match build_nodes(
        &resolved,
        &traversal.visited,
        &request.workspace,
        request_id,
    ) {
        Ok(nodes) => nodes,
        Err(response) => return response,
    };
    let edges = match build_edges(
        dedupe_edges(traversal.edges),
        &resolved,
        &request.workspace,
        request_id,
    ) {
        Ok(edges) => edges,
        Err(response) => return response,
    };
    let phase3_end_ms = phase_timer.elapsed().as_millis();

    let stats = SubgraphStats {
        nodes_returned: nodes.len(),
        edges_returned: edges.len(),
        max_depth_reached: traversal.max_depth_reached,
        phase1_edges_ms,
        phase2_bfs_ms: phase2_end_ms - phase1_edges_ms,
        phase3_meta_ms: phase3_end_ms - phase2_end_ms,
        total_ms: total_timer.elapsed().as_millis(),
    };

    log_subgraph_timing(
        &request.workspace,
        request.root,
        &nodes,
        &edges,
        traversal.truncated,
        &stats,
    );

    Json(SubgraphResponse {
        request_id: request_id.to_string(),
        active_workspace: request.workspace.clone(),
        workspace: request.workspace,
        nodes,
        edges,
        truncated: traversal.truncated,
        next_cursor: None,
        stats,
    })
    .into_response()
}

fn resolve_workspace_store(
    state: &AppState,
    workspace: &str,
    request_id: &str,
) -> Result<Arc<TicketStore>, Response> {
    state.ensure_workspace_runtime(workspace).ok_or_else(|| {
        viewer_api::error::ApiError::not_found("workspace", request_id)
            .into_response_with_status(StatusCode::NOT_FOUND)
    })
}

fn build_adjacency(
    all_edges: &[EdgeRecord],
    edge_kind_filter: &str,
) -> HashMap<Uuid, Vec<AdjEntry>> {
    let mut adjacency = HashMap::new();
    for edge in all_edges {
        if edge_kind_filter != "all" && edge.kind != edge_kind_filter {
            continue;
        }

        adjacency.entry(edge.from).or_insert_with(Vec::new).push((
            edge.to,
            edge.from,
            edge.to,
            edge.kind.clone(),
        ));
        adjacency.entry(edge.to).or_insert_with(Vec::new).push((
            edge.from,
            edge.from,
            edge.to,
            edge.kind.clone(),
        ));
    }
    adjacency
}

fn traverse_graph(
    adjacency: &HashMap<Uuid, Vec<AdjEntry>>,
    root: Uuid,
    direction: &str,
    depth_limit: usize,
    limit_nodes: usize,
    limit_edges: usize,
) -> TraversalResult {
    let mut visited = HashMap::new();
    let mut edges = Vec::new();
    let mut truncated = false;
    let mut max_depth_reached = 0;
    let mut queue = VecDeque::from([(root, 0)]);

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

        if let Some(neighbors) = adjacency.get(&current_id) {
            for (neighbor, edge_from, edge_to, edge_kind) in neighbors {
                if !direction_allows(direction, *edge_from == current_id) {
                    continue;
                }
                if edges.len() < limit_edges {
                    edges.push(RawEdgeItem {
                        from: *edge_from,
                        to: *edge_to,
                        kind: edge_kind.clone(),
                    });
                }
                if !visited.contains_key(neighbor) {
                    queue.push_back((*neighbor, depth + 1));
                }
            }
        }
    }

    TraversalResult {
        visited,
        edges,
        truncated,
        max_depth_reached,
    }
}

fn direction_allows(
    direction: &str,
    is_outbound: bool,
) -> bool {
    match direction {
        "out" => is_outbound,
        "in" => !is_outbound,
        _ => true,
    }
}

fn resolve_graph_tickets(
    state: &AppState,
    active_workspace: &str,
    traversal: &TraversalResult,
    request_id: &str,
) -> Result<HashMap<Uuid, ResolvedIndexedTicket>, Response> {
    let mut ids: Vec<Uuid> = traversal.visited.keys().copied().collect();
    for edge in &traversal.edges {
        ids.push(edge.from);
        ids.push(edge.to);
    }
    ids.sort();
    ids.dedup();
    state
        .registry
        .resolve_indexed_many(active_workspace, &ids)
        .map_err(|error| storage_err(error, request_id))
}

fn build_nodes(
    resolved: &HashMap<Uuid, ResolvedIndexedTicket>,
    visited: &HashMap<Uuid, usize>,
    active_workspace: &str,
    request_id: &str,
) -> Result<Vec<NodeItem>, Response> {
    let mut nodes = visited
        .iter()
        .map(|(node_id, depth)| {
            build_node_item(
                *node_id,
                *depth,
                resolved.get(node_id),
                active_workspace,
                request_id,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    nodes.sort_by_key(|node| node.depth);
    Ok(nodes)
}

fn build_node_item(
    node_id: Uuid,
    depth: usize,
    ticket: Option<&ResolvedIndexedTicket>,
    active_workspace: &str,
    request_id: &str,
) -> Result<NodeItem, Response> {
    if let Some(ticket) = ticket {
        let ticket_ref = ticket_ref_from_indexed(
            &ticket.store,
            &ticket.workspace,
            &ticket.ticket,
        )
        .map_err(|error| storage_err(error, request_id))?;
        return Ok(NodeItem {
            id: node_id.to_string(),
            ticket_ref,
            title: ticket.ticket.title.clone(),
            state: ticket.ticket.state.clone(),
            depth,
            ticket_type: Some(ticket.ticket.type_id.clone()),
            priority: ticket_priority(&ticket.ticket),
        });
    }

    Ok(NodeItem {
        id: node_id.to_string(),
        ticket_ref: fallback_ticket_ref(active_workspace, node_id),
        title: None,
        state: None,
        depth,
        ticket_type: None,
        priority: None,
    })
}

fn ticket_priority(ticket: &IndexedTicket) -> Option<String> {
    TicketFs::read(&ticket.path).ok().and_then(|manifest| {
        manifest
            .extra
            .get("priority")
            .and_then(|value| value.as_str())
            .map(|priority| priority.to_string())
    })
}

fn build_edges(
    edges: Vec<RawEdgeItem>,
    resolved: &HashMap<Uuid, ResolvedIndexedTicket>,
    active_workspace: &str,
    request_id: &str,
) -> Result<Vec<EdgeItem>, Response> {
    edges
        .into_iter()
        .map(|edge| {
            Ok(EdgeItem {
                from: edge.from.to_string(),
                to: edge.to.to_string(),
                from_ref: resolve_edge_ref(
                    resolved,
                    active_workspace,
                    edge.from,
                    request_id,
                )?,
                to_ref: resolve_edge_ref(
                    resolved,
                    active_workspace,
                    edge.to,
                    request_id,
                )?,
                kind: edge.kind,
            })
        })
        .collect()
}

fn resolve_edge_ref(
    resolved: &HashMap<Uuid, ResolvedIndexedTicket>,
    active_workspace: &str,
    id: Uuid,
    request_id: &str,
) -> Result<TicketRef, Response> {
    match resolved.get(&id) {
        Some(ticket) => ticket_ref_from_indexed(
            &ticket.store,
            &ticket.workspace,
            &ticket.ticket,
        )
        .map_err(|error| storage_err(error, request_id)),
        None => Ok(fallback_ticket_ref(active_workspace, id)),
    }
}

fn fallback_ticket_ref(
    active_workspace: &str,
    id: Uuid,
) -> TicketRef {
    TicketRef {
        workspace: active_workspace.to_string(),
        id: id.to_string(),
    }
}

fn dedupe_edges(edges: Vec<RawEdgeItem>) -> Vec<RawEdgeItem> {
    let mut seen = HashSet::new();
    edges
        .into_iter()
        .filter(|edge| seen.insert((edge.from, edge.to, edge.kind.clone())))
        .collect()
}

fn log_subgraph_timing(
    workspace: &str,
    root: Uuid,
    nodes: &[NodeItem],
    edges: &[EdgeItem],
    truncated: bool,
    stats: &SubgraphStats,
) {
    tracing::info!(
        workspace = %workspace,
        root = %root,
        nodes = nodes.len(),
        edges = edges.len(),
        truncated,
        elapsed_ms = stats.total_ms,
        phase1_edges_ms = stats.phase1_edges_ms,
        phase2_bfs_ms = stats.phase2_bfs_ms,
        phase3_meta_ms = stats.phase3_meta_ms,
        "subgraph timing"
    );
}
