//! Canonical health-finding generation for ticket stores.
//!
//! All three transport surfaces (CLI, HTTP, MCP) delegate to this module so
//! that finding keys, severities, and message text are identical regardless of
//! how they are serialized to their respective envelopes.

use std::collections::BTreeMap;

use serde::Serialize;
use uuid::Uuid;

use crate::{
    model::edge::EdgeRecord,
    storage::{
        indexed::IndexedTicket,
        store::TicketStore,
        ticket_fs::TicketFs,
    },
    workflow::WorkflowModel,
};

// ─── Types ───────────────────────────────────────────────────────────────────

/// One normalized health finding for a ticket.
#[derive(Debug, Clone, Serialize)]
pub struct HealthFinding {
    pub ticket_id: Uuid,
    pub short_id: String,
    pub title: String,
    pub check: String,
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prerequisite_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prerequisite_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prerequisite_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependent_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependent_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dependency_state_gap: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_reverse_dependent_reach: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transitive_reverse_dependents: Option<usize>,
}

/// Aggregated health findings for a set of tickets.
#[derive(Debug, Default)]
pub struct HealthReport {
    /// Count of findings grouped by check key.
    pub summary: BTreeMap<String, u64>,
    /// Individual findings in ticket-visit order.
    pub findings: Vec<HealthFinding>,
}

impl HealthReport {
    fn record(&mut self, check: &str, finding: HealthFinding) {
        *self.summary.entry(check.to_string()).or_insert(0) += 1;
        self.findings.push(finding);
    }
}

// ─── Public entry point ──────────────────────────────────────────────────────

/// Produce a normalized `HealthReport` for `tickets` using edge and workflow
/// context.  Done or cancelled tickets are skipped automatically.
///
/// This is the single canonical implementation consumed by CLI, HTTP, and MCP.
pub fn collect_findings(
    store: &TicketStore,
    tickets: &[IndexedTicket],
    all_edges: &[EdgeRecord],
    workflow: &WorkflowModel,
) -> HealthReport {
    let done_ids = tickets
        .iter()
        .filter(|t| {
            matches!(
                t.state.as_deref(),
                Some("done") | Some("cancelled")
            )
        })
        .map(|t| t.id)
        .collect::<std::collections::HashSet<_>>();

    let mut report = HealthReport::default();
    for ticket in tickets {
        if done_ids.contains(&ticket.id) {
            continue;
        }
        append_ticket_findings(store, ticket, all_edges, workflow, &mut report);
    }
    report
}

// ─── Per-ticket finding generators ───────────────────────────────────────────

fn append_ticket_findings(
    store: &TicketStore,
    ticket: &IndexedTicket,
    all_edges: &[EdgeRecord],
    workflow: &WorkflowModel,
    report: &mut HealthReport,
) {
    append_description_findings(ticket, report);
    append_title_finding(ticket, report);
    append_dependency_state_findings(ticket, workflow, report);
    append_dangling_edge_findings(store, ticket, all_edges, report);
}

fn append_description_findings(ticket: &IndexedTicket, report: &mut HealthReport) {
    let short_id = short_id(ticket.id);
    let title = ticket.title.as_deref().unwrap_or("?").to_string();
    match TicketFs::read_description(&ticket.path) {
        None => report.record(
            "missing_description",
            HealthFinding {
                ticket_id: ticket.id,
                short_id,
                title,
                check: "missing_description".into(),
                severity: "warning".into(),
                message: "No description.md file — ticket lacks detailed context.".into(),
                ..Default::default()
            },
        ),
        Some(body) => {
            let trimmed_len = body.trim().len();
            if trimmed_len < 50 {
                report.record(
                    "short_description",
                    HealthFinding {
                        ticket_id: ticket.id,
                        short_id,
                        title,
                        check: "short_description".into(),
                        severity: "info".into(),
                        message: format!(
                            "description.md is very short ({trimmed_len} chars) — consider adding more detail."
                        ),
                        ..Default::default()
                    },
                );
            }
        },
    }
}

fn append_title_finding(ticket: &IndexedTicket, report: &mut HealthReport) {
    if ticket.title.is_none() || ticket.title.as_deref() == Some("") {
        report.record(
            "missing_title",
            HealthFinding {
                ticket_id: ticket.id,
                short_id: short_id(ticket.id),
                title: "(none)".into(),
                check: "missing_title".into(),
                severity: "error".into(),
                message: "Ticket has no title.".into(),
                ..Default::default()
            },
        );
    }
}

fn append_dependency_state_findings(
    ticket: &IndexedTicket,
    workflow: &WorkflowModel,
    report: &mut HealthReport,
) {
    let state = ticket.state.as_deref().unwrap_or("");
    if state == "new" {
        return;
    }

    if let Some(unresolved) = workflow.unresolved_dependencies(&ticket.id) {
        report.record(
            "unblocked_with_deps",
            HealthFinding {
                ticket_id: ticket.id,
                short_id: short_id(ticket.id),
                title: ticket.title.as_deref().unwrap_or("?").to_string(),
                check: "unblocked_with_deps".into(),
                severity: "info".into(),
                message: format!(
                    "Ticket is '{state}' but has {} unresolved dependency/ies — may need state review.",
                    unresolved.len()
                ),
                ..Default::default()
            },
        );
    }

    for inversion in workflow
        .dependency_state_inversions(&ticket.id)
        .into_iter()
        .flatten()
    {
        report.record(
            "dependency_convergence",
            HealthFinding {
                ticket_id: ticket.id,
                short_id: short_id(ticket.id),
                title: ticket.title.as_deref().unwrap_or("?").to_string(),
                check: "dependency_convergence".into(),
                severity: "warning".into(),
                message: format!(
                    "Ticket depends on {} in earlier state '{}' while this ticket is '{}'.",
                    short_id(inversion.prerequisite_id),
                    inversion.prerequisite_state.as_deref().unwrap_or("?"),
                    inversion.dependent_state.as_deref().unwrap_or(state),
                ),
                prerequisite_id: Some(inversion.prerequisite_id),
                prerequisite_title: inversion.prerequisite_title.clone(),
                prerequisite_state: inversion.prerequisite_state.clone(),
                dependent_id: Some(inversion.dependent_id),
                dependent_state: inversion.dependent_state.clone(),
                dependency_state_gap: Some(inversion.dependency_state_gap),
                affected_reverse_dependent_reach: Some(
                    inversion.affected_reverse_dependent_reach,
                ),
                transitive_reverse_dependents: Some(inversion.transitive_reverse_dependents),
            },
        );
    }
}

