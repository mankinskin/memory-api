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
mod tests {
    use std::{
        collections::BTreeMap,
        sync::Arc,
    };

    use axum::{
        body::{
            Body,
            to_bytes,
        },
        http::{
            Request,
            StatusCode,
        },
    };
    use serde_json::{
        Value,
        json,
    };
    use ticket_api::{
        model::{
            edge::EdgeRecord,
            filesystem::ScanRoot,
        },
        storage::store::TicketStore,
    };
    use tower::ServiceExt;

    use crate::serve::{
        AppState,
        StreamBroker,
        WorkspaceRegistry,
        routes::build_router,
    };

    fn workspace_name(dir: &std::path::Path) -> String {
        crate::serve::registry::canonical_workspace_name_for_index_root(
            dir,
            "workspace",
        )
    }

    fn make_store(dir: &std::path::Path) -> Arc<TicketStore> {
        let store = Arc::new(TicketStore::init(dir).expect("open store"));
        store
            .add_scan_root(ScanRoot {
                path: dir.join("tickets"),
                label: "default".into(),
            })
            .expect("add scan root");
        store
    }

    fn make_router(store: Arc<TicketStore>) -> axum::Router {
        let state = AppState::new(
            Arc::new(WorkspaceRegistry::single_opened(store)),
            Arc::new(StreamBroker::new()),
        );
        build_router(state)
    }

