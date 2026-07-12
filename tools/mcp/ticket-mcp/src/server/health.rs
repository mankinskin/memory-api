use std::collections::{
    HashSet,
    VecDeque,
};

use ticket_api::{
    health::collect_findings,
    model::edge::EdgeRecord,
    query_helpers::{
        apply_field_filters,
        parse_where_filters,
    },
    storage::indexed::IndexedTicket,
    workflow::WorkflowModel,
};

use super::*;

impl TicketServer {
    pub async fn run_health_checks(
        &self,
        workspace: &str,
        root: Option<&str>,
        all: bool,
        ids: &[String],
        depth: Option<usize>,
        direction: Option<&str>,
        where_clauses: &[String],
    ) -> Result<CallToolResult, McpError> {
        let workspace = workspace.to_owned();
        let root = root.map(str::to_owned);
        let ids = ids.to_owned();
        let direction = direction.map(str::to_owned);
        let where_clauses = where_clauses.to_owned();

        self.with_store_ext(&workspace.clone(), move |store| {
            let all_edges = store.list_all_edges().map_err(Self::store_err)?;
            let tickets = tickets_in_scope(
                store,
                root.as_deref(),
                all,
                &ids,
                depth,
                direction.as_deref(),
                &all_edges,
            )?;
            let filters = parse_where_filters(&where_clauses).map_err(|message| {
                McpError::invalid_params(message, None)
            })?;
            let tickets = apply_field_filters(tickets, &filters);
            let workflow = WorkflowModel::build(
                store,
                store.list(None, None, None).map_err(Self::store_err)?,
                all_edges.clone(),
            )
            .map_err(Self::store_err)?;
            let report =
                collect_findings(store, &tickets, &all_edges, &workflow);
            let tickets_checked = tickets
                .iter()
                .filter(|ticket| {
                    !matches!(
                        ticket.state.as_deref(),
                        Some("done") | Some("cancelled")
                    )
                })
                .count();

            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "tickets_checked": tickets_checked,
                "finding_count": report.findings.len(),
                "summary": report.summary,
                "findings": report.findings,
            }))
        })
        .await
    }
}

fn tickets_in_scope(
    store: &TicketStore,
    root: Option<&str>,
    all: bool,
    ids: &[String],
    depth: Option<usize>,
    direction: Option<&str>,
    all_edges: &[EdgeRecord],
) -> Result<Vec<IndexedTicket>, McpError> {
    if !ids.is_empty() {
        return explicit_tickets(store, ids);
    }
    if all {
        return store
            .list(None, None, None)
            .map_err(TicketServer::store_err);
    }

    let root_str = root.ok_or_else(|| {
        McpError::invalid_params(
            "one of 'root', 'all', or 'ids' is required",
            None,
        )
    })?;

    root_scope_tickets(
        store,
        root_str,
        depth.unwrap_or(6).min(8),
        direction.unwrap_or("out"),
        all_edges,
    )
}

fn explicit_tickets(
    store: &TicketStore,
    ids: &[String],
) -> Result<Vec<IndexedTicket>, McpError> {
    let mut tickets = Vec::new();

    for id_str in ids {
        let id = TicketServer::resolve_uuid_with(store, id_str)?;
        if let Some(ticket) =
            store.get_indexed(&id).map_err(TicketServer::store_err)?
        {
            tickets.push(ticket);
        }
    }

    Ok(tickets)
}

fn root_scope_tickets(
    store: &TicketStore,
    root_str: &str,
    depth_limit: usize,
    direction: &str,
    all_edges: &[EdgeRecord],
) -> Result<Vec<IndexedTicket>, McpError> {
    let root_id = TicketServer::resolve_uuid_with(store, root_str)?;
    let scope_ids =
        collect_scope_ids(root_id, depth_limit, direction, all_edges);

    Ok(scope_ids
        .iter()
        .filter_map(|id| store.get_indexed(id).ok().flatten())
        .collect())
}

fn collect_scope_ids(
    root_id: Uuid,
    depth_limit: usize,
    direction: &str,
    all_edges: &[EdgeRecord],
) -> Vec<Uuid> {
    let mut visited = HashSet::new();
    let mut collected_ids = Vec::new();
    let mut queue = VecDeque::new();
    queue.push_back((root_id, 0));

    while let Some((current_id, depth)) = queue.pop_front() {
        if !visited.insert(current_id) {
            continue;
        }
        collected_ids.push(current_id);
        if depth >= depth_limit {
            continue;
        }

        for edge in all_edges {
            if !relevant_scope_edge(edge) {
                continue;
            }
            let Some((neighbor, is_outbound)) =
                adjacent_ticket(edge, current_id)
            else {
                continue;
            };
            if direction_matches(direction, is_outbound)
                && !visited.contains(&neighbor)
            {
                queue.push_back((neighbor, depth + 1));
            }
        }
    }

    collected_ids
}

fn relevant_scope_edge(edge: &EdgeRecord) -> bool {
    edge.kind == "depends_on" || edge.kind == "linked"
}

fn adjacent_ticket(
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

fn direction_matches(
    direction: &str,
    is_outbound: bool,
) -> bool {
    match direction {
        "out" => is_outbound,
        "in" => !is_outbound,
        _ => true,
    }
}
