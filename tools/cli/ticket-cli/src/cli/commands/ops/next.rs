use std::collections::HashSet;

use serde_json::{
    Value,
    json,
};
use ticket_api::{
    storage::store::TicketStore,
    workflow::{
        WorkflowModel,
        WorkflowTreeNode,
        apply_board_filter,
    },
};
use uuid::Uuid;

use crate::cli::{
    BlockersArgs,
    CliRunError,
    NextArgs,
    UnblockedByArgs,
    commands::{
        resolve_uuid_prefix,
        ticket_workspace_metadata_for_id,
    },
};

struct NextScope {
    root: Value,
    reachable_dependencies: usize,
    blocked_dependencies: usize,
    remaining_blockers: HashSet<Uuid>,
    blocker_tree: Value,
}

pub(super) fn run(
    args: NextArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let board_snap = store.board_show(None).ok();
    let all_tickets = store.list(None, None, None)?;
    let filtered_scope =
        WorkflowModel::filter_scope(&all_tickets, args.filter.as_deref());
    let model =
        WorkflowModel::build(store, all_tickets, store.list_all_edges()?)?;
    let root_id = args
        .root
        .as_deref()
        .map(|root| resolve_uuid_prefix(root, store))
        .transpose()?;
    let next_scope = root_id
        .map(|resolved_root_id| build_next_scope(resolved_root_id, &model));
    let candidate_scope = intersect_scopes(
        filtered_scope,
        next_scope.as_ref().map(|scope| &scope.remaining_blockers),
    );
    let mut candidates =
        model.actionable_candidate_ids(candidate_scope.as_ref());
    model.sort_candidate_ids(&mut candidates);
    let board_filtered =
        apply_board_filter(candidates, board_snap.as_ref(), args.no_board);
    let frontier_count = board_filtered.candidates.len();
    let limited_candidates = limit_candidates(
        board_filtered.candidates,
        args.limit,
        root_id.is_some(),
    );

    let active_index_root = store.index_root.display().to_string();
    let mut payload = json!({
        "command": "next",
        "status": "ok",
        "scope": {
            "active_index_root": active_index_root,
            "filter": &args.filter,
            "root": root_id.map(|id| id.to_string()),
        },
        "count": limited_candidates.len(),
        "items": build_items(&limited_candidates, &model, store),
        "excluded_by_board": board_filtered.excluded_by_board,
        "warnings": board_filtered.warnings,
    });

    if let Some(scope) = next_scope {
        let obj = payload
            .as_object_mut()
            .expect("next payload should be a JSON object");
        obj.insert("root".to_string(), scope.root);
        obj.insert(
            "reachable_dependencies".to_string(),
            json!(scope.reachable_dependencies),
        );
        obj.insert(
            "blocked_dependencies".to_string(),
            json!(scope.blocked_dependencies),
        );
        obj.insert(
            "remaining_blocker_count".to_string(),
            json!(scope.remaining_blockers.len()),
        );
        obj.insert("blocker_tree".to_string(), scope.blocker_tree);
        obj.insert("frontier_count".to_string(), json!(frontier_count));
    }

    Ok(payload)
}

pub(super) fn run_unblocked_by(
    args: UnblockedByArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let root_id = resolve_uuid_prefix(&args.id, store)?;
    let model = WorkflowModel::build(
        store,
        store.list(None, None, None)?,
        store.list_all_edges()?,
    )?;
    let dependent_ids = model.reverse_dependents(root_id);
    let satisfied_ids = HashSet::from([root_id]);
    let tree = model
        .unlock_tree_with_satisfied(root_id, &satisfied_ids)
        .ok_or_else(|| {
            CliRunError::Storage(ticket_api::error::StorageError::NotFound(
                root_id,
            ))
        })?;
    let frontier_ids =
        model.unlock_frontier_leaf_ids_with_satisfied(root_id, &satisfied_ids);
    let blocked_dependents = dependent_ids
        .iter()
        .filter(|ticket_id| {
            !model
                .unresolved_dependencies_excluding(ticket_id, &satisfied_ids)
                .is_empty()
        })
        .count();

    Ok(json!({
        "command": "unblocked_by",
        "status": "ok",
        "kind": "unblocked_by",
        "root": build_tree_item(tree, &model, &satisfied_ids),
        "reachable_dependents": dependent_ids.len(),
        "blocked_dependents": blocked_dependents,
        "frontier_count": frontier_ids.len(),
        "frontier_items": build_candidate_items(
            &frontier_ids,
            &model,
            &satisfied_ids,
        ),
    }))
}

