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
    error::storage_err,
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
    edges: Vec<EdgeItem>,
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
    tokio::task::spawn_blocking(move || bfs_graph(state, &request_id, request))
        .await
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
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

    let nodes = match build_nodes(&store, &traversal.visited, request_id) {
        Ok(nodes) => nodes,
        Err(response) => return response,
    };
    let edges = dedupe_edges(traversal.edges);
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
                    edges.push(EdgeItem {
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

fn build_nodes(
    store: &TicketStore,
    visited: &HashMap<Uuid, usize>,
    request_id: &str,
) -> Result<Vec<NodeItem>, Response> {
    let node_ids: Vec<Uuid> = visited.keys().copied().collect();
    let meta_map = store
        .get_indexed_many(&node_ids)
        .map_err(|error| storage_err(error, request_id))?;

    let mut nodes: Vec<NodeItem> = visited
        .iter()
        .map(|(node_id, depth)| {
            build_node_item(*node_id, *depth, meta_map.get(node_id))
        })
        .collect();
    nodes.sort_by_key(|node| node.depth);
    Ok(nodes)
}

fn build_node_item(
    node_id: Uuid,
    depth: usize,
    ticket: Option<&IndexedTicket>,
) -> NodeItem {
    if let Some(ticket) = ticket {
        return NodeItem {
            id: node_id.to_string(),
            title: ticket.title.clone(),
            state: ticket.state.clone(),
            depth,
            ticket_type: Some(ticket.type_id.clone()),
            priority: ticket_priority(ticket),
        };
    }

    NodeItem {
        id: node_id.to_string(),
        title: None,
        state: None,
        depth,
        ticket_type: None,
        priority: None,
    }
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

fn dedupe_edges(edges: Vec<EdgeItem>) -> Vec<EdgeItem> {
    let mut seen = HashSet::new();
    edges
        .into_iter()
        .filter(|edge| {
            seen.insert((edge.from.clone(), edge.to.clone(), edge.kind.clone()))
        })
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
