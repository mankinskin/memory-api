use std::{
    collections::HashSet,
    sync::Arc,
};

use axum::{
    extract::{
        Extension,
        Query,
        State,
    },
    http::StatusCode,
    response::{
        IntoResponse,
        Json,
        Response,
    },
};
use serde::{
    Deserialize,
    Serialize,
};
use ticket_api::{
    storage::store::TicketStore,
    workflow::{
        BoardExcludedCandidate,
        WorkflowModel,
        WorkflowTreeNode,
        apply_board_filter,
    },
};
use uuid::Uuid;
use viewer_api::error::{
    ApiError,
    RequestIdExt,
};

use crate::serve::{
    AppState,
    error::{
        storage_err,
        task_join_err,
    },
    handlers::tickets::{
        ticket_ref_from_indexed,
        TicketRef,
    },
};

#[derive(Deserialize)]
pub struct WorkflowNextQuery {
    pub workspace: String,
    pub root: Option<Uuid>,
    pub filter: Option<String>,
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct WorkflowTreeQuery {
    pub workspace: String,
    pub root: Uuid,
}

#[derive(Clone)]
struct NextScope {
    root: WorkflowRootSummary,
    reachable_dependencies: usize,
    blocked_dependencies: usize,
    remaining_blockers: HashSet<Uuid>,
    blocker_tree: WorkflowTreeItem,
}

#[derive(Clone, Serialize)]
pub struct WorkflowRootSummary {
    pub id: String,
    pub ticket_ref: TicketRef,
    pub title: Option<String>,
    pub state: Option<String>,
}

#[derive(Serialize)]
pub struct WorkflowCandidateItem {
    pub rank: usize,
    pub id: String,
    pub ticket_ref: TicketRef,
    pub title: Option<String>,
    pub state: Option<String>,
    #[serde(rename = "type")]
    pub ticket_type: String,
    pub priority: String,
    pub effort: Option<u64>,
    pub dependency_count: usize,
    pub remaining_blocker_count: usize,
    pub dependee_count: usize,
    pub transitive_reverse_dependents: usize,
    pub affected_reverse_dependent_reach: usize,
    pub max_affected_dependent_state: Option<String>,
    pub dependency_state_gap: usize,
    pub became_actionable_at: Option<String>,
    pub last_blocker_progress_at: Option<String>,
    pub created_at: String,
}

#[derive(Clone, Serialize)]
pub struct WorkflowTreeItem {
    pub id: String,
    pub ticket_ref: TicketRef,
    pub title: Option<String>,
    pub state: Option<String>,
    #[serde(rename = "type")]
    pub ticket_type: String,
    pub priority: String,
    pub remaining_blocker_count: usize,
    pub unresolved_frontier_leaf_count: usize,
    pub frontier_leaf_ids: Vec<String>,
    pub blocker_distance: usize,
    pub is_frontier: bool,
    pub dependency_count: usize,
    pub immediate_dependees: usize,
    pub transitive_reverse_dependents: usize,
    pub affected_reverse_dependent_reach: usize,
    pub dependency_state_gap: usize,
    pub became_actionable_at: Option<String>,
    pub last_blocker_progress_at: Option<String>,
    pub children: Vec<WorkflowTreeItem>,
}

#[derive(Serialize)]
pub struct ScopeMetadata {
    pub workspace: String,
    pub active_index_root: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filter: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<String>,
}

#[derive(Serialize)]
pub struct WorkflowNextResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub scope: ScopeMetadata,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub root: Option<WorkflowRootSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reachable_dependencies: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_dependencies: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_blocker_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocker_tree: Option<WorkflowTreeItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontier_count: Option<usize>,
    pub count: usize,
    pub items: Vec<WorkflowCandidateItem>,
    pub excluded_by_board: Vec<BoardExcludedCandidate>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
pub struct WorkflowTreeResponse {
    pub request_id: String,
    pub active_workspace: String,
    pub workspace: String,
    pub kind: String,
    pub root: WorkflowTreeItem,
    pub frontier_count: usize,
    pub frontier_items: Vec<WorkflowCandidateItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reachable_dependents: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_dependents: Option<usize>,
}

#[derive(Clone, Copy)]
enum TreeKind {
    Blockers,
    UnblockedBy,
}

pub async fn workflow_next(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<WorkflowNextQuery>,
) -> Response {
    let (workspace, store) = match resolve_workspace_request(
        &state,
        &params.workspace,
        &rid.0,
    ) {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        match workflow_next_payload(
            &store,
            &workspace,
            &params,
            &request_id,
        ) {
            Ok(payload) => Json(payload).into_response(),
            Err(response) => response,
        }
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "workflow next request"))
}

