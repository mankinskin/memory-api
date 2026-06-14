use std::collections::BTreeMap;

use serde_json::{
    Value,
    json,
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

use super::HealthContext;

pub(super) fn collect_findings(
    store: &TicketStore,
    tickets: &[IndexedTicket],
    all_edges: &[EdgeRecord],
    context: &HealthContext,
) -> (BTreeMap<String, u64>, Vec<Value>) {
    let mut summary = BTreeMap::new();
    let mut findings = Vec::new();

    for ticket in tickets {
        if context.done_ids.contains(&ticket.id) {
            continue;
        }
        append_ticket_findings(
            store,
            ticket,
            all_edges,
            context,
            &mut summary,
            &mut findings,
        );
    }

    (summary, findings)
}

fn append_ticket_findings(
    store: &TicketStore,
    ticket: &IndexedTicket,
    all_edges: &[EdgeRecord],
    context: &HealthContext,
    summary: &mut BTreeMap<String, u64>,
    findings: &mut Vec<Value>,
) {
    let short_id = short_ticket_id(ticket.id);
    let title = ticket.title.as_deref().unwrap_or("?");

    append_description_findings(ticket, &short_id, title, summary, findings);
    append_title_finding(ticket, &short_id, summary, findings);
    append_dependency_state_finding(
        ticket, &short_id, title, context, summary, findings,
    );
    append_dangling_edge_findings(
        store, ticket, all_edges, &short_id, title, summary, findings,
    );
}

fn append_description_findings(
    ticket: &IndexedTicket,
    short_id: &str,
    title: &str,
    summary: &mut BTreeMap<String, u64>,
    findings: &mut Vec<Value>,
) {
    match TicketFs::read_description(&ticket.path) {
        None => record_finding(
            summary,
            findings,
            "missing_description",
            json!({
                "ticket_id": ticket.id,
                "short_id": short_id,
                "title": title,
                "check": "missing_description",
                "severity": "warning",
                "message": "No description.md file - ticket lacks detailed context.",
            }),
        ),
        Some(body) => {
            let trimmed_len = body.trim().len();
            if trimmed_len < 50 {
                record_finding(
                    summary,
                    findings,
                    "short_description",
                    json!({
                        "ticket_id": ticket.id,
                        "short_id": short_id,
                        "title": title,
                        "check": "short_description",
                        "severity": "info",
                        "message": format!("description.md is very short ({trimmed_len} chars) - consider adding more detail."),
                    }),
                );
            }
        },
    }
}

fn append_title_finding(
    ticket: &IndexedTicket,
    short_id: &str,
    summary: &mut BTreeMap<String, u64>,
    findings: &mut Vec<Value>,
) {
    if ticket.title.is_none() || ticket.title.as_deref() == Some("") {
        record_finding(
            summary,
            findings,
            "missing_title",
            json!({
                "ticket_id": ticket.id,
                "short_id": short_id,
                "title": "(none)",
                "check": "missing_title",
                "severity": "error",
                "message": "Ticket has no title.",
            }),
        );
    }
}

fn append_dependency_state_finding(
    ticket: &IndexedTicket,
    short_id: &str,
    title: &str,
    context: &HealthContext,
    summary: &mut BTreeMap<String, u64>,
    findings: &mut Vec<Value>,
) {
    let state = ticket.state.as_deref().unwrap_or("");
    if state == "new" {
        return;
    }

    if let Some(unresolved) = context.workflow.unresolved_dependencies(&ticket.id) {
        record_finding(
            summary,
            findings,
            "unblocked_with_deps",
            json!({
                "ticket_id": ticket.id,
                "short_id": short_id,
                "title": title,
                "check": "unblocked_with_deps",
                "severity": "info",
                "message": format!("Ticket is '{state}' but has {} unresolved dependency/ies - may need state review.", unresolved.len()),
            }),
        );
    }

    for inversion in context
        .workflow
        .dependency_state_inversions(&ticket.id)
        .into_iter()
        .flatten()
    {
        record_finding(
            summary,
            findings,
            "dependency_convergence",
            json!({
                "ticket_id": ticket.id,
                "short_id": short_id,
                "title": title,
                "check": "dependency_convergence",
                "severity": "warning",
                "message": format!(
                    "Ticket depends on {} in earlier state '{}' while this ticket is '{}'.",
                    short_ticket_id(inversion.prerequisite_id),
                    inversion.prerequisite_state.as_deref().unwrap_or("?"),
                    inversion.dependent_state.as_deref().unwrap_or(state),
                ),
                "prerequisite_id": inversion.prerequisite_id,
                "prerequisite_title": inversion.prerequisite_title,
                "prerequisite_state": inversion.prerequisite_state,
                "dependent_id": inversion.dependent_id,
                "dependent_state": inversion.dependent_state,
                "dependency_state_gap": inversion.dependency_state_gap,
                "affected_reverse_dependent_reach": inversion.affected_reverse_dependent_reach,
                "transitive_reverse_dependents": inversion.transitive_reverse_dependents,
            }),
        );
    }
}

fn append_dangling_edge_findings(
    store: &TicketStore,
    ticket: &IndexedTicket,
    all_edges: &[EdgeRecord],
    short_id: &str,
    title: &str,
    summary: &mut BTreeMap<String, u64>,
    findings: &mut Vec<Value>,
) {
    for target in dangling_dependency_targets(store, ticket.id, all_edges) {
        record_finding(
            summary,
            findings,
            "dangling_edge",
            json!({
                "ticket_id": ticket.id,
                "short_id": short_id,
                "title": title,
                "check": "dangling_edge",
                "severity": "error",
                "message": format!("depends_on edge points to {} which is missing.", short_ticket_id(target)),
            }),
        );
    }
}

fn dangling_dependency_targets(
    store: &TicketStore,
    ticket_id: Uuid,
    all_edges: &[EdgeRecord],
) -> Vec<Uuid> {
    all_edges
        .iter()
        .filter_map(|edge| depends_on_target(edge, ticket_id))
        .filter(|target| !ticket_exists(store, *target))
        .collect()
}

fn depends_on_target(
    edge: &EdgeRecord,
    ticket_id: Uuid,
) -> Option<Uuid> {
    if edge.from == ticket_id && edge.kind == "depends_on" {
        Some(edge.to)
    } else {
        None
    }
}

fn ticket_exists(
    store: &TicketStore,
    ticket_id: Uuid,
) -> bool {
    store
        .get_indexed(&ticket_id)
        .ok()
        .flatten()
        .is_some()
}

fn short_ticket_id(ticket_id: Uuid) -> String {
    ticket_id.to_string().chars().take(8).collect()
}

fn record_finding(
    summary: &mut BTreeMap<String, u64>,
    findings: &mut Vec<Value>,
    key: &str,
    finding: Value,
) {
    *summary.entry(key.to_string()).or_insert(0) += 1;
    findings.push(finding);
}
