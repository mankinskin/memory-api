use std::{
    collections::{
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
    health::collect_findings,
    model::edge::EdgeRecord,
    query_helpers::{
        apply_field_filters,
        parse_where_filters,
        resolve_uuid_with_prefix,
    },
    storage::{
        indexed::IndexedTicket,
        store::TicketStore,
    },
    workflow::WorkflowModel,
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
    let field_filters = match parse_where_filters(&params.where_clauses) {
        Ok(filters) => filters,
        Err(message) => {
            return viewer_api::error::ApiError::bad_request(
                "invalid_where",
                message,
                &request_id,
            )
            .into_response_with_status(StatusCode::BAD_REQUEST);
        },
    };
    let tickets = apply_field_filters(tickets, &field_filters);

    let workflow = match WorkflowModel::build(
        &store,
        store
            .list(None, None, None)
            .map_err(|e| storage_err(e, &request_id))
            .ok()
            .unwrap_or_default(),
        all_edges.clone(),
    ) {
        Ok(w) => w,
        Err(error) => return storage_err(error, &request_id),
    };
    let report = collect_findings(&store, &tickets, &all_edges, &workflow);
    let tickets_checked = tickets
        .iter()
        .filter(|ticket| {
            !matches!(ticket.state.as_deref(), Some("done") | Some("cancelled"))
        })
        .count();
    let findings: Vec<serde_json::Value> = report
        .findings
        .into_iter()
        .map(|f| serde_json::to_value(f).unwrap_or_default())
        .collect();

    Json(HealthCheckResponse {
        request_id,
        workspace: params.workspace,
        tickets_checked,
        finding_count: findings.len(),
        summary: report.summary,
        findings,
    })
    .into_response()
}

fn resolve_workspace_store(
    state: &AppState,
    workspace: &str,
    request_id: &str,
) -> Result<Arc<TicketStore>, Response> {
    state
        .resolve_public_workspace_request(workspace, request_id)
        .map(|(_, store)| store)
}

fn tickets_in_scope(
    store: &TicketStore,
    params: &HealthCheckQuery,
    all_edges: &[EdgeRecord],
    request_id: &str,
) -> Result<Vec<IndexedTicket>, Response> {
    if !params.ids.is_empty() {
        return explicit_tickets(store, &params.ids, request_id);
    }
    if params.all.unwrap_or(false) {
        list_all_tickets(store, request_id)
    } else {
        root_scope_tickets(store, params, all_edges, request_id)
    }
}

fn explicit_tickets(
    store: &TicketStore,
    ids: &[String],
    request_id: &str,
) -> Result<Vec<IndexedTicket>, Response> {
    let mut resolved = Vec::new();
    for id in ids {
        let uuid = resolve_uuid_with_prefix(store, id)
            .map_err(|error| storage_err(error, request_id))?;
        resolved.push(uuid);
    }
    Ok(load_live_tickets(store, &resolved))
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
        .collect()
}