pub(super) fn run_blockers(
    args: BlockersArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let root_id = resolve_uuid_prefix(&args.id, store)?;
    let model = WorkflowModel::build(
        store,
        store.list(None, None, None)?,
        store.list_all_edges()?,
    )?;
    let tree = model.blocker_tree(root_id).ok_or_else(|| {
        CliRunError::Storage(ticket_api::error::StorageError::NotFound(root_id))
    })?;
    let frontier_ids = tree.frontier_leaf_ids.clone();
    let satisfied_ids = HashSet::new();

    Ok(json!({
        "command": "blockers",
        "status": "ok",
        "kind": "blockers",
        "root": build_tree_item(tree, &model, &satisfied_ids),
        "frontier_count": frontier_ids.len(),
        "frontier_items": build_candidate_items(
            &frontier_ids,
            &model,
            &satisfied_ids,
        ),
    }))
}

fn root_summary(
    root_id: Uuid,
    model: &WorkflowModel,
) -> Value {
    let root_ticket = model.ticket(&root_id);

    json!({
        "id": root_id,
        "title": root_ticket.and_then(|ticket| ticket.title.clone()),
        "state": root_ticket.and_then(|ticket| ticket.state.clone()),
    })
}

fn build_next_scope(
    root_id: Uuid,
    model: &WorkflowModel,
) -> NextScope {
    let empty_satisfied = HashSet::new();
    let scope = model
        .root_blocker_scope(root_id)
        .expect("resolved root id must exist in workflow model");

    NextScope {
        root: root_summary(root_id, model),
        reachable_dependencies: scope.reachable_dependencies,
        blocked_dependencies: scope.blocked_dependencies,
        blocker_tree: build_tree_item(scope.tree, model, &empty_satisfied),
        remaining_blockers: scope.remaining_blockers,
    }
}

fn intersect_scopes(
    primary: Option<HashSet<Uuid>>,
    secondary: Option<&HashSet<Uuid>>,
) -> Option<HashSet<Uuid>> {
    match (primary, secondary) {
        (Some(primary), Some(secondary)) => Some(
            primary
                .into_iter()
                .filter(|ticket_id| secondary.contains(ticket_id))
                .collect(),
        ),
        (Some(primary), None) => Some(primary),
        (None, Some(secondary)) => Some(secondary.iter().copied().collect()),
        (None, None) => None,
    }
}

fn limit_candidates(
    mut candidates: Vec<Uuid>,
    limit: Option<usize>,
    root_scoped: bool,
) -> Vec<Uuid> {
    match limit {
        Some(limit) => candidates.truncate(limit),
        None if !root_scoped => candidates.truncate(20),
        None => {},
    }
    candidates
}

fn build_items(
    candidates: &[Uuid],
    model: &WorkflowModel,
    store: &TicketStore,
) -> Vec<Value> {
    candidates
        .iter()
        .enumerate()
        .filter_map(|(rank, ticket_id)| {
            let ticket = model.ticket(ticket_id)?;
            let metrics = model.metrics(ticket_id).cloned().unwrap_or_default();
            json!({
                "rank": rank + 1,
                "id": ticket.id,
                "title": ticket.title,
                "state": ticket.state,
                "type": ticket.type_id,
                "priority": model.priority_or_none(ticket_id),
                "effort": model.effort(ticket_id),
                "dependency_count": model.dependency_count(ticket_id),
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
                "workspace": ticket_workspace_metadata_for_id(store, ticket.id),
            })
            .into()
        })
        .collect()
}

fn build_candidate_items(
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
            let remaining_blockers =
                model.unresolved_dependencies_excluding(ticket_id, satisfied_ids);
            json!({
                "rank": rank + 1,
                "id": ticket.id,
                "title": ticket.title,
                "state": ticket.state,
                "type": ticket.type_id,
                "priority": model.priority_or_none(ticket_id),
                "effort": model.effort(ticket_id),
                "dependency_count": model.dependency_count(ticket_id),
                "remaining_blocker_count": remaining_blockers.len(),
                "remaining_blockers": remaining_blockers,
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

fn build_tree_item(
    node: WorkflowTreeNode,
    model: &WorkflowModel,
    satisfied_ids: &HashSet<Uuid>,
) -> Value {
    let ticket = model.ticket(&node.ticket_id);
    let metrics = model.metrics(&node.ticket_id).cloned().unwrap_or_default();
    let created_at = ticket.map(|ticket| ticket.created_at.to_rfc3339());
    let ticket_type = ticket.map(|ticket| ticket.type_id.clone());

    json!({
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
            .map(|child| build_tree_item(child, model, satisfied_ids))
            .collect::<Vec<_>>(),
    })
}
