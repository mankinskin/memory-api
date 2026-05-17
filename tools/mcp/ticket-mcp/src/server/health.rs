use std::collections::{
    HashMap,
    HashSet,
    VecDeque,
};

use ticket_api::{
    model::edge::EdgeRecord,
    storage::indexed::IndexedTicket,
};

use super::*;

mod findings;

pub(super) struct HealthContext {
    pub(super) tickets: Vec<IndexedTicket>,
    pub(super) all_edges: Vec<EdgeRecord>,
    pub(super) done_ids: HashSet<Uuid>,
    pub(super) unresolved_deps: HashMap<Uuid, Vec<Uuid>>,
}

impl TicketServer {
    pub(crate) async fn run_health_checks(
        &self,
        workspace: &str,
        root: Option<&str>,
        all: bool,
        ids: &[String],
        depth: Option<usize>,
        direction: Option<&str>,
    ) -> Result<CallToolResult, McpError> {
        let workspace = workspace.to_owned();
        let root = root.map(str::to_owned);
        let ids = ids.to_owned();
        let direction = direction.map(str::to_owned);

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
            let context = build_health_context(tickets, all_edges);
            let report = findings::collect_findings(store, &context)?;
            let tickets_checked = context
                .tickets
                .iter()
                .filter(|ticket| !context.done_ids.contains(&ticket.id))
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
            if !ticket.deleted {
                tickets.push(ticket);
            }
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
        .filter(|ticket| !ticket.deleted)
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

fn build_health_context(
    tickets: Vec<IndexedTicket>,
    all_edges: Vec<EdgeRecord>,
) -> HealthContext {
    let done_ids = done_ticket_ids(&tickets);
    let ticket_ids: HashSet<Uuid> =
        tickets.iter().map(|ticket| ticket.id).collect();
    let mut unresolved_deps: HashMap<Uuid, Vec<Uuid>> = HashMap::new();

    for edge in &all_edges {
        if edge.kind == "depends_on"
            && ticket_ids.contains(&edge.from)
            && !done_ids.contains(&edge.to)
        {
            unresolved_deps.entry(edge.from).or_default().push(edge.to);
        }
    }

    HealthContext {
        tickets,
        all_edges,
        done_ids,
        unresolved_deps,
    }
}

fn done_ticket_ids(tickets: &[IndexedTicket]) -> HashSet<Uuid> {
    tickets
        .iter()
        .filter(|ticket| is_done_state(ticket.state.as_deref()))
        .map(|ticket| ticket.id)
        .collect()
}

fn is_done_state(state: Option<&str>) -> bool {
    matches!(state, Some("done" | "cancelled"))
}