fn workflow_next_payload(
    store: &TicketStore,
    workspace: &str,
    params: &WorkflowNextQuery,
    request_id: &str,
) -> Result<WorkflowNextResponse, Response> {
    let scope_root = params.root.map(|id| id.to_string());
    let scope_filter = params.filter.clone();
    let active_index_root = store.index_root.display().to_string();
    let tickets = store
        .list(None, None, None)
        .map_err(|error| storage_err(error, request_id))?;
    let all_edges = store
        .list_all_edges()
        .map_err(|error| storage_err(error, request_id))?;
    let model = WorkflowModel::build(store, tickets.clone(), all_edges)
        .map_err(|error| storage_err(error, request_id))?;
    ensure_next_root_exists(&model, params.root, request_id)?;

    let next_scope = build_optional_next_scope(params.root, &model, store, workspace)
        .map_err(|error| storage_err(error, request_id))?;
    let (mut candidates, excluded_by_board, warnings, frontier_count) = collect_board_filtered_candidates(
        &tickets,
        &model,
        params,
        next_scope.as_ref(),
        store,
    );
    apply_next_limit(&mut candidates, params);

    let empty_satisfied = HashSet::new();
    let items = build_candidate_items(
        &candidates,
        &model,
        store,
        workspace,
        &empty_satisfied,
    )
    .map_err(|error| storage_err(error, request_id))?;
    Ok(WorkflowNextResponse {
        request_id: request_id.to_string(),
        active_workspace: workspace.to_string(),
        workspace: workspace.to_string(),
        scope: ScopeMetadata {
            workspace: workspace.to_string(),
            active_index_root,
            filter: scope_filter,
            root: scope_root,
        },
        root: next_scope.as_ref().map(|scope| scope.root.clone()),
        reachable_dependencies: next_scope
            .as_ref()
            .map(|scope| scope.reachable_dependencies),
        blocked_dependencies: next_scope
            .as_ref()
            .map(|scope| scope.blocked_dependencies),
        remaining_blocker_count: next_scope
            .as_ref()
            .map(|scope| scope.remaining_blockers.len()),
        blocker_tree: next_scope.as_ref().map(|scope| scope.blocker_tree.clone()),
        frontier_count: next_scope.as_ref().map(|_| frontier_count),
        count: items.len(),
        items,
        excluded_by_board,
        warnings,
    })
}

fn ensure_next_root_exists(
    model: &WorkflowModel,
    root: Option<Uuid>,
    request_id: &str,
) -> Result<(), Response> {
    if root.is_some_and(|root_id| model.ticket(&root_id).is_none()) {
        return Err(
            ApiError::not_found("ticket", request_id)
                .into_response_with_status(StatusCode::NOT_FOUND),
        );
    }
    Ok(())
}

fn build_optional_next_scope(
    root: Option<Uuid>,
    model: &WorkflowModel,
    store: &TicketStore,
    workspace: &str,
) -> Result<Option<NextScope>, ticket_api::error::StorageError> {
    root.map(|root_id| build_next_scope(root_id, model, store, workspace))
        .transpose()
}

