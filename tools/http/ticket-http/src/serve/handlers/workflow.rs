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
    #[serde(default = "default_limit")]
    pub limit: usize,
}

#[derive(Deserialize)]
pub struct WorkflowTreeQuery {
    pub workspace: String,
    pub root: Uuid,
}

#[derive(Clone)]
struct NextScope {
    root: WorkflowRootSummary,
    reachable_dependents: usize,
    blocked_dependents: usize,
    remaining_blockers: HashSet<Uuid>,
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

#[derive(Serialize)]
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
    pub reachable_dependents: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_dependents: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_blocker_count: Option<usize>,
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

fn default_limit() -> usize {
    20
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
        let scope_root = params.root.map(|id| id.to_string());
        let scope_filter = params.filter.clone();
        let active_index_root = store.index_root.display().to_string();
        let tickets = match store.list(None, None, None) {
            Ok(tickets) => tickets,
            Err(error) => return storage_err(error, &request_id),
        };
        let all_edges = match store.list_all_edges() {
            Ok(edges) => edges,
            Err(error) => return storage_err(error, &request_id),
        };
        let model = match WorkflowModel::build(&store, tickets.clone(), all_edges) {
            Ok(model) => model,
            Err(error) => return storage_err(error, &request_id),
        };
        let board_snap = store.board_show(None).ok();

        if let Some(root_id) = params.root {
            if model.ticket(&root_id).is_none() {
                return ApiError::not_found("ticket", &request_id)
                    .into_response_with_status(StatusCode::NOT_FOUND);
            }
        }

        let filtered_scope =
            WorkflowModel::filter_scope(&tickets, params.filter.as_deref());
        let satisfied_ids = params.root.into_iter().collect::<HashSet<_>>();
        let next_scope = match params.root {
            Some(root_id) => match build_next_scope(
                root_id,
                &model,
                &store,
                &workspace,
                &satisfied_ids,
            ) {
                Ok(scope) => Some(scope),
                Err(error) => return storage_err(error, &request_id),
            },
            None => None,
        };

        let candidate_scope = intersect_scopes(
            filtered_scope,
            next_scope.as_ref().map(|scope| &scope.remaining_blockers),
        );
        let mut candidates = if satisfied_ids.is_empty() {
            model.actionable_candidate_ids(candidate_scope.as_ref())
        } else {
            model.actionable_candidate_ids_with_satisfied(
                candidate_scope.as_ref(),
                &satisfied_ids,
            )
        };
        model.sort_candidate_ids(&mut candidates);
        let board_filtered =
            apply_board_filter(candidates, board_snap.as_ref(), false);
        let mut candidates = board_filtered.candidates;
        candidates.truncate(params.limit.min(100));

        let items = match build_candidate_items(
            &candidates,
            &model,
            &store,
            &workspace,
            &satisfied_ids,
        ) {
            Ok(items) => items,
            Err(error) => return storage_err(error, &request_id),
        };

        Json(WorkflowNextResponse {
            request_id: request_id.clone(),
            active_workspace: workspace.clone(),
            workspace: workspace.clone(),
            scope: ScopeMetadata {
                workspace: workspace.clone(),
                active_index_root,
                filter: scope_filter,
                root: scope_root,
            },
            root: next_scope.as_ref().map(|scope| scope.root.clone()),
            reachable_dependents: next_scope
                .as_ref()
                .map(|scope| scope.reachable_dependents),
            blocked_dependents: next_scope
                .as_ref()
                .map(|scope| scope.blocked_dependents),
            remaining_blocker_count: next_scope
                .as_ref()
                .map(|scope| scope.remaining_blockers.len()),
            count: items.len(),
            items,
            excluded_by_board: board_filtered.excluded_by_board,
            warnings: board_filtered.warnings,
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "workflow next request"))
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
        let tickets = match store.list(None, None, None) {
            Ok(tickets) => tickets,
            Err(error) => return storage_err(error, &request_id),
        };
        let all_edges = match store.list_all_edges() {
            Ok(edges) => edges,
            Err(error) => return storage_err(error, &request_id),
        };
        let model = match WorkflowModel::build(&store, tickets, all_edges) {
            Ok(model) => model,
            Err(error) => return storage_err(error, &request_id),
        };

        if model.ticket(&params.root).is_none() {
            return ApiError::not_found("ticket", &request_id)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }

        let satisfied_ids = HashSet::from([params.root]);
        let (tree, frontier_ids, reachable_dependents, blocked_dependents, kind_label) = match kind {
            TreeKind::Blockers => {
                let Some(tree) = model.blocker_tree(params.root) else {
                    return ApiError::not_found("ticket", &request_id)
                        .into_response_with_status(StatusCode::NOT_FOUND);
                };
                let frontier_ids = tree.frontier_leaf_ids.clone();
                (tree, frontier_ids, None, None, "blockers")
            }
            TreeKind::UnblockedBy => {
                let Some(tree) = model.unlock_tree_with_satisfied(params.root, &satisfied_ids) else {
                    return ApiError::not_found("ticket", &request_id)
                        .into_response_with_status(StatusCode::NOT_FOUND);
                };
                let dependent_ids = model.reverse_dependents(params.root);
                let blocked_dependents = dependent_ids
                    .iter()
                    .filter(|ticket_id| {
                        !model
                            .unresolved_dependencies_excluding(ticket_id, &satisfied_ids)
                            .is_empty()
                    })
                    .count();
                (
                    tree,
                    model.unlock_frontier_leaf_ids_with_satisfied(
                        params.root,
                        &satisfied_ids,
                    ),
                    Some(dependent_ids.len()),
                    Some(blocked_dependents),
                    "unblocked-by",
                )
            }
        };

        let root = match build_tree_item(tree, &model, &store, &workspace) {
            Ok(root) => root,
            Err(error) => return storage_err(error, &request_id),
        };
        let empty_satisfied = HashSet::new();
        let frontier_items = match build_candidate_items(
            &frontier_ids,
            &model,
            &store,
            &workspace,
            if matches!(kind, TreeKind::UnblockedBy) {
                &satisfied_ids
            } else {
                &empty_satisfied
            },
        ) {
            Ok(items) => items,
            Err(error) => return storage_err(error, &request_id),
        };

        Json(WorkflowTreeResponse {
            request_id: request_id.clone(),
            active_workspace: workspace.clone(),
            workspace: workspace.clone(),
            kind: kind_label.to_string(),
            root,
            frontier_count: frontier_items.len(),
            frontier_items,
            reachable_dependents,
            blocked_dependents,
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "workflow tree request"))
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
    satisfied_ids: &HashSet<Uuid>,
) -> Result<NextScope, ticket_api::error::StorageError> {
    let dependent_ids = model.reverse_dependents(root_id);
    let blocked_dependents = dependent_ids
        .iter()
        .filter(|ticket_id| {
            !model
                .unresolved_dependencies_excluding(ticket_id, satisfied_ids)
                .is_empty()
        })
        .count();

    Ok(NextScope {
        root: build_root_summary(root_id, model, store, active_workspace)?,
        reachable_dependents: dependent_ids.len(),
        blocked_dependents,
        remaining_blockers: model.remaining_blockers_for_dependents_with_satisfied(
            &dependent_ids,
            satisfied_ids,
        ),
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
                Some("Scoped prerequisite"),
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
        let blocked_dependent = store
            .create(
                None,
                "tracker-improvement",
                Some("Blocked dependent"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .unwrap();
        for to in [root, scoped_blocker] {
            store
                .add_edge(EdgeRecord {
                    from: blocked_dependent,
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
        assert_eq!(scoped["reachable_dependents"], 1);
        assert_eq!(scoped["blocked_dependents"], 1);
        assert_eq!(scoped["remaining_blocker_count"], 1);
        assert_eq!(scoped["items"][0]["id"], scoped_blocker.to_string());
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