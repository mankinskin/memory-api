//! Canonical health-finding generation for ticket stores.
//!
//! All three transport surfaces (CLI, HTTP, MCP) delegate to this module so
//! that finding keys, severities, and message text are identical regardless of
//! how they are serialized to their respective envelopes.

use std::{
    collections::BTreeMap,
    path::PathBuf,
};

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
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    pub r#type: String,
    pub check: String,
    pub severity: String,
    pub message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub instructions: Vec<String>,
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
    fn record(
        &mut self,
        check: &str,
        finding: HealthFinding,
    ) {
        *self.summary.entry(check.to_string()).or_insert(0) += 1;
        self.findings.push(finding);
    }
}

// ─── Public entry point ──────────────────────────────────────────────────────

/// Produce a normalized `HealthReport` for `tickets` using edge and workflow
/// context.
///
/// Graph participation checks run for all tickets to preserve topology parity,
/// while non-topology checks skip done/cancelled tickets.
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
            matches!(t.state.as_deref(), Some("done") | Some("cancelled"))
        })
        .map(|t| t.id)
        .collect::<std::collections::HashSet<_>>();

    let mut report = HealthReport::default();
    for ticket in tickets {
        append_graph_participation_findings(ticket, all_edges, &mut report);
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
    append_effort_estimation_findings(ticket, workflow, report);
    append_description_findings(ticket, report);
    append_title_finding(ticket, report);
    append_dependency_state_findings(ticket, workflow, report);
    append_dangling_edge_findings(store, ticket, all_edges, report);
}

fn append_effort_estimation_findings(
    ticket: &IndexedTicket,
    workflow: &WorkflowModel,
    report: &mut HealthReport,
) {
    if workflow.effort(&ticket.id).is_some() {
        return;
    }

    report.record(
        "missing_effort_estimation",
        base_finding(
            ticket,
            "missing_effort_estimation",
            "warning",
            "Ticket is missing an effort estimation (for example '1200' or '2.5k tokens').".to_string(),
            vec![
                "Set the ticket 'effort' field to a token-budget estimate so planning and ranking remain accurate.".to_string(),
            ],
        ),
    );
}

fn append_graph_participation_findings(
    ticket: &IndexedTicket,
    all_edges: &[EdgeRecord],
    report: &mut HealthReport,
) {
    let dependency_count = all_edges
        .iter()
        .filter(|edge| edge.kind == "depends_on" && edge.from == ticket.id)
        .count();
    let dependee_count = all_edges
        .iter()
        .filter(|edge| edge.kind == "depends_on" && edge.to == ticket.id)
        .count();

    if dependency_count > 0 || dependee_count > 0 {
        return;
    }

    let title = ticket.title.as_deref().unwrap_or("?").to_string();
    let path = ticket.path.to_string_lossy().to_string();
    report.record(
        "graph_participation",
        base_finding(
            ticket,
            "graph_participation",
            "warning",
            format!("{title} is not linked into the depends_on graph."),
            vec![
                format!(
                    "Link {path} to real prerequisites or dependees using depends_on edges."
                ),
                "If this is standalone work, place it under a tracker ticket to keep the graph connected.".to_string(),
            ],
        ),
    );
}

fn append_description_findings(
    ticket: &IndexedTicket,
    report: &mut HealthReport,
) {
    match TicketFs::read_description(&ticket.path) {
        None => report.record(
            "missing_description",
            base_finding(
                ticket,
                "missing_description",
                "warning",
                "No description.md file — ticket lacks detailed context.".into(),
                vec!["Add a description.md with goal, scope, acceptance checks, and current status.".to_string()],
            ),
        ),
        Some(body) => {
            let trimmed_len = body.trim().len();
            if trimmed_len < 50 {
                report.record(
                    "short_description",
                    base_finding(
                        ticket,
                        "short_description",
                        "info",
                        format!(
                            "description.md is very short ({trimmed_len} chars) — consider adding more detail."
                        ),
                        vec![
                            "Expand description.md with acceptance criteria, dependencies, and validation evidence.".to_string(),
                        ],
                    ),
                );
            }
        },
    }
}