fn collect_board_filtered_candidates(
    tickets: &[ticket_api::storage::indexed::IndexedTicket],
    model: &WorkflowModel,
    params: &WorkflowNextQuery,
    next_scope: Option<&NextScope>,
    store: &TicketStore,
) -> (Vec<Uuid>, Vec<BoardExcludedCandidate>, Vec<String>, usize) {
    let filtered_scope = WorkflowModel::filter_scope(tickets, params.filter.as_deref());
    let candidate_scope = intersect_scopes(
        filtered_scope,
        next_scope.map(|scope| &scope.remaining_blockers),
    );
    let mut candidates = model.actionable_candidate_ids(candidate_scope.as_ref());
    model.sort_candidate_ids(&mut candidates);
    let board_filtered = apply_board_filter(candidates, store.board_show(None).ok().as_ref(), false);
    let frontier_count = board_filtered.candidates.len();
    (
        board_filtered.candidates,
        board_filtered.excluded_by_board,
        board_filtered.warnings,
        frontier_count,
    )
}

fn apply_next_limit(candidates: &mut Vec<Uuid>, params: &WorkflowNextQuery) {
    match params.limit {
        Some(limit) => candidates.truncate(limit.min(100)),
        None if params.root.is_none() => candidates.truncate(20),
        None => {}
    }
}

pub async fn workflow_blockers(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<WorkflowTreeQuery>,
) -> Response {
    workflow_tree_response(state, rid.0, params, TreeKind::Blockers).await
}

pub async fn workflow_unblocked_by(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<WorkflowTreeQuery>,
) -> Response {
    workflow_tree_response(state, rid.0, params, TreeKind::UnblockedBy).await
}

async fn workflow_tree_response(
    state: AppState,
    request_id: String,
    params: WorkflowTreeQuery,
    kind: TreeKind,
) -> Response {
    let (workspace, store) = match resolve_workspace_request(
        &state,
        &params.workspace,
        &request_id,
    ) {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        match workflow_tree_payload(
            &store,
            &workspace,
            &request_id,
            params.root,
            kind,
        ) {
            Ok(payload) => Json(payload).into_response(),
            Err(response) => response,
        }
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "workflow tree request"))
}

fn workflow_tree_payload(
    store: &TicketStore,
    workspace: &str,
    request_id: &str,
    root: Uuid,
    kind: TreeKind,
) -> Result<WorkflowTreeResponse, Response> {
    let tickets = store
        .list(None, None, None)
        .map_err(|error| storage_err(error, request_id))?;
    let all_edges = store
        .list_all_edges()
        .map_err(|error| storage_err(error, request_id))?;
    let model = WorkflowModel::build(store, tickets, all_edges)
        .map_err(|error| storage_err(error, request_id))?;

    if model.ticket(&root).is_none() {
        return Err(
            ApiError::not_found("ticket", request_id)
                .into_response_with_status(StatusCode::NOT_FOUND),
        );
    }

    let tree_info = tree_kind_payload(&model, root, kind, request_id)?;
    let root_item = build_tree_item(tree_info.tree, &model, store, workspace)
        .map_err(|error| storage_err(error, request_id))?;
    let empty_satisfied = HashSet::new();
    let frontier_items = build_candidate_items(
        &tree_info.frontier_ids,
        &model,
        store,
        workspace,
        tree_info.satisfied_ids.as_ref().unwrap_or(&empty_satisfied),
    )
    .map_err(|error| storage_err(error, request_id))?;

    Ok(WorkflowTreeResponse {
        request_id: request_id.to_string(),
        active_workspace: workspace.to_string(),
        workspace: workspace.to_string(),
        kind: tree_info.kind_label.to_string(),
        root: root_item,
        frontier_count: frontier_items.len(),
        frontier_items,
        reachable_dependents: tree_info.reachable_dependents,
        blocked_dependents: tree_info.blocked_dependents,
    })
}

