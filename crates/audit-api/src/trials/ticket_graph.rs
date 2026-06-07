use std::path::Path;

use serde_json::json;
use ticket_api::{
    error::StorageError,
    health,
    storage::TicketStore,
    workflow::WorkflowModel,
};

use crate::{
    config::format_output_path,
    models::{
        AuditFinding,
        CountMetric,
        Severity,
        TrialStatus,
    },
};

pub struct TicketGraphResult {
    pub metric: CountMetric,
    pub findings: Vec<AuditFinding>,
}

pub fn evaluate(repo_root: &Path) -> TicketGraphResult {
    let store = match TicketStore::open(repo_root) {
        Ok(store) => store,
        Err(StorageError::WorkspaceNotFound { path }) => {
            return TicketGraphResult {
                metric: CountMetric::unavailable(format!(
                    "ticket store not initialized at {}; skipping ticket dependency topology audit",
                    format_output_path(&path)
                )),
                findings: Vec::new(),
            };
        },
        Err(err) => {
            return TicketGraphResult {
                metric: CountMetric {
                    status: TrialStatus::Failed,
                    count: None,
                    details: Some(format!(
                        "failed to inspect ticket dependency topology: {err}"
                    )),
                },
                findings: Vec::new(),
            };
        },
    };

    let tickets = match store.list(None, None, None) {
        Ok(tickets) => tickets,
        Err(err) => {
            return failed_result(err);
        },
    };
    let edges = match store.list_all_edges() {
        Ok(edges) => edges,
        Err(err) => {
            return failed_result(err);
        },
    };
    let workflow = match WorkflowModel::build(&store, tickets.clone(), edges.clone()) {
        Ok(workflow) => workflow,
        Err(err) => {
            return failed_result(err);
        },
    };

    let canonical_health = health::collect_findings(&store, &tickets, &edges, &workflow);
    let orphan_findings: Vec<AuditFinding> = canonical_health
        .findings
        .into_iter()
        .filter_map(|finding| {
            if finding.check != "graph_participation" {
                return None;
            }

            let display_path = relative_ticket_path(repo_root, &finding.path);
            let title = if finding.title.is_empty() {
                finding.ticket_id.to_string()
            } else {
                finding.title.clone()
            };
            Some(AuditFinding {
                id: format!("ticket_graph:{}", finding.ticket_id),
                category: "ticket_graph".to_string(),
                severity: orphan_severity(finding.state.as_deref()),
                summary: format!(
                    "{} is not linked into the depends_on graph.",
                    title
                ),
                path: Some(display_path.clone()),
                line: None,
                metric_name: "orphan_ticket_count".to_string(),
                metric_value: json!(1),
                threshold: Some(json!(0)),
                instructions: if finding.instructions.is_empty() {
                    vec![
                        format!(
                            "Link {} to its real prerequisites with depends_on edges, or attach it under an existing parent ticket.",
                            display_path
                        ),
                        "If this is otherwise standalone work, create a project-tracker parent ticket that depends_on this ticket so the task is still connected to the broader plan.".to_string(),
                    ]
                } else {
                    finding.instructions.clone()
                },
                evidence: json!({
                    "ticket_id": finding.ticket_id,
                    "path": display_path,
                    "state": finding.state,
                    "type": finding.r#type,
                    "check": finding.check,
                }),
            })
        })
        .collect();
    let mut convergence_findings = Vec::new();
    let mut sorted_ticket_ids = workflow
        .actionable_candidate_ids(None)
        .into_iter()
        .chain(workflow.eligible_candidate_ids(None))
        .collect::<Vec<_>>();
    sorted_ticket_ids.sort();
    sorted_ticket_ids.dedup();

    for ticket_id in sorted_ticket_ids {
        let Some(ticket) = workflow.ticket(&ticket_id) else {
            continue;
        };
        let Some(issues) = workflow.dependency_state_inversions(&ticket_id) else {
            continue;
        };
        let dependent_path = relative_ticket_path(repo_root, &ticket.path);
        let dependent_title = ticket
            .title
            .clone()
            .unwrap_or_else(|| ticket.id.to_string());

        for issue in issues {
            let prerequisite_path = workflow
                .ticket(&issue.prerequisite_id)
                .map(|prerequisite| {
                    relative_ticket_path(repo_root, &prerequisite.path)
                });
            convergence_findings.push(AuditFinding {
                id: format!(
                    "ticket_graph:convergence:{}:{}",
                    issue.dependent_id, issue.prerequisite_id
                ),
                category: "ticket_graph".to_string(),
                severity: convergence_severity(issue.dependent_state.as_deref()),
                summary: format!(
                    "{} depends on {} while the prerequisite is in an earlier workflow state.",
                    dependent_title,
                    issue
                        .prerequisite_title
                        .clone()
                        .unwrap_or_else(|| issue.prerequisite_id.to_string())
                ),
                path: Some(dependent_path.clone()),
                line: None,
                metric_name: "dependency_convergence_count".to_string(),
                metric_value: json!(1),
                threshold: Some(json!(0)),
                instructions: vec![
                    format!(
                        "Advance {} before continuing work on {} when the dependency is still real.",
                        prerequisite_path
                            .clone()
                            .unwrap_or_else(|| issue.prerequisite_id.to_string()),
                        dependent_path
                    ),
                    "If the dependent moved ahead intentionally, document the exception or correct the ticket states so the dependency order is explicit.".to_string(),
                ],
                evidence: json!({
                    "dependent_id": issue.dependent_id,
                    "dependent_path": dependent_path,
                    "dependent_title": issue.dependent_title,
                    "dependent_state": issue.dependent_state,
                    "prerequisite_id": issue.prerequisite_id,
                    "prerequisite_path": prerequisite_path,
                    "prerequisite_title": issue.prerequisite_title,
                    "prerequisite_state": issue.prerequisite_state,
                    "dependency_state_gap": issue.dependency_state_gap,
                    "affected_reverse_dependent_reach": issue.affected_reverse_dependent_reach,
                    "transitive_reverse_dependents": issue.transitive_reverse_dependents,
                }),
            });
        }
    }

    let orphan_count = orphan_findings.len();
    let convergence_count = convergence_findings.len();
    let findings = orphan_findings
        .into_iter()
        .chain(convergence_findings)
        .collect();
    TicketGraphResult {
        metric: CountMetric {
            status: TrialStatus::Collected,
            count: Some(orphan_count),
            details: Some(if orphan_count == 0 {
                if convergence_count == 0 {
                    "all tickets participate in at least one depends_on relationship"
                        .to_string()
                } else {
                    format!(
                        "all tickets participate in at least one depends_on relationship; {convergence_count} dependency convergence finding(s) detected"
                    )
                }
            } else {
                format!(
                    "{orphan_count} ticket(s) have neither outgoing dependencies nor incoming dependees; {convergence_count} dependency convergence finding(s) detected"
                )
            }),
        },
        findings,
    }
}

