use std::collections::HashSet;

use serde_json::Value;
use ticket_api::workflow::{
    WorkflowModel,
    apply_board_filter,
};
use uuid::Uuid;

use super::{
    types::*,
    *,
};

impl TicketServer {
    pub(crate) async fn next_tickets_tool(
        &self,
        input: NextTicketsInput,
    ) -> Result<CallToolResult, McpError> {
        let limit = input.limit.map(|value| value.min(100));
        let filter = input.filter;
        let workspace = input.workspace;
        let root = input.root;

        // Resolve the active index root for scope metadata before entering
        // the store closure so it is always present in the response.
        let active_index_root = self
            .resolve_workspace_root(&workspace)
            .map(|path| path.display().to_string())
            .unwrap_or_default();

        let (
            items,
            excluded_by_board,
            warnings,
            resolved_root_id,
            root_summary,
            reachable_dependencies,
            blocked_dependencies,
            remaining_blocker_count,
            blocker_tree,
            frontier_count,
        ) = self
            .with_store_ext(&workspace, |store| {
                let board_snap = store.board_show(None).ok();
                let tickets =
                    store.list(None, None, None).map_err(Self::store_err)?;
                let filtered_scope =
                    WorkflowModel::filter_scope(&tickets, filter.as_deref());
                let model = WorkflowModel::build(
                    store,
                    tickets,
                    store.list_all_edges().map_err(Self::store_err)?,
                )
                .map_err(Self::store_err)?;

                let root_id = root
                    .as_deref()
                    .map(|r| Self::resolve_uuid_for_read(store, r))
                    .transpose()?;
                let root_scope = root_id.and_then(|rid| {
                    model.root_blocker_scope(rid).map(|scope| (rid, scope))
                });
                let root_remaining_blockers = root_scope
                    .as_ref()
                    .map(|(_, scope)| scope.remaining_blockers.clone());

                let candidate_scope = intersect_option_scopes(
                    filtered_scope,
                    root_remaining_blockers,
                );

                let mut candidates =
                    model.actionable_candidate_ids(candidate_scope.as_ref());
                model.sort_candidate_ids(&mut candidates);
                let board_filtered =
                    apply_board_filter(candidates, board_snap.as_ref(), false);
                let full_frontier_count = board_filtered.candidates.len();
                let mut candidates = board_filtered.candidates;
                match limit {
                    Some(limit) => candidates.truncate(limit),
                    None if root_scope.is_none() => candidates.truncate(20),
                    None => {},
                }

                let empty_satisfied = HashSet::new();
                let root_summary = root_scope.as_ref().and_then(|(rid, _)| {
                    model.ticket(rid).map(|ticket| {
                        serde_json::json!({
                            "id": rid.to_string(),
                            "title": ticket.title,
                            "state": ticket.state,
                        })
                    })
                });
                let (
                    reachable_dependencies,
                    blocked_dependencies,
                    remaining_blocker_count,
                    blocker_tree,
                    frontier_count,
                ) = if let Some((_, scope)) = root_scope {
                    (
                        Some(scope.reachable_dependencies),
                        Some(scope.blocked_dependencies),
                        Some(scope.remaining_blockers.len()),
                        Some(tree_item(scope.tree, &model, &empty_satisfied)),
                        Some(full_frontier_count),
                    )
                } else {
                    (None, None, None, None, None)
                };

                Ok((
                    ranked_items(&candidates, &model, &empty_satisfied),
                    board_filtered.excluded_by_board,
                    board_filtered.warnings,
                    root_id.map(|id| id.to_string()),
                    root_summary,
                    reachable_dependencies,
                    blocked_dependencies,
                    remaining_blocker_count,
                    blocker_tree,
                    frontier_count,
                ))
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "scope": {
                "workspace": workspace,
                "active_index_root": active_index_root,
                "filter": filter,
                "root": resolved_root_id,
            },
            "root": root_summary,
            "reachable_dependencies": reachable_dependencies,
            "blocked_dependencies": blocked_dependencies,
            "remaining_blocker_count": remaining_blocker_count,
            "blocker_tree": blocker_tree,
            "frontier_count": frontier_count,
            "count": items.len(),
            "items": items,
            "excluded_by_board": excluded_by_board,
            "warnings": warnings,
        }))
    }
}

fn intersect_option_scopes(
    a: Option<HashSet<Uuid>>,
    b: Option<HashSet<Uuid>>,
) -> Option<HashSet<Uuid>> {
    match (a, b) {
        (Some(set_a), Some(set_b)) =>
            Some(set_a.intersection(&set_b).copied().collect()),
        (Some(set_a), None) => Some(set_a),
        (None, Some(set_b)) => Some(set_b),
        (None, None) => None,
    }
}

fn ranked_items(
    candidates: &[Uuid],
    model: &WorkflowModel,
    satisfied_ids: &HashSet<Uuid>,
) -> Vec<Value> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(rank, ticket_id)| {
            let ticket = model.ticket(ticket_id)?;
            let metrics = model.metrics(ticket_id).cloned().unwrap_or_default();
            serde_json::json!({
                "rank": rank + 1,
                "id": ticket.id.to_string(),
                "title": ticket.title,
                "state": ticket.state,
                "type": ticket.type_id,
                "priority": model.priority_or_none(ticket_id),
                "effort": model.effort(ticket_id),
                "dependency_count": model.dependency_count(ticket_id),
                "remaining_blocker_count": model
                    .unresolved_dependencies_excluding(ticket_id, satisfied_ids)
                    .len(),
                "dependee_count": model.dependee_count(ticket_id),
                "transitive_reverse_dependents": metrics.transitive_reverse_dependents,
                "affected_reverse_dependent_reach": metrics.affected_reverse_dependent_reach,
                "max_affected_dependent_state": metrics.max_affected_dependent_state,
                "dependency_state_gap": metrics.dependency_state_gap,
                "became_actionable_at": metrics
                    .became_actionable_at
                    .map(|timestamp| timestamp.to_rfc3339()),
                "last_blocker_progress_at": metrics
                    .last_blocker_progress_at
                    .map(|timestamp| timestamp.to_rfc3339()),
                "created_at": ticket.created_at.to_rfc3339(),
            })
            .into()
        })
        .collect()
}

