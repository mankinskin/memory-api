use std::{
    collections::{
        HashSet,
        VecDeque,
    },
    io::BufRead,
};

use serde_json::{
    Value,
    json,
};
use ticket_api::{
    health::collect_findings,
    model::edge::EdgeRecord,
    storage::{
        indexed::IndexedTicket,
        store::TicketStore,
    },
    workflow::WorkflowModel,
};
use uuid::Uuid;

use crate::cli::{
    CliRunError,
    HealthArgs,
    helpers::parse_fields,
};

pub(super) fn run(
    args: HealthArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let all_edges = store.list_all_edges()?;
    let field_filters = parse_field_filters(&args)?;
    let tickets = scoped_tickets(&args, store, &all_edges)?;
    let tickets = apply_field_filters(tickets, &field_filters);
    let workflow = WorkflowModel::build(
        store,
        store.list(None, None, None)?,
        all_edges.to_vec(),
    )?;
    let report = collect_findings(store, &tickets, &all_edges, &workflow);
    let tickets_checked = tickets
        .iter()
        .filter(|ticket| {
            !matches!(
                ticket.state.as_deref(),
                Some("done") | Some("cancelled")
            )
        })
        .count();

    Ok(json!({
        "command": "health",
        "status": "ok",
        "tickets_checked": tickets_checked,
        "finding_count": report.findings.len(),
        "summary": report.summary,
        "findings": report.findings,
    }))
}

fn parse_field_filters(
    args: &HealthArgs
) -> Result<Vec<(String, String)>, CliRunError> {
    Ok(parse_fields(&args.where_clauses)?.into_iter().collect())
}

fn scoped_tickets(
    args: &HealthArgs,
    store: &TicketStore,
    all_edges: &[EdgeRecord],
) -> Result<Vec<IndexedTicket>, CliRunError> {
    if args.stdin {
        stdin_tickets(store)
    } else if args.all {
        Ok(store.list(None, None, None)?)
    } else {
        root_scope_tickets(args, store, all_edges)
    }
}

fn stdin_tickets(
    store: &TicketStore
) -> Result<Vec<IndexedTicket>, CliRunError> {
    let stdin = std::io::stdin();
    let mut ids = Vec::new();

    for line in stdin.lock().lines() {
        let line = line.map_err(ticket_api::error::StorageError::Io)?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        ids.push(super::super::resolve_uuid_prefix(trimmed, store)?);
    }

    Ok(load_live_tickets(store, &ids))
}

fn root_scope_tickets(
    args: &HealthArgs,
    store: &TicketStore,
    all_edges: &[EdgeRecord],
) -> Result<Vec<IndexedTicket>, CliRunError> {
    let root_str = args
        .root
        .as_ref()
        .expect("clap ensures root is present when --all/--stdin is not set");
    let root = super::super::resolve_uuid_prefix(root_str, store)?;
    let ids = collect_scope_ids(
        root,
        args.direction.as_str(),
        args.depth.min(8),
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
    let mut ids = Vec::new();
    let mut queue = VecDeque::from([(root, 0)]);

    while let Some((current_id, depth)) = queue.pop_front() {
        if !visited.insert(current_id) {
            continue;
        }
        ids.push(current_id);
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

    ids
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

fn apply_field_filters(
    tickets: Vec<IndexedTicket>,
    field_filters: &[(String, String)],
) -> Vec<IndexedTicket> {
    if field_filters.is_empty() {
        return tickets;
    }

    tickets
        .into_iter()
        .filter(|ticket| matches_filters(ticket, field_filters))
        .collect()
}

fn matches_filters(
    ticket: &IndexedTicket,
    field_filters: &[(String, String)],
) -> bool {
    field_filters.iter().all(|(key, expected)| {
        let actual = match key.as_str() {
            "state" => ticket.state.as_deref().map(String::from),
            "type" => Some(ticket.type_id.clone()),
            "title" => ticket.title.clone(),
            _ => None,
        };
        actual.as_deref() == Some(expected.as_str())
    })
}