fn append_title_finding(
    ticket: &IndexedTicket,
    report: &mut HealthReport,
) {
    if ticket.title.is_none() || ticket.title.as_deref() == Some("") {
        report.record(
            "missing_title",
            base_finding(
                ticket,
                "missing_title",
                "error",
                "Ticket has no title.".into(),
                vec!["Set a concise ticket title describing the change intent and scope.".to_string()],
            ),
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

    for inversion in workflow
        .dependency_state_inversions(&ticket.id)
        .into_iter()
        .flatten()
    {
        report.record(
            "dependency_convergence",
            HealthFinding {
                message: format!(
                    "Ticket depends on {} in earlier state '{}' while this ticket is '{}'.",
                    short_id(inversion.prerequisite_id),
                    inversion.prerequisite_state.as_deref().unwrap_or("?"),
                    inversion.dependent_state.as_deref().unwrap_or(state),
                ),
                instructions: vec![
                    "Advance the prerequisite ticket before continuing this dependent ticket when dependency order is still valid.".to_string(),
                    "If out-of-order progress is intentional, document the exception and update states to make it explicit.".to_string(),
                ],
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
                ..base_finding(
                    ticket,
                    "dependency_convergence",
                    "warning",
                    String::new(),
                    Vec::new(),
                )
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
        let target_exists =
            store.get_indexed(&edge.to).ok().flatten().is_some();
        if target_exists {
            continue;
        }
        report.record(
            "dangling_edge",
            base_finding(
                ticket,
                "dangling_edge",
                "error",
                format!(
                    "depends_on edge points to {} which is missing.",
                    short_id(edge.to)
                ),
                vec![
                    "Remove or retarget the stale depends_on edge to an existing prerequisite ticket.".to_string(),
                ],
            ),
        );
    }
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn short_id(id: Uuid) -> String {
    id.to_string()[..8].to_string()
}

fn base_finding(
    ticket: &IndexedTicket,
    check: &str,
    severity: &str,
    message: String,
    instructions: Vec<String>,
) -> HealthFinding {
    HealthFinding {
        ticket_id: ticket.id,
        short_id: short_id(ticket.id),
        title: ticket.title.as_deref().unwrap_or("?").to_string(),
        path: ticket.path.clone(),
        state: ticket.state.clone(),
        r#type: ticket.type_id.clone(),
        check: check.to_string(),
        severity: severity.to_string(),
        message,
        instructions,
        ..Default::default()
    }
}

impl Default for HealthFinding {
    fn default() -> Self {
        Self {
            ticket_id: Uuid::nil(),
            short_id: String::new(),
            title: String::new(),
            path: PathBuf::new(),
            state: None,
            r#type: String::new(),
            check: String::new(),
            severity: String::new(),
            message: String::new(),
            instructions: Vec::new(),
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

    fn extra_with_effort(value: &str) -> BTreeMap<String, serde_json::Value> {
        let mut extra = BTreeMap::new();
        extra.insert(
            "effort".to_string(),
            serde_json::Value::String(value.to_string()),
        );
        extra
    }

    fn open_store() -> (tempfile::TempDir, TicketStore) {
        let dir = tempdir().unwrap();
        let store = TicketStore::init(dir.path()).unwrap();
        (dir, store)
    }

    #[test]
    fn no_findings_for_ticket_with_good_description() {
        let (_dir, store) = open_store();
        let linked_parent = store
            .create(
                None,
                "tracker-improvement",
                Some("My well-described ticket"),
                Some("ready"),
                extra_with_effort("1200"),
                None,
                Some("This description is definitely long enough to pass the 50-character threshold."),
            )
            .unwrap();
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("My well-described dependent ticket"),
                Some("ready"),
                extra_with_effort("800"),
                None,
                Some("This description is definitely long enough to pass the 50-character threshold."),
            )
            .unwrap();
        store
            .add_edge(crate::model::edge::EdgeRecord {
                from: linked_parent,
                to: id,
                kind: "depends_on".to_string(),
                created_at: chrono::Utc::now(),
            })
            .unwrap();

        let tickets = store.list(None, None, None).unwrap();
        let edges = store.list_all_edges().unwrap();
        let workflow =
            WorkflowModel::build(&store, tickets.clone(), edges.clone())
                .unwrap();
        let report =
            super::collect_findings(&store, &tickets, &edges, &workflow);

        let ticket_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.ticket_id == id)
            .collect();
        assert!(
            ticket_findings.is_empty(),
            "expected no findings, got {ticket_findings:?}"
        );
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
                extra_with_effort("900"),
                None,
                None,
            )
            .unwrap();

        let tickets = store.list(None, None, None).unwrap();
        let edges = store.list_all_edges().unwrap();
        let workflow =
            WorkflowModel::build(&store, tickets.clone(), edges.clone())
                .unwrap();
        let report =
            super::collect_findings(&store, &tickets, &edges, &workflow);

        let finding = report
            .findings
            .iter()
            .find(|f| f.ticket_id == id && f.check == "missing_description")
            .expect("expected missing_description finding");
        assert_eq!(
            finding.severity, "warning",
            "severity must be 'warning', not 'error'"
        );
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
                extra_with_effort("600"),
                None,
                Some("Short."),
            )
            .unwrap();

        let tickets = store.list(None, None, None).unwrap();
        let edges = store.list_all_edges().unwrap();
        let workflow =
            WorkflowModel::build(&store, tickets.clone(), edges.clone())
                .unwrap();
        let report =
            super::collect_findings(&store, &tickets, &edges, &workflow);

        let finding = report
            .findings
            .iter()
            .find(|f| f.ticket_id == id && f.check == "short_description")
            .expect("expected short_description finding");
        assert_eq!(finding.severity, "info");
    }

    #[test]
    fn done_ticket_skips_non_topology_checks() {
        let (_dir, store) = open_store();
        let done = store
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
        let linked = store
            .create(
                None,
                "tracker-improvement",
                Some("Linked ticket"),
                Some("done"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();
        store
            .add_edge(crate::model::edge::EdgeRecord {
                from: done,
                to: linked,
                kind: "depends_on".to_string(),
                created_at: chrono::Utc::now(),
            })
            .unwrap();

        let tickets = store.list(None, None, None).unwrap();
        let edges = store.list_all_edges().unwrap();
        let workflow =
            WorkflowModel::build(&store, tickets.clone(), edges.clone())
                .unwrap();
        let report =
            super::collect_findings(&store, &tickets, &edges, &workflow);

        let ticket_findings: Vec<_> = report
            .findings
            .iter()
            .filter(|f| f.ticket_id == done)
            .collect();
        assert!(
            ticket_findings.is_empty(),
            "done ticket should skip non-topology checks, got {ticket_findings:?}"
        );
    }

    #[test]
    fn orphan_ticket_produces_graph_participation_finding() {
        let (_dir, store) = open_store();
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("Orphan ticket"),
                Some("in-implementation"),
                extra_with_effort("1500"),
                None,
                Some("This description is definitely long enough to pass the 50-character threshold."),
            )
            .unwrap();

        let tickets = store.list(None, None, None).unwrap();
        let edges = store.list_all_edges().unwrap();
        let workflow =
            WorkflowModel::build(&store, tickets.clone(), edges.clone())
                .unwrap();
        let report =
            super::collect_findings(&store, &tickets, &edges, &workflow);

        let finding = report
            .findings
            .iter()
            .find(|f| f.ticket_id == id && f.check == "graph_participation")
            .expect("expected graph_participation finding");
        assert_eq!(finding.severity, "warning");
        assert!(
            finding.message.contains("depends_on graph"),
            "message must mention depends_on graph"
        );
    }

    #[test]
    fn missing_effort_estimation_produces_warning() {
        let (_dir, store) = open_store();
        let parent = store
            .create(
                None,
                "tracker-improvement",
                Some("Parent"),
                Some("ready"),
                extra_with_effort("500"),
                None,
                Some("This description is definitely long enough to pass the 50-character threshold."),
            )
            .unwrap();
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("Missing effort"),
                Some("ready"),
                BTreeMap::new(),
                None,
                Some("This description is definitely long enough to pass the 50-character threshold."),
            )
            .unwrap();
        store
            .add_edge(crate::model::edge::EdgeRecord {
                from: parent,
                to: id,
                kind: "depends_on".to_string(),
                created_at: chrono::Utc::now(),
            })
            .unwrap();

        let tickets = store.list(None, None, None).unwrap();
        let edges = store.list_all_edges().unwrap();
        let workflow =
            WorkflowModel::build(&store, tickets.clone(), edges.clone())
                .unwrap();
        let report =
            super::collect_findings(&store, &tickets, &edges, &workflow);

        let finding = report
            .findings
            .iter()
            .find(|f| {
                f.ticket_id == id && f.check == "missing_effort_estimation"
            })
            .expect("expected missing_effort_estimation finding");
        assert_eq!(finding.severity, "warning");
        assert!(
            finding.message.contains("effort estimation"),
            "message must mention effort estimation"
        );
    }
}