fn failed_result(err: StorageError) -> TicketGraphResult {
    TicketGraphResult {
        metric: CountMetric {
            status: TrialStatus::Failed,
            count: None,
            details: Some(format!(
                "failed to inspect ticket dependency topology: {err}"
            )),
        },
        findings: Vec::new(),
    }
}

fn orphan_severity(state: Option<&str>) -> Severity {
    match state {
        Some("in-implementation") | Some("in-review") => Severity::High,
        _ => Severity::Medium,
    }
}

fn convergence_severity(state: Option<&str>) -> Severity {
    match state {
        Some("in-implementation") | Some("in-review") => Severity::High,
        _ => Severity::Medium,
    }
}

fn relative_ticket_path(
    repo_root: &Path,
    ticket_path: &Path,
) -> String {
    ticket_path
        .strip_prefix(repo_root)
        .map(format_output_path)
        .unwrap_or_else(|_| format_output_path(ticket_path))
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::{
            Path,
            PathBuf,
        },
        time::{
            SystemTime,
            UNIX_EPOCH,
        },
    };

    use chrono::Utc;
    use ticket_api::{
        model::edge::EdgeRecord,
        storage::TicketStore,
    };

    use super::evaluate;
    use crate::models::{
        Severity,
        TrialStatus,
    };

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn new(prefix: &str) -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("time went backwards")
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("{prefix}-{}-{unique}", std::process::id()));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn reports_only_orphan_tickets() {
        let repo = TestDir::new("audit-ticket-graph");
        let store = TicketStore::init(repo.path()).expect("init ticket store");
        let linked_parent = store
            .create(
                None,
                "tracker-improvement",
                Some("linked parent"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create linked parent");
        let linked_child = store
            .create(
                None,
                "tracker-improvement",
                Some("linked child"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create linked child");
        let orphan = store
            .create(
                None,
                "tracker-improvement",
                Some("orphan task"),
                Some("in-implementation"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create orphan ticket");

        store
            .add_edge(EdgeRecord {
                from: linked_parent,
                to: linked_child,
                kind: "depends_on".to_string(),
                created_at: Utc::now(),
            })
            .expect("add edge");

        let result = evaluate(repo.path());

        assert!(matches!(result.metric.status, TrialStatus::Collected));
        assert_eq!(result.metric.count, Some(1));
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].category, "ticket_graph");
        assert!(matches!(result.findings[0].severity, Severity::High));
        assert_eq!(
            result.findings[0].evidence["ticket_id"],
            serde_json::json!(orphan)
        );
    }

    #[test]
    fn reports_unavailable_without_ticket_store() {
        let repo = TestDir::new("audit-ticket-graph-missing-store");

        let result = evaluate(repo.path());

        assert!(matches!(result.metric.status, TrialStatus::Unavailable));
        assert_eq!(result.metric.count, None);
        assert!(result.findings.is_empty());
    }

    #[test]
    fn reports_dependency_convergence_findings() {
        let repo = TestDir::new("audit-ticket-graph-convergence");
        let store = TicketStore::init(repo.path()).expect("init ticket store");
        let prerequisite = store
            .create(
                None,
                "tracker-improvement",
                Some("lagging prerequisite"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create prerequisite");
        let dependent = store
            .create(
                None,
                "tracker-improvement",
                Some("advanced dependent"),
                Some("in-review"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create dependent");

        store
            .add_edge(EdgeRecord {
                from: dependent,
                to: prerequisite,
                kind: "depends_on".to_string(),
                created_at: Utc::now(),
            })
            .expect("add edge");

        let result = evaluate(repo.path());

        assert!(matches!(result.metric.status, TrialStatus::Collected));
        assert_eq!(result.metric.count, Some(0));
        let convergence = result
            .findings
            .iter()
            .find(|finding| finding.metric_name == "dependency_convergence_count")
            .expect("convergence finding");
        assert!(matches!(convergence.severity, Severity::High));
        assert_eq!(
            convergence.evidence["dependent_id"],
            serde_json::json!(dependent)
        );
        assert_eq!(
            convergence.evidence["prerequisite_id"],
            serde_json::json!(prerequisite)
        );
        assert_eq!(convergence.evidence["dependency_state_gap"], serde_json::json!(2));
    }
}
