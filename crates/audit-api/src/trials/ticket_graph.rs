use std::{
    collections::HashMap,
    path::Path,
};

use serde_json::json;
use ticket_api::{
    error::StorageError,
    storage::TicketStore,
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

    let mut dependency_counts = HashMap::new();
    let mut dependee_counts = HashMap::new();
    for edge in edges.into_iter().filter(|edge| edge.kind == "depends_on") {
        *dependency_counts.entry(edge.from).or_insert(0usize) += 1;
        *dependee_counts.entry(edge.to).or_insert(0usize) += 1;
    }

    let mut tickets = tickets;
    tickets.sort_by(|left, right| left.path.cmp(&right.path));

    let findings: Vec<AuditFinding> = tickets
        .into_iter()
        .filter_map(|ticket| {
            let dependency_count = dependency_counts.get(&ticket.id).copied().unwrap_or(0);
            let dependee_count = dependee_counts.get(&ticket.id).copied().unwrap_or(0);
            if dependency_count > 0 || dependee_count > 0 {
                return None;
            }

            let display_path = relative_ticket_path(repo_root, &ticket.path);
            let title = ticket
                .title
                .clone()
                .unwrap_or_else(|| ticket.id.to_string());
            Some(AuditFinding {
                id: format!("ticket_graph:{}", ticket.id),
                category: "ticket_graph".to_string(),
                severity: orphan_severity(ticket.state.as_deref()),
                summary: format!(
                    "{} is not linked into the depends_on graph.",
                    title
                ),
                path: Some(display_path.clone()),
                line: None,
                metric_name: "orphan_ticket_count".to_string(),
                metric_value: json!(1),
                threshold: Some(json!(0)),
                instructions: vec![
                    format!(
                        "Link {} to its real prerequisites with depends_on edges, or attach it under an existing parent ticket.",
                        display_path
                    ),
                    "If this is otherwise standalone work, create a project-tracker parent ticket that depends_on this ticket so the task is still connected to the broader plan.".to_string(),
                ],
                evidence: json!({
                    "ticket_id": ticket.id,
                    "path": display_path,
                    "state": ticket.state,
                    "dependency_count": dependency_count,
                    "dependee_count": dependee_count,
                }),
            })
        })
        .collect();

    let orphan_count = findings.len();
    TicketGraphResult {
        metric: CountMetric {
            status: TrialStatus::Collected,
            count: Some(orphan_count),
            details: Some(if orphan_count == 0 {
                "all tickets participate in at least one depends_on relationship"
                    .to_string()
            } else {
                format!(
                    "{orphan_count} ticket(s) have neither outgoing dependencies nor incoming dependees"
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
            let path = std::env::temp_dir().join(format!(
                "{prefix}-{}-{unique}",
                std::process::id()
            ));
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
}