struct TreePayload<'a> {
    tree: WorkflowTreeNode,
    frontier_ids: Vec<Uuid>,
    reachable_dependents: Option<usize>,
    blocked_dependents: Option<usize>,
    kind_label: &'a str,
    satisfied_ids: Option<HashSet<Uuid>>,
}

fn tree_kind_payload<'a>(
    model: &'a WorkflowModel,
    root: Uuid,
    kind: TreeKind,
    request_id: &str,
) -> Result<TreePayload<'a>, Response> {
    match kind {
        TreeKind::Blockers => blockers_tree_payload(model, root, request_id),
        TreeKind::UnblockedBy => unblocked_by_tree_payload(model, root, request_id),
    }
}

fn blockers_tree_payload<'a>(
    model: &'a WorkflowModel,
    root: Uuid,
    request_id: &str,
) -> Result<TreePayload<'a>, Response> {
    let tree = model
        .blocker_tree(root)
        .ok_or_else(|| {
            ApiError::not_found("ticket", request_id)
                .into_response_with_status(StatusCode::NOT_FOUND)
        })?;
    let frontier_ids = tree.frontier_leaf_ids.clone();
    Ok(TreePayload {
        tree,
        frontier_ids,
        reachable_dependents: None,
        blocked_dependents: None,
        kind_label: "blockers",
        satisfied_ids: None,
    })
}

fn unblocked_by_tree_payload<'a>(
    model: &'a WorkflowModel,
    root: Uuid,
    request_id: &str,
) -> Result<TreePayload<'a>, Response> {
    let satisfied_ids = HashSet::from([root]);
    let tree = model
        .unlock_tree_with_satisfied(root, &satisfied_ids)
        .ok_or_else(|| {
            ApiError::not_found("ticket", request_id)
                .into_response_with_status(StatusCode::NOT_FOUND)
        })?;
    let dependent_ids = model.reverse_dependents(root);
    let blocked_dependents = dependent_ids
        .iter()
        .filter(|ticket_id| {
            !model
                .unresolved_dependencies_excluding(ticket_id, &satisfied_ids)
                .is_empty()
        })
        .count();

    Ok(TreePayload {
        tree,
        frontier_ids: model.unlock_frontier_leaf_ids_with_satisfied(root, &satisfied_ids),
        reachable_dependents: Some(dependent_ids.len()),
        blocked_dependents: Some(blocked_dependents),
        kind_label: "unblocked-by",
        satisfied_ids: Some(satisfied_ids),
    })
}

