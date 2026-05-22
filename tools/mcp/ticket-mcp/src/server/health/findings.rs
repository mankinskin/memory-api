use std::collections::BTreeMap;

use rmcp::ErrorData as McpError;
use serde_json::Value;
use ticket_api::storage::{
    indexed::IndexedTicket,
    store::TicketStore,
    ticket_fs::TicketFs,
};
use uuid::Uuid;

use super::HealthContext;

#[derive(Default)]
pub(super) struct HealthReport {
    pub(super) summary: BTreeMap<String, u64>,
    pub(super) findings: Vec<Value>,
}

pub(super) fn collect_findings(
    store: &TicketStore,
    context: &HealthContext,
) -> Result<HealthReport, McpError> {
    let mut report = HealthReport::default();

    for ticket in &context.tickets {
        if context.done_ids.contains(&ticket.id) {
            continue;
        }
        append_ticket_findings(store, context, &mut report, ticket)?;
    }

    Ok(report)
}

fn append_ticket_findings(
    store: &TicketStore,
    context: &HealthContext,
    report: &mut HealthReport,
    ticket: &IndexedTicket,
) -> Result<(), McpError> {
    append_description_findings(report, ticket);
    append_title_finding(report, ticket);
    append_dependency_state_finding(report, context, ticket);
    append_dangling_edge_findings(store, report, context, ticket)?;
    Ok(())
}

fn append_description_findings(
    report: &mut HealthReport,
    ticket: &IndexedTicket,
) {
    let short_id = short_id(ticket.id);
    let title = ticket.title.as_deref().unwrap_or("?");
    let description = TicketFs::read_description(&ticket.path);

    match description {
        None => record_finding(
            report,
            "missing_description",
            serde_json::json!({
                "ticket_id": ticket.id,
                "short_id": short_id,
                "title": title,
                "check": "missing_description",
                "severity": "error",
                "message": "No description.md file — ticket lacks detailed context.",
            }),
        ),
        Some(body) if body.trim().len() < 50 => record_finding(
            report,
            "short_description",
            serde_json::json!({
                "ticket_id": ticket.id,
                "short_id": short_id,
                "title": title,
                "check": "short_description",
                "severity": "info",
                "message": format!(
                    "description.md is very short ({} chars) — consider adding more detail.",
                    body.trim().len()
                ),
            }),
        ),
        Some(_) => {},
    }
}

fn append_title_finding(
    report: &mut HealthReport,
    ticket: &IndexedTicket,
) {
    if ticket.title.is_some() && ticket.title.as_deref() != Some("") {
        return;
    }

    record_finding(
        report,
        "missing_title",
        serde_json::json!({
            "ticket_id": ticket.id,
            "short_id": short_id(ticket.id),
            "title": "(none)",
            "check": "missing_title",
            "severity": "error",
            "message": "Ticket has no title.",
        }),
    );
}

fn append_dependency_state_finding(
    report: &mut HealthReport,
    context: &HealthContext,
    ticket: &IndexedTicket,
) {
    let state = ticket.state.as_deref().unwrap_or("");
    if state == "new" {
        return;
    }

    if let Some(unresolved) = context.workflow.unresolved_dependencies(&ticket.id) {
        record_finding(
            report,
            "unblocked_with_deps",
            serde_json::json!({
                "ticket_id": ticket.id,
                "short_id": short_id(ticket.id),
                "title": ticket.title.as_deref().unwrap_or("?"),
                "check": "unblocked_with_deps",
                "severity": "info",
                "message": format!(
                    "Ticket is '{state}' but has {} unresolved dependency/ies — may need state review.",
                    unresolved.len()
                ),
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
            report,
            "dependency_convergence",
            serde_json::json!({
                "ticket_id": ticket.id,
                "short_id": short_id(ticket.id),
                "title": ticket.title.as_deref().unwrap_or("?"),
                "check": "dependency_convergence",
                "severity": "warning",
                "message": format!(
                    "Ticket depends on {} in earlier state '{}' while this ticket is '{}'.",
                    short_id(inversion.prerequisite_id),
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
    report: &mut HealthReport,
    context: &HealthContext,
    ticket: &IndexedTicket,
) -> Result<(), McpError> {
    for edge in &context.all_edges {
        if edge.from != ticket.id || edge.kind != "depends_on" {
            continue;
        }

        let target_exists = store
            .get_indexed(&edge.to)
            .ok()
            .flatten()
            .map(|target| !target.deleted)
            .unwrap_or(false);
        if target_exists {
            continue;
        }

        record_finding(
            report,
            "dangling_edge",
            serde_json::json!({
                "ticket_id": ticket.id,
                "short_id": short_id(ticket.id),
                "title": ticket.title.as_deref().unwrap_or("?"),
                "check": "dangling_edge",
                "severity": "error",
                "message": format!(
                    "depends_on edge points to {} which is deleted or missing.",
                    short_id(edge.to)
                ),
            }),
        );
    }

    Ok(())
}

fn record_finding(
    report: &mut HealthReport,
    key: &str,
    finding: Value,
) {
    *report.summary.entry(key.to_string()).or_insert(0) += 1;
    report.findings.push(finding);
}

fn short_id(id: Uuid) -> String {
    id.to_string()[..8].to_string()
}
