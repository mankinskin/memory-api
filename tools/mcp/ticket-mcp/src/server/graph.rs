use std::collections::{
    HashMap,
    HashSet,
    VecDeque,
};

use ticket_api::model::edge::EdgeRecord;

use super::{
    types::*,
    *,
};

struct GraphRequest {
    workspace: String,
    root_str: String,
    direction: String,
    edge_kind: Option<String>,
    depth_limit: usize,
    node_limit: usize,
    edge_limit: usize,
}

struct TraversalResult {
    nodes: Vec<NodeItem>,
    edges: Vec<EdgeItem>,
    truncated: bool,
    max_depth_reached: usize,
}

#[derive(Clone)]
struct Neighbor {
    id: Uuid,
    edge: EdgeItem,
}

struct TraversalState {
    visited: HashSet<Uuid>,
    nodes: Vec<NodeItem>,
    edges: Vec<EdgeItem>,
    queue: VecDeque<(Uuid, usize)>,
    truncated: bool,
    max_depth_reached: usize,
}

impl TraversalState {
    fn new(root: Uuid) -> Self {
        let mut queue = VecDeque::new();
        queue.push_back((root, 0));
        Self {
            visited: HashSet::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            queue,
            truncated: false,
            max_depth_reached: 0,
        }
    }
}

impl TicketServer {
    pub(crate) async fn bfs_graph(
        &self,
        workspace: String,
        root_str: &str,
        direction: &str,
        edge_kind: Option<&str>,
        depth: Option<usize>,
        limit_nodes: Option<usize>,
        limit_edges: Option<usize>,
    ) -> Result<CallToolResult, McpError> {
        let request = GraphRequest {
            workspace,
            root_str: root_str.to_owned(),
            direction: direction.to_owned(),
            edge_kind: edge_kind.map(str::to_owned),
            depth_limit: depth.unwrap_or(2).min(8),
            node_limit: limit_nodes.unwrap_or(500),
            edge_limit: limit_edges.unwrap_or(2000),
        };

        self.with_store_ext(&request.workspace.clone(), move |store| {
            let root = Self::resolve_uuid_with(store, &request.root_str)?;
            let all_edges = store.list_all_edges().map_err(Self::store_err)?;
            let traversal = traverse_graph(store, root, &all_edges, &request)?;
            let stats = SubgraphStats {
                nodes_returned: traversal.nodes.len(),
                edges_returned: traversal.edges.len(),
                max_depth_reached: traversal.max_depth_reached,
            };
            Self::json_result(&SubgraphResponse {
                workspace: request.workspace,
                nodes: traversal.nodes,
                edges: traversal.edges,
                truncated: traversal.truncated,
                stats,
            })
        })
        .await
    }
}

fn traverse_graph(
    store: &TicketStore,
    root: Uuid,
    all_edges: &[EdgeRecord],
    request: &GraphRequest,
) -> Result<TraversalResult, McpError> {
    let adjacency = build_adjacency(
        all_edges,
        &request.direction,
        request.edge_kind.as_deref(),
    );
    let mut state = TraversalState::new(root);

    while let Some((current_id, current_depth)) = state.queue.pop_front() {
        if !state.visited.insert(current_id) {
            continue;
        }
        if state.nodes.len() >= request.node_limit {
            state.truncated = true;
            break;
        }

        record_node(store, &mut state, current_id, current_depth)?;
        if current_depth >= request.depth_limit {
            continue;
        }

        extend_neighbors(
            &mut state,
            &adjacency,
            current_id,
            current_depth,
            request.edge_limit,
        );
    }

    Ok(TraversalResult {
        nodes: state.nodes,
        edges: dedupe_edges(state.edges),
        truncated: state.truncated,
        max_depth_reached: state.max_depth_reached,
    })
}

fn record_node(
    store: &TicketStore,
    state: &mut TraversalState,
    current_id: Uuid,
    current_depth: usize,
) -> Result<(), McpError> {
    state.max_depth_reached = state.max_depth_reached.max(current_depth);
    state
        .nodes
        .push(build_node(store, current_id, current_depth)?);
    Ok(())
}

fn extend_neighbors(
    state: &mut TraversalState,
    adjacency: &HashMap<Uuid, Vec<Neighbor>>,
    current_id: Uuid,
    current_depth: usize,
    edge_limit: usize,
) {
    let Some(neighbors) = adjacency.get(&current_id) else {
        return;
    };

    for neighbor in neighbors {
        if state.edges.len() < edge_limit {
            state.edges.push(neighbor.edge.clone());
        }
        if !state.visited.contains(&neighbor.id) {
            state.queue.push_back((neighbor.id, current_depth + 1));
        }
    }
}

fn build_adjacency(
    all_edges: &[EdgeRecord],
    direction: &str,
    edge_kind: Option<&str>,
) -> HashMap<Uuid, Vec<Neighbor>> {
    let mut adjacency: HashMap<Uuid, Vec<Neighbor>> = HashMap::new();

    for edge in all_edges {
        if !edge_kind_matches(edge_kind, &edge.kind) {
            continue;
        }
        if direction != "in" {
            adjacency.entry(edge.from).or_default().push(Neighbor {
                id: edge.to,
                edge: edge_item(edge),
            });
        }
        if direction != "out" {
            adjacency.entry(edge.to).or_default().push(Neighbor {
                id: edge.from,
                edge: edge_item(edge),
            });
        }
    }

    adjacency
}

fn edge_kind_matches(
    edge_kind: Option<&str>,
    actual_kind: &str,
) -> bool {
    match edge_kind {
        Some("all") | None => true,
        Some(kind) => kind == actual_kind,
    }
}

fn build_node(
    store: &TicketStore,
    current_id: Uuid,
    current_depth: usize,
) -> Result<NodeItem, McpError> {
    match store.get_indexed(&current_id) {
        Ok(Some(ticket)) => Ok(NodeItem {
            id: current_id.to_string(),
            title: ticket.title,
            state: ticket.state,
            depth: current_depth,
        }),
        Ok(None) => Ok(NodeItem {
            id: current_id.to_string(),
            title: None,
            state: None,
            depth: current_depth,
        }),
        Err(err) => Err(TicketServer::store_err(err)),
    }
}

fn edge_item(edge: &EdgeRecord) -> EdgeItem {
    EdgeItem {
        from: edge.from.to_string(),
        to: edge.to.to_string(),
        kind: edge.kind.clone(),
    }
}

fn dedupe_edges(edges: Vec<EdgeItem>) -> Vec<EdgeItem> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();

    for edge in edges {
        let key = (edge.from.clone(), edge.to.clone(), edge.kind.clone());
        if seen.insert(key) {
            deduped.push(edge);
        }
    }

    deduped
}
