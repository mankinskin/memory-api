use std::collections::{
    HashMap,
    HashSet,
};

use serde_json::{
    Value,
    json,
};
use ticket_api::{
    BoardEntryStatus,
    BoardSnapshot,
    storage::{
        indexed::IndexedTicket,
        store::TicketStore,
    },
};
use uuid::Uuid;

use crate::cli::{
    CliRunError,
    StatusArgs,
};

const DONE_STATES: &[&str] = &["done", "cancelled"];
const ACTIVE_STATES: &[&str] = &["ready", "in-implementation", "in-review"];

#[derive(Default)]
struct StatusSections {
    active: Vec<Value>,
    ready: Vec<Value>,
    blocked: Vec<Value>,
    done_count: usize,
    total: usize,
}

pub(super) fn run(
    args: StatusArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let board_snap = store.board_show(None).ok();
    let tickets =
        filtered_tickets(store.list(None, None, None)?, args.filter.as_deref());
    let done_ids = done_ticket_ids(&tickets);
    let blockers = unresolved_blockers(store, &done_ids)?;
    let sections = status_sections(&tickets, &blockers, args.show_blocked);
    let parallel_groups = parallel_groups(&sections.ready);
    let blocked_count = if args.show_blocked {
        sections.blocked.len()
    } else {
        sections.total
            - sections.done_count
            - sections.active.len()
            - sections.ready.len()
    };

    Ok(json!({
        "command": "status",
        "status": "ok",
        "summary": {
            "total": sections.total,
            "done": sections.done_count,
            "active": sections.active.len(),
            "ready": sections.ready.len(),
            "blocked": blocked_count,
        },
        "active": sections.active,
        "ready": sections.ready,
        "blocked": sections.blocked,
        "parallel_groups": parallel_groups,
        "board": board_value(board_snap.as_ref()),
    }))
}

fn filtered_tickets(
    tickets: Vec<IndexedTicket>,
    filter: Option<&str>,
) -> Vec<IndexedTicket> {
    match filter {
        Some(prefix) => tickets
            .into_iter()
            .filter(|ticket| {
                ticket.title.as_deref().unwrap_or("").starts_with(prefix)
            })
            .collect(),
        None => tickets,
    }
}

fn done_ticket_ids(tickets: &[IndexedTicket]) -> HashSet<Uuid> {
    tickets
        .iter()
        .filter(|ticket| {
            ticket
                .state
                .as_deref()
                .map(|state| DONE_STATES.contains(&state))
                .unwrap_or(false)
        })
        .map(|ticket| ticket.id)
        .collect()
}

fn unresolved_blockers(
    store: &TicketStore,
    done_ids: &HashSet<Uuid>,
) -> Result<HashMap<Uuid, Vec<Uuid>>, CliRunError> {
    let mut blockers = HashMap::new();
    for edge in &store.list_all_edges()? {
        if edge.kind == "depends_on" && !done_ids.contains(&edge.to) {
            blockers
                .entry(edge.from)
                .or_insert_with(Vec::new)
                .push(edge.to);
        }
    }
    Ok(blockers)
}

fn status_sections(
    tickets: &[IndexedTicket],
    blockers: &HashMap<Uuid, Vec<Uuid>>,
    show_blocked: bool,
) -> StatusSections {
    let mut sections = StatusSections::default();

    for ticket in tickets {
        sections.total += 1;
        let state = ticket.state.as_deref().unwrap_or("new");

        if DONE_STATES.contains(&state) {
            sections.done_count += 1;
            continue;
        }

        let unresolved = blockers.get(&ticket.id).cloned().unwrap_or_default();
        if ACTIVE_STATES.contains(&state) {
            sections.active.push(ticket_entry(ticket, state));
        } else if unresolved.is_empty() {
            sections.ready.push(ticket_entry(ticket, state));
        } else if show_blocked {
            sections.blocked.push(json!({
                "id": ticket.id,
                "title": ticket.title,
                "state": state,
                "waiting_on": dependency_entries(&unresolved, tickets),
            }));
        }
    }

    sections
}

fn ticket_entry(
    ticket: &IndexedTicket,
    state: &str,
) -> Value {
    json!({
        "id": ticket.id,
        "title": ticket.title,
        "state": state,
        "component": ticket.type_id,
    })
}

fn dependency_entries(
    unresolved: &[Uuid],
    tickets: &[IndexedTicket],
) -> Vec<Value> {
    unresolved
        .iter()
        .map(|ticket_id| {
            let (title, state) = dependency_metadata(*ticket_id, tickets);
            json!({ "id": ticket_id, "title": title, "state": state })
        })
        .collect()
}

fn dependency_metadata(
    ticket_id: Uuid,
    tickets: &[IndexedTicket],
) -> (String, String) {
    let title = tickets
        .iter()
        .find(|ticket| ticket.id == ticket_id)
        .and_then(|ticket| ticket.title.clone())
        .unwrap_or_else(|| ticket_id.to_string());
    let state = tickets
        .iter()
        .find(|ticket| ticket.id == ticket_id)
        .and_then(|ticket| ticket.state.clone())
        .unwrap_or_else(|| "unknown".to_string());
    (title, state)
}

fn parallel_groups(ready: &[Value]) -> Vec<Value> {
    let mut by_component: HashMap<String, Vec<&Value>> = HashMap::new();
    for entry in ready {
        let component =
            entry["component"].as_str().unwrap_or("unknown").to_string();
        by_component
            .entry(component)
            .or_insert_with(Vec::new)
            .push(entry);
    }

    by_component
        .into_iter()
        .map(|(component, tickets)| {
            json!({
                "component": component,
                "count": tickets.len(),
                "tickets": tickets,
            })
        })
        .collect()
}

fn board_value(board_snap: Option<&BoardSnapshot>) -> Value {
    board_snap
        .map(|snap| {
            let stale_suffix = if snap.stale_count > 0 {
                format!(" [{} stale \u{26a0}]", snap.stale_count)
            } else {
                String::new()
            };
            let summary = format!(
                "Board: [{}/{} active]{}",
                snap.active_count, snap.config.max_wip, stale_suffix
            );

            json!({
                "summary": summary,
                "active_count": snap.active_count,
                "stale_count": snap.stale_count,
                "max_wip": snap.config.max_wip,
                "wip_limit_reached": snap.wip_limit_reached,
                "warnings": snap.warnings,
                "entries": board_entries(snap),
            })
        })
        .unwrap_or(Value::Null)
}

fn board_entries(snapshot: &BoardSnapshot) -> Vec<Value> {
    snapshot
        .entries
        .iter()
        .filter(|entry| {
            entry.status == BoardEntryStatus::Active
                || entry.status == BoardEntryStatus::Stale
        })
        .map(|entry| {
            json!({
                "ticket_id": entry.ticket_id,
                "agent_id": entry.agent_id,
                "status": board_status(entry.status.clone()),
                "intent": entry.intent,
                "last_heartbeat": entry.last_heartbeat.to_rfc3339(),
            })
        })
        .collect()
}

fn board_status(status: BoardEntryStatus) -> &'static str {
    match status {
        BoardEntryStatus::Active => "active",
        BoardEntryStatus::Stale => "stale",
        BoardEntryStatus::Conflict => "conflict",
        BoardEntryStatus::Completed => "completed",
    }
}