    async fn get_json(app: axum::Router, uri: String) -> Value {
        let response = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn workflow_next_preserves_recent_actionable_order_and_supports_root_scope() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = workspace_name(dir.path());
        let store = make_store(dir.path());
        let app = make_router(Arc::clone(&store));

        let high_fields = BTreeMap::from([(String::from("priority"), json!("high"))]);
        let recently_actionable = store
            .create(
                None,
                "tracker-improvement",
                Some("Alpha recently actionable"),
                Some("ready"),
                high_fields.clone(),
                None,
                None,
            )
            .unwrap();
        let steadier_newer = store
            .create(
                None,
                "tracker-improvement",
                Some("Zulu steady ready work"),
                Some("ready"),
                high_fields.clone(),
                None,
                None,
            )
            .unwrap();
        let transient_blocker = store
            .create(
                None,
                "tracker-improvement",
                Some("Transient blocker"),
                Some("in-review"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();
        store
            .add_edge(EdgeRecord {
                from: recently_actionable,
                to: transient_blocker,
                kind: String::from("depends_on"),
                created_at: chrono::Utc::now(),
            })
            .unwrap();
        store.close(&transient_blocker, "done", None).unwrap();

        let next =
            get_json(app.clone(), format!("/api/workflow/next?workspace={workspace}")).await;
        let items = next["items"].as_array().unwrap();
        assert!(items.len() >= 2, "expected at least two candidates: {items:?}");
        assert_eq!(items[0]["id"], recently_actionable.to_string());
        assert_eq!(items[1]["id"], steadier_newer.to_string());
        assert!(items[0]["became_actionable_at"].as_str().is_some());
        assert!(items[1]["became_actionable_at"].as_str().is_some());
        assert_eq!(next["scope"]["workspace"], workspace.as_str());
        assert_eq!(next["excluded_by_board"], json!([]));
        assert_eq!(next["warnings"], json!([]));
        assert!(
            next["scope"]["active_index_root"].as_str().is_some(),
            "scope.active_index_root should be present",
        );
        assert!(next["scope"]["filter"].is_null());
        assert!(next["scope"]["root"].is_null());

        let root = store
            .create(
                None,
                "tracker-improvement",
                Some("Root ticket to unblock"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();
        let scoped_blocker = store
            .create(
                None,
                "tracker-improvement",
                Some("Scoped blocker"),
                Some("ready"),
                high_fields,
                None,
                None,
            )
            .unwrap();
        let intermediate_blocker = store
            .create(
                None,
                "tracker-improvement",
                Some("Intermediate blocker"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();
        let nested_leaf = store
            .create(
                None,
                "tracker-improvement",
                Some("Nested actionable blocker"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();

        for (from, to) in [
            (root, scoped_blocker),
            (root, intermediate_blocker),
            (intermediate_blocker, nested_leaf),
        ] {
            store
                .add_edge(EdgeRecord {
                    from,
                    to,
                    kind: String::from("depends_on"),
                    created_at: chrono::Utc::now(),
                })
                .unwrap();
        }

        let scoped = get_json(
            app,
            format!("/api/workflow/next?workspace={workspace}&root={root}"),
        )
        .await;
        assert_eq!(scoped["root"]["id"], root.to_string());
        assert_eq!(scoped["reachable_dependencies"], 3);
        assert_eq!(scoped["blocked_dependencies"], 1);
        assert_eq!(scoped["remaining_blocker_count"], 3);
        assert_eq!(scoped["frontier_count"], 2);
        let item_ids = scoped["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect::<std::collections::HashSet<_>>();
        assert!(item_ids.contains(scoped_blocker.to_string().as_str()));
        assert!(item_ids.contains(nested_leaf.to_string().as_str()));
        assert!(!item_ids.contains(intermediate_blocker.to_string().as_str()));
        assert_eq!(scoped["blocker_tree"]["id"], root.to_string());
        assert_eq!(scoped["scope"]["workspace"], workspace.as_str());
        assert_eq!(scoped["excluded_by_board"], json!([]));
        assert_eq!(scoped["warnings"], json!([]));
        assert!(
            scoped["scope"]["active_index_root"].as_str().is_some(),
            "scope.active_index_root should be present in scoped response",
        );
        assert_eq!(scoped["scope"]["root"], root.to_string());
    }

    #[tokio::test]
    async fn workflow_next_filters_board_active_candidates_into_excluded_by_board() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = workspace_name(dir.path());
        let store = make_store(dir.path());
        let app = make_router(Arc::clone(&store));

        let active = store
            .create(
                None,
                "tracker-improvement",
                Some("Active board ticket"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();
        let free = store
            .create(
                None,
                "tracker-improvement",
                Some("Free candidate ticket"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();

        store
            .board_configure(Some(ticket_api::BoardConfig {
                max_wip: 1,
                stale_after_secs: 3600,
                completed_audit_window_secs: 3600,
            }))
            .unwrap();
        store
            .board_check_in(&active, "http-parity-agent", 3600, "in flight", Vec::new())
            .unwrap();

        let next =
            get_json(app, format!("/api/workflow/next?workspace={workspace}")).await;
        let items = next["items"].as_array().unwrap();
        let excluded = next["excluded_by_board"].as_array().unwrap();
        let warnings = next["warnings"].as_array().unwrap();

        assert!(items.iter().any(|item| item["id"] == free.to_string()));
        assert!(!items.iter().any(|item| item["id"] == active.to_string()));
        assert!(
            excluded
                .iter()
                .any(|entry| entry["ticket_id"] == active.to_string()),
            "active board ticket must be surfaced in excluded_by_board: {excluded:?}"
        );
        assert!(
            warnings.iter().any(|warning| warning
                .as_str()
                .unwrap_or("")
                .contains("WIP limit reached")),
            "expected WIP warning, got {warnings:?}"
        );
    }

    #[tokio::test]
    async fn workflow_blockers_returns_nested_tree_and_frontier_items() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = workspace_name(dir.path());
        let store = make_store(dir.path());
        let app = make_router(Arc::clone(&store));

        let root = store
            .create(
                None,
                "tracker-improvement",
                Some("Root"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();
        let direct_leaf = store
            .create(
                None,
                "tracker-improvement",
                Some("Direct frontier leaf"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();
        let nested_parent = store
            .create(
                None,
                "tracker-improvement",
                Some("Nested parent"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();
        let nested_leaf = store
            .create(
                None,
                "tracker-improvement",
                Some("Nested frontier leaf"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();

        for (from, to) in [
            (root, nested_parent),
            (root, direct_leaf),
            (nested_parent, nested_leaf),
        ] {
            store
                .add_edge(EdgeRecord {
                    from,
                    to,
                    kind: String::from("depends_on"),
                    created_at: chrono::Utc::now(),
                })
                .unwrap();
        }

        let response = get_json(
            app,
            format!("/api/workflow/blockers?workspace={workspace}&root={root}"),
        )
        .await;

        assert_eq!(response["kind"], "blockers");
        assert_eq!(response["root"]["id"], root.to_string());
        assert_eq!(response["root"]["unresolved_frontier_leaf_count"], 2);
        let children = response["root"]["children"].as_array().unwrap();
        assert_eq!(children[0]["id"], direct_leaf.to_string());
        assert_eq!(children[1]["id"], nested_parent.to_string());
        let frontier = response["frontier_items"].as_array().unwrap();
        assert_eq!(frontier.len(), 2);
        assert_eq!(frontier[0]["id"], direct_leaf.to_string());
        assert_eq!(frontier[1]["id"], nested_leaf.to_string());
    }

    #[tokio::test]
    async fn workflow_unblocked_by_returns_nested_tree_and_frontier_items() {
        let dir = tempfile::tempdir().unwrap();
        let workspace = workspace_name(dir.path());
        let store = make_store(dir.path());
        let app = make_router(Arc::clone(&store));

        let root = store
            .create(
                None,
                "tracker-improvement",
                Some("Shared prerequisite"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();
        let direct = store
            .create(
                None,
                "tracker-improvement",
                Some("Direct dependent"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();
        let transitive = store
            .create(
                None,
                "tracker-improvement",
                Some("Transitive dependent"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();
        let extra_blocker = store
            .create(
                None,
                "tracker-improvement",
                Some("Other blocker"),
                Some("ready"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();
        let still_blocked = store
            .create(
                None,
                "tracker-improvement",
                Some("Still blocked dependent"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();

        for (from, to) in [
            (direct, root),
            (transitive, direct),
            (still_blocked, root),
            (still_blocked, extra_blocker),
        ] {
            store
                .add_edge(EdgeRecord {
                    from,
                    to,
                    kind: String::from("depends_on"),
                    created_at: chrono::Utc::now(),
                })
                .unwrap();
        }

        let response = get_json(
            app,
            format!("/api/workflow/unblocked-by?workspace={workspace}&root={root}"),
        )
        .await;

        assert_eq!(response["kind"], "unblocked-by");
        assert_eq!(response["root"]["id"], root.to_string());
        assert_eq!(response["reachable_dependents"], 3);
        assert_eq!(response["blocked_dependents"], 2);
        let children = response["root"]["children"].as_array().unwrap();
        assert_eq!(children[0]["id"], direct.to_string());
        let frontier = response["frontier_items"].as_array().unwrap();
        assert_eq!(frontier.len(), 2);
        assert_eq!(frontier[0]["id"], direct.to_string());
        assert_eq!(frontier[1]["id"], still_blocked.to_string());
    }
}