fn tree_item(
    node: ticket_api::workflow::WorkflowTreeNode,
    model: &WorkflowModel,
    satisfied_ids: &HashSet<Uuid>,
) -> Value {
    let ticket = model.ticket(&node.ticket_id);
    let metrics = model.metrics(&node.ticket_id).cloned().unwrap_or_default();
    let created_at = ticket.map(|ticket| ticket.created_at.to_rfc3339());
    let ticket_type = ticket.map(|ticket| ticket.type_id.clone());

    serde_json::json!({
        "id": node.ticket_id,
        "title": node.title,
        "state": node.state,
        "type": ticket_type,
        "priority": node.priority,
        "remaining_blocker_count": node.remaining_blocker_count,
        "remaining_blockers": model.unresolved_dependencies_excluding(&node.ticket_id, satisfied_ids),
        "unresolved_frontier_leaf_count": node.unresolved_frontier_leaf_count,
        "frontier_leaf_ids": node.frontier_leaf_ids,
        "blocker_distance": node.blocker_distance,
        "is_frontier": node.is_frontier,
        "dependency_count": node.dependency_count,
        "dependee_count": node.immediate_dependees,
        "transitive_reverse_dependents": node.transitive_reverse_dependents,
        "affected_reverse_dependent_reach": node.affected_reverse_dependent_reach,
        "dependency_state_gap": node.dependency_state_gap,
        "became_actionable_at": metrics
            .became_actionable_at
            .map(|timestamp| timestamp.to_rfc3339()),
        "last_blocker_progress_at": metrics
            .last_blocker_progress_at
            .map(|timestamp| timestamp.to_rfc3339()),
        "created_at": created_at,
        "children": node
            .children
            .into_iter()
            .map(|child| tree_item(child, model, satisfied_ids))
            .collect::<Vec<_>>(),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::Value;
    use ticket_api::{
        model::filesystem::ScanRoot,
        storage::store::TicketStore,
    };

    use super::*;

    fn result_json(result: &CallToolResult) -> Value {
        let text = result
            .content
            .iter()
            .find_map(|content| {
                if let rmcp::model::RawContent::Text(text) = &content.raw {
                    Some(text.text.as_str())
                } else {
                    None
                }
            })
            .expect("text content");
        serde_json::from_str(text).expect("valid JSON")
    }

    #[tokio::test]
    async fn next_tickets_startup_policy_discovers_child_tickets() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace_root = temp.path().join("workspace");
        let parent_root = workspace_root.join(".ticket");
        let parent = TicketStore::init(&parent_root).expect("parent store");
        let child_root = workspace_root.join("child").join(".ticket");
        let child = TicketStore::init(&child_root).expect("child store");
        let blocker_id = child
            .create(
                None,
                "tracker-improvement",
                Some("child workspace prerequisite"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create child ticket");
        let tracker_id = child
            .create(
                None,
                "tracker-improvement",
                Some("child workspace tracker"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create child tracker");
        child
            .add_edge(ticket_api::model::edge::EdgeRecord {
                from: tracker_id,
                to: blocker_id,
                kind: "depends_on".to_string(),
                created_at: chrono::Utc::now(),
            })
            .expect("add dependency edge");
        drop(child);
        drop(parent);

        let store = open_canonical_store(&parent_root)
            .expect("apply workspace policy at MCP startup");
        drop(store);

        let server = TicketServer::new(parent_root);
        let result = server
            .next_tickets_tool(NextTicketsInput {
                workspace: workspace_root.display().to_string(),
                limit: None,
                filter: None,
                root: Some(tracker_id.to_string()),
            })
            .await
            .expect("resolve child ticket through parent workspace");
        let json = result_json(&result);

        assert_eq!(
            json["scope"]["root"].as_str(),
            Some(tracker_id.to_string().as_str())
        );
        assert_eq!(
            json["items"][0]["id"].as_str(),
            Some(blocker_id.to_string().as_str())
        );
    }

    #[tokio::test]
    async fn next_tickets_missing_root_reports_all_scanned_workspaces() {
        let temp = tempfile::tempdir().expect("tempdir");
        let parent = TicketStore::init(temp.path()).expect("parent store");
        let child_root = temp.path().join("child").join(".ticket");
        TicketStore::init(&child_root).expect("child store");
        parent
            .add_scan_root(ScanRoot {
                path: child_root.join("tickets"),
                label: "child".to_string(),
            })
            .expect("register child scan root");
        let parent_root = parent.index_root.clone();
        drop(parent);

        let server = TicketServer::new(parent_root);
        let error = server
            .next_tickets_tool(NextTicketsInput {
                workspace: temp.path().display().to_string(),
                limit: None,
                filter: None,
                root: Some("deadbeef".to_string()),
            })
            .await
            .expect_err("missing root should fail");

        assert!(error.to_string().contains("searched workspaces"));
        let diagnostic = error.to_string();
        let child_scan_root = child_root
            .join("tickets")
            .display()
            .to_string()
            .replace('\\', "/");
        assert!(
            diagnostic.contains(&child_scan_root),
            "missing child scan root in diagnostic: {diagnostic}"
        );
    }
}
