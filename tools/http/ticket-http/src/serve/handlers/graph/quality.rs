use std::{
    collections::{
        HashMap,
        HashSet,
        VecDeque,
    },
    sync::Arc,
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
    },
};
use uuid::Uuid;

use crate::serve::{
    AppState,
    error::storage_err,
};

use super::{
    HealthCheckQuery,
    HealthCheckResponse,
};

mod findings;

use findings::collect_findings;

struct HealthContext {
    done_ids: HashSet<Uuid>,
    unresolved_deps: HashMap<Uuid, Vec<Uuid>>,
}

pub(super) async fn handle_health_check(
    state: AppState,
    request_id: String,
    params: HealthCheckQuery,
) -> Response {
    let store =
        match resolve_workspace_store(&state, &params.workspace, &request_id) {
            Ok(store) => store,
            Err(response) => return response,
        };

    let all_edges = match store.list_all_edges() {
        Ok(edges) => edges,
        Err(error) => return storage_err(error, &request_id),
    };

    let tickets =
        match tickets_in_scope(&store, &params, &all_edges, &request_id) {
            Ok(tickets) => tickets,
            Err(response) => return response,
        };

    let context = build_health_context(&tickets, &all_edges);
    let (summary, findings) =
        collect_findings(&store, &tickets, &all_edges, &context);
    let tickets_checked = tickets
        .iter()
        .filter(|ticket| !context.done_ids.contains(&ticket.id))
        .count();

    Json(HealthCheckResponse {
        request_id,
        workspace: params.workspace,
        tickets_checked,
        finding_count: findings.len(),
        summary,
        findings,
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

fn tickets_in_scope(
    store: &TicketStore,
    params: &HealthCheckQuery,
    all_edges: &[EdgeRecord],
    request_id: &str,
) -> Result<Vec<IndexedTicket>, Response> {
    if params.all.unwrap_or(false) {
        list_all_tickets(store, request_id)
    } else {
        root_scope_tickets(store, params, all_edges, request_id)
    }
}

fn list_all_tickets(
    store: &TicketStore,
    request_id: &str,
) -> Result<Vec<IndexedTicket>, Response> {
    store
        .list(None, None, None)
        .map_err(|error| storage_err(error, request_id))
}

fn root_scope_tickets(
    store: &TicketStore,
    params: &HealthCheckQuery,
    all_edges: &[EdgeRecord],
    request_id: &str,
) -> Result<Vec<IndexedTicket>, Response> {
    let root = params.root.ok_or_else(|| {
        viewer_api::error::ApiError::bad_request(
            "missing_parameter",
            "one of 'root' or 'all=true' is required",
            request_id,
        )
        .into_response_with_status(StatusCode::BAD_REQUEST)
    })?;

    let ids = collect_scope_ids(
        root,
        params.direction.as_deref().unwrap_or("out"),
        params.depth.min(8),
        all_edges,
    );
    Ok(load_live_tickets(store, &ids))
}

fn collect_scope_ids(
    root: Uuid,
    direction: &str,
    depth_limit: usize,
    all_edges: &[EdgeRecord],
) -> Vec<Uuid> {
    let mut visited = HashSet::new();
    let mut collected_ids = Vec::new();
    let mut queue = VecDeque::from([(root, 0)]);

    while let Some((current_id, depth)) = queue.pop_front() {
        if !visited.insert(current_id) {
            continue;
        }
        collected_ids.push(current_id);
        if depth >= depth_limit {
            continue;
        }

        for edge in all_edges {
            let Some(neighbor) = scope_neighbor(edge, current_id, direction)
            else {
                continue;
            };
            if !visited.contains(&neighbor) {
                queue.push_back((neighbor, depth + 1));
            }
        }
    }

    collected_ids
}

fn scope_neighbor(
    edge: &EdgeRecord,
    current_id: Uuid,
    direction: &str,
) -> Option<Uuid> {
    if edge.kind != "depends_on" && edge.kind != "linked" {
        return None;
    }

    let (neighbor, is_outbound) = edge_neighbor(edge, current_id)?;
    if direction_allows(direction, is_outbound) {
        Some(neighbor)
    } else {
        None
    }
}

fn edge_neighbor(
    edge: &EdgeRecord,
    current_id: Uuid,
) -> Option<(Uuid, bool)> {
    if edge.from == current_id {
        Some((edge.to, true))
    } else if edge.to == current_id {
        Some((edge.from, false))
    } else {
        None
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

fn load_live_tickets(
    store: &TicketStore,
    ids: &[Uuid],
) -> Vec<IndexedTicket> {
    ids.iter()
        .filter_map(|id| store.get_indexed(id).ok().flatten())
        .filter(|ticket| !ticket.deleted)
        .collect()
}

fn build_health_context(
    tickets: &[IndexedTicket],
    all_edges: &[EdgeRecord],
) -> HealthContext {
    let ticket_ids: HashSet<Uuid> =
        tickets.iter().map(|ticket| ticket.id).collect();
    let done_ids = done_ticket_ids(tickets);
    let unresolved_deps =
        unresolved_dependency_map(all_edges, &ticket_ids, &done_ids);

    HealthContext {
        done_ids,
        unresolved_deps,
    }
}

fn done_ticket_ids(tickets: &[IndexedTicket]) -> HashSet<Uuid> {
    let done_states: HashSet<&str> =
        ["done", "cancelled"].into_iter().collect();
    tickets
        .iter()
        .filter(|ticket| {
            ticket
                .state
                .as_deref()
                .map(|state| done_states.contains(state))
                .unwrap_or(false)
        })
        .map(|ticket| ticket.id)
        .collect()
}

fn unresolved_dependency_map(
    all_edges: &[EdgeRecord],
    ticket_ids: &HashSet<Uuid>,
    done_ids: &HashSet<Uuid>,
) -> HashMap<Uuid, Vec<Uuid>> {
    let mut unresolved = HashMap::new();
    for edge in all_edges {
        if edge.kind != "depends_on" {
            continue;
        }
        if !ticket_ids.contains(&edge.from) || done_ids.contains(&edge.to) {
            continue;
        }
        unresolved
            .entry(edge.from)
            .or_insert_with(Vec::new)
            .push(edge.to);
    }
    unresolved
}