fn resolve_workspace_request(
    state: &AppState,
    requested_workspace: &str,
    request_id: &str,
) -> Result<(String, Arc<TicketStore>), Response> {
    state.resolve_public_workspace_request(requested_workspace, request_id)
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

fn build_next_scope(
    root_id: Uuid,
    model: &WorkflowModel,
    store: &TicketStore,
    active_workspace: &str,
) -> Result<NextScope, ticket_api::error::StorageError> {
    let scope = model
        .root_blocker_scope(root_id)
        .ok_or(ticket_api::error::StorageError::NotFound(root_id))?;

    Ok(NextScope {
        root: build_root_summary(root_id, model, store, active_workspace)?,
        reachable_dependencies: scope.reachable_dependencies,
        blocked_dependencies: scope.blocked_dependencies,
        remaining_blockers: scope.remaining_blockers,
        blocker_tree: build_tree_item(scope.tree, model, store, active_workspace)?,
    })
}

fn build_root_summary(
    ticket_id: Uuid,
    model: &WorkflowModel,
    store: &TicketStore,
    active_workspace: &str,
) -> Result<WorkflowRootSummary, ticket_api::error::StorageError> {
    let ticket = model
        .ticket(&ticket_id)
        .ok_or(ticket_api::error::StorageError::NotFound(ticket_id))?;
    Ok(WorkflowRootSummary {
        id: ticket_id.to_string(),
        ticket_ref: ticket_ref_from_indexed(store, active_workspace, ticket)?,
        title: ticket.title.clone(),
        state: ticket.state.clone(),
    })
}

fn build_candidate_items(
    ids: &[Uuid],
    model: &WorkflowModel,
    store: &TicketStore,
    active_workspace: &str,
    satisfied_ids: &HashSet<Uuid>,
) -> Result<Vec<WorkflowCandidateItem>, ticket_api::error::StorageError> {
    let mut items = Vec::with_capacity(ids.len());
    for (rank, ticket_id) in ids.iter().enumerate() {
        let Some(ticket) = model.ticket(ticket_id) else {
            continue;
        };
        let metrics = model.metrics(ticket_id).cloned().unwrap_or_default();
        items.push(WorkflowCandidateItem {
            rank: rank + 1,
            id: ticket.id.to_string(),
            ticket_ref: ticket_ref_from_indexed(store, active_workspace, ticket)?,
            title: ticket.title.clone(),
            state: ticket.state.clone(),
            ticket_type: ticket.type_id.clone(),
            priority: model.priority_or_none(ticket_id).to_string(),
            effort: model.effort(ticket_id),
            dependency_count: model.dependency_count(ticket_id),
            remaining_blocker_count: model
                .unresolved_dependencies_excluding(ticket_id, satisfied_ids)
                .len(),
            dependee_count: model.dependee_count(ticket_id),
            transitive_reverse_dependents: metrics.transitive_reverse_dependents,
            affected_reverse_dependent_reach: metrics.affected_reverse_dependent_reach,
            max_affected_dependent_state: metrics.max_affected_dependent_state,
            dependency_state_gap: metrics.dependency_state_gap,
            became_actionable_at: metrics
                .became_actionable_at
                .map(|timestamp| timestamp.to_rfc3339()),
            last_blocker_progress_at: metrics
                .last_blocker_progress_at
                .map(|timestamp| timestamp.to_rfc3339()),
            created_at: ticket.created_at.to_rfc3339(),
        });
    }
    Ok(items)
}

fn build_tree_item(
    node: WorkflowTreeNode,
    model: &WorkflowModel,
    store: &TicketStore,
    active_workspace: &str,
) -> Result<WorkflowTreeItem, ticket_api::error::StorageError> {
    let ticket = model
        .ticket(&node.ticket_id)
        .ok_or(ticket_api::error::StorageError::NotFound(node.ticket_id))?;
    let metrics = model.metrics(&node.ticket_id).cloned().unwrap_or_default();
    let children = node
        .children
        .into_iter()
        .map(|child| build_tree_item(child, model, store, active_workspace))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(WorkflowTreeItem {
        id: node.ticket_id.to_string(),
        ticket_ref: ticket_ref_from_indexed(store, active_workspace, ticket)?,
        title: node.title,
        state: node.state,
        ticket_type: ticket.type_id.clone(),
        priority: node.priority,
        remaining_blocker_count: node.remaining_blocker_count,
        unresolved_frontier_leaf_count: node.unresolved_frontier_leaf_count,
        frontier_leaf_ids: node
            .frontier_leaf_ids
            .into_iter()
            .map(|id| id.to_string())
            .collect(),
        blocker_distance: node.blocker_distance,
        is_frontier: node.is_frontier,
        dependency_count: node.dependency_count,
        immediate_dependees: node.immediate_dependees,
        transitive_reverse_dependents: node.transitive_reverse_dependents,
        affected_reverse_dependent_reach: node.affected_reverse_dependent_reach,
        dependency_state_gap: node.dependency_state_gap,
        became_actionable_at: metrics
            .became_actionable_at
            .map(|timestamp| timestamp.to_rfc3339()),
        last_blocker_progress_at: metrics
            .last_blocker_progress_at
            .map(|timestamp| timestamp.to_rfc3339()),
        children,
    })
}


#[cfg(test)]
#[path = "workflow_tests.rs"]
mod tests;