fn append_dangling_edge_findings(
    store: &TicketStore,
    ticket: &IndexedTicket,
    all_edges: &[EdgeRecord],
    report: &mut HealthReport,
) {
    for edge in all_edges {
        if edge.from != ticket.id || edge.kind != "depends_on" {
            continue;
        }
        let target_exists = store
            .get_indexed(&edge.to)
            .ok()
            .flatten()
            .map(|t| !t.deleted)
            .unwrap_or(false);
        if target_exists {
            continue;
        }
        report.record(
            "dangling_edge",
            HealthFinding {
                ticket_id: ticket.id,
                short_id: short_id(ticket.id),
                title: ticket.title.as_deref().unwrap_or("?").to_string(),
                check: "dangling_edge".into(),
                severity: "error".into(),
                message: format!(
                    "depends_on edge points to {} which is deleted or missing.",
                    short_id(edge.to)
                ),
                ..Default::default()
            },
        );
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn short_id(id: Uuid) -> String {
    id.to_string()[..8].to_string()
}

impl Default for HealthFinding {
    fn default() -> Self {
        Self {
            ticket_id: Uuid::nil(),
            short_id: String::new(),
            title: String::new(),
            check: String::new(),
            severity: String::new(),
            message: String::new(),
            prerequisite_id: None,
            prerequisite_title: None,
            prerequisite_state: None,
            dependent_id: None,
            dependent_state: None,
            dependency_state_gap: None,
            affected_reverse_dependent_reach: None,
            transitive_reverse_dependents: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tempfile::tempdir;

    use crate::{
        storage::store::TicketStore,
        workflow::WorkflowModel,
    };

    fn open_store() -> (tempfile::TempDir, TicketStore) {
        let dir = tempdir().unwrap();
        let store = TicketStore::init(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn no_findings_for_ticket_with_good_description() {
        let (_dir, store) = open_store();
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("My well-described ticket"),
                Some("ready"),
                BTreeMap::new(),
                None,
                Some("This description is definitely long enough to pass the 50-character threshold."),
            )
            .unwrap();

        let tickets = store.list(None, None, None).unwrap();
        let edges = store.list_all_edges().unwrap();
        let workflow = WorkflowModel::build(&store, tickets.clone(), edges.clone()).unwrap();
        let report = super::collect_findings(&store, &tickets, &edges, &workflow);

        let ticket_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.ticket_id == id)
            .collect();
        assert!(ticket_findings.is_empty(), "expected no findings, got {ticket_findings:?}");
    }

    #[test]
    fn missing_description_produces_warning_not_error() {
        let (_dir, store) = open_store();
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("Ticket with no description"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();

        let tickets = store.list(None, None, None).unwrap();
        let edges = store.list_all_edges().unwrap();
        let workflow = WorkflowModel::build(&store, tickets.clone(), edges.clone()).unwrap();
        let report = super::collect_findings(&store, &tickets, &edges, &workflow);

        let finding = report
            .findings
            .iter()
            .find(|f| f.ticket_id == id && f.check == "missing_description")
            .expect("expected missing_description finding");
        assert_eq!(finding.severity, "warning", "severity must be 'warning', not 'error'");
        assert!(
            finding.message.contains("description.md"),
            "message must mention description.md"
        );
        assert_eq!(*report.summary.get("missing_description").unwrap_or(&0), 1);
    }

    #[test]
    fn short_description_produces_info_finding() {
        let (_dir, store) = open_store();
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("Ticket with terse description"),
                Some("ready"),
                BTreeMap::new(),
                None,
                Some("Short."),
            )
            .unwrap();

        let tickets = store.list(None, None, None).unwrap();
        let edges = store.list_all_edges().unwrap();
        let workflow = WorkflowModel::build(&store, tickets.clone(), edges.clone()).unwrap();
        let report = super::collect_findings(&store, &tickets, &edges, &workflow);

        let finding = report
            .findings
            .iter()
            .find(|f| f.ticket_id == id && f.check == "short_description")
            .expect("expected short_description finding");
        assert_eq!(finding.severity, "info");
    }

    #[test]
    fn done_ticket_is_skipped() {
        let (_dir, store) = open_store();
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("Finished ticket"),
                Some("done"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();

        let tickets = store.list(None, None, None).unwrap();
        let edges = store.list_all_edges().unwrap();
        let workflow = WorkflowModel::build(&store, tickets.clone(), edges.clone()).unwrap();
        let report = super::collect_findings(&store, &tickets, &edges, &workflow);

        let ticket_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.ticket_id == id)
            .collect();
        assert!(
            ticket_findings.is_empty(),
            "done ticket must produce no findings, got {ticket_findings:?}"
        );
    }
}
