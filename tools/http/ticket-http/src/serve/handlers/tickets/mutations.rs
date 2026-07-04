use std::collections::BTreeMap;

use axum::{
    extract::{
        Extension,
        Path,
        Query,
        State,
    },
    http::{
        HeaderMap,
        StatusCode,
    },
    response::{
        IntoResponse,
        Json,
        Response,
    },
};
use serde_json::Value;
use uuid::Uuid;

use viewer_api::{
    auth::extract_bearer_token,
    error::{
        ApiError,
        RequestIdExt,
    },
};

use crate::serve::{
    AppState,
    error::{
        storage_err,
        task_join_err,
    },
};

use super::types::{
    CancelTicketBody,
    CloseTicketBody,
    CreateTicketBody,
    DeleteResponse,
    MoveTicketBody,
    MoveTicketResponse,
    MutationResponse,
    MutationWorkspaceParam,
    RevertTicketBody,
    TicketDetail,
    UpdateTicketBody,
    ticket_ref_for_id,
};

/// `POST /api/tickets?workspace=<name>`
///
/// Create a new ticket. Returns `201 Created` with the new ticket detail.
pub async fn create_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<MutationWorkspaceParam>,
    Json(body): Json<CreateTicketBody>,
) -> Response {
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };

    let extra = body.fields.unwrap_or_default();
    let type_id = body.type_id;
    let title = body.title;
    let description = body.description;
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let workspace = workspace.clone();
        let id = match store.create(
            None,
            &type_id,
            title.as_deref(),
            None,
            extra,
            None,
            description.as_deref(),
        ) {
            Ok(id) => id,
            Err(e) => return storage_err(e, &request_id),
        };

        let manifest = match store.get(&id) {
            Ok(manifest) => manifest,
            Err(e) => return storage_err(e, &request_id),
        };

        let created_at = indexed_created_at(&store, &id);
        let ticket_ref = match ticket_ref_for_id(&store, &workspace, &id) {
            Ok(ticket_ref) => ticket_ref,
            Err(e) => return storage_err(e, &request_id),
        };

        (
            StatusCode::CREATED,
            Json(MutationResponse {
                request_id,
                active_workspace: workspace.clone(),
                workspace,
                ticket: TicketDetail {
                    id: manifest.id.to_string(),
                    ticket_ref,
                    created_at,
                    fields: manifest.extra,
                },
            }),
        )
            .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket create request"))
}

/// `PATCH /api/tickets/{id}?workspace=<name>`
///
/// Update fields, state, or description of an existing ticket.
pub async fn update_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<MutationWorkspaceParam>,
    headers: HeaderMap,
    Json(body): Json<UpdateTicketBody>,
) -> Response {
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };

    let patch = body.fields.unwrap_or_default();
    let transition_states = body.transition_states;
    let to_state = body.state;
    let description = body.description;
    let author = author_from_headers(&headers);
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let workspace = workspace.clone();
        let manifest = match store.update(
            &id,
            patch,
            Some(transition_states.as_slice()),
            to_state.as_deref(),
            description.as_deref(),
            author.as_deref(),
        ) {
            Ok(manifest) => manifest,
            Err(e) => return storage_err(e, &request_id),
        };

        let ticket_ref = match ticket_ref_for_id(&store, &workspace, &id) {
            Ok(ticket_ref) => ticket_ref,
            Err(e) => return storage_err(e, &request_id),
        };

        Json(MutationResponse {
            request_id,
            active_workspace: workspace.clone(),
            workspace,
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                ticket_ref,
                created_at: indexed_created_at(&store, &id),
                fields: manifest.extra,
            },
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket update request"))
}

/// `POST /api/tickets/{id}/close?workspace=<name>`
///
/// Fast-forward a ticket through all intermediate states to the target terminal
/// state. `target_state` defaults to `"done"`.
pub async fn close_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<MutationWorkspaceParam>,
    headers: HeaderMap,
    Json(body): Json<CloseTicketBody>,
) -> Response {
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };

    let target = body.target_state.as_deref().unwrap_or("done").to_string();
    let author = author_from_headers(&headers);
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let (manifest, _path) =
            match store.close(&id, &target, author.as_deref()) {
                Ok(result) => result,
                Err(e) => return storage_err(e, &request_id),
            };

        Json(MutationResponse {
            request_id: request_id.clone(),
            active_workspace: workspace.clone(),
            workspace: workspace.clone(),
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                ticket_ref: match ticket_ref_for_id(&store, &workspace, &id) {
                    Ok(ticket_ref) => ticket_ref,
                    Err(e) => return storage_err(e, &request_id),
                },
                created_at: indexed_created_at(&store, &id),
                fields: manifest.extra,
            },
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket close request"))
}

/// `POST /api/tickets/{id}/cancel?workspace=<name>`
///
/// Transition a ticket to the `cancelled` state. Optional `reason` field is
/// stored as a ticket field update.
pub async fn cancel_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<MutationWorkspaceParam>,
    headers: HeaderMap,
    Json(body): Json<CancelTicketBody>,
) -> Response {
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };

    let author = author_from_headers(&headers);
    let mut patch = BTreeMap::new();
    if let Some(reason) = body.reason {
        patch.insert("cancel_reason".to_string(), Value::String(reason));
    }
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let manifest = match store.update(
            &id,
            patch,
            Some(&[]),
            Some("cancelled"),
            None,
            author.as_deref(),
        ) {
            Ok(manifest) => manifest,
            Err(e) => return storage_err(e, &request_id),
        };

        Json(MutationResponse {
            request_id: request_id.clone(),
            active_workspace: workspace.clone(),
            workspace: workspace.clone(),
            ticket: TicketDetail {
                id: manifest.id.to_string(),
                ticket_ref: match ticket_ref_for_id(&store, &workspace, &id) {
                    Ok(ticket_ref) => ticket_ref,
                    Err(e) => return storage_err(e, &request_id),
                },
                created_at: indexed_created_at(&store, &id),
                fields: manifest.extra,
            },
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket cancel request"))
}

/// `POST /api/tickets/{id}/move?workspace=<name>`
///
/// Dry-run or apply a cross-workspace ticket move using the storage-layer
/// planner and journaled execution primitive.
pub async fn move_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<MutationWorkspaceParam>,
    Json(body): Json<MoveTicketBody>,
) -> Response {
    let span = tracing::info_span!(
        target: "ticket_http::transport",
        "ticket_http_move_request",
        request_id = %rid.0,
        ticket_id = %id,
        requested_workspace = %params.workspace,
        dry_run = body.dry_run,
    );
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };

    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();
    let target_workspace = normalize_workspace_root(&body.to_workspace_root);
    let dry_run = body.dry_run;

    tracing::debug!(
        target: "ticket_http::transport",
        parent: &span,
        request_id = %request_id,
        workspace = %workspace,
        target_workspace_root = %target_workspace.display(),
        "ticket_http_move_resolved"
    );

    tokio::task::spawn_blocking(move || {
        let span = tracing::info_span!(
            target: "ticket_http::transport",
            "ticket_http_move_execute",
            request_id = %task_request_id,
            ticket_id = %id,
            workspace = %workspace,
            dry_run,
            journal_id = tracing::field::Empty,
        );
        let request_id = task_request_id.clone();
        let plan = match store.plan_move_preflight(&id, &target_workspace) {
            Ok(report) => report,
            Err(error) => return storage_err(error, &request_id),
        };

        if dry_run || !plan.supported() {
            tracing::info!(
                target: "ticket_http::transport",
                parent: &span,
                request_id = %request_id,
                ticket_id = %id,
                mode = "plan",
                supported = plan.supported(),
                blockers = plan.blockers.len(),
                "ticket_http_move_complete"
            );
            let status = if plan.supported() {
                StatusCode::OK
            } else {
                StatusCode::CONFLICT
            };
            return (
                status,
                Json(MoveTicketResponse {
                    request_id,
                    active_workspace: workspace.clone(),
                    workspace: workspace.clone(),
                    id: id.to_string(),
                    status: if plan.supported() {
                        "ok".to_string()
                    } else {
                        "blocked".to_string()
                    },
                    mode: "plan".to_string(),
                    plan: move_plan_json(&plan),
                    outcome: None,
                    recovery: move_recovery_json(),
                }),
            )
                .into_response();
        }

        let outcome = match store.execute_move_with_journal(&plan) {
            Ok(outcome) => outcome,
            Err(error) => return storage_err(error, &request_id),
        };
        span.record("journal_id", outcome.journal.id.to_string());
        tracing::info!(
            target: "ticket_http::transport",
            parent: &span,
            request_id = %request_id,
            ticket_id = %id,
            journal_id = %outcome.journal.id,
            mode = "apply",
            phase = ?outcome.journal.phase,
            resumed = outcome.resumed,
            rolled_back = outcome.rolled_back,
            "ticket_http_move_complete"
        );

        Json(MoveTicketResponse {
            request_id,
            active_workspace: workspace.clone(),
            workspace: workspace.clone(),
            id: id.to_string(),
            status: "ok".to_string(),
            mode: "apply".to_string(),
            plan: move_plan_json(&plan),
            outcome: Some(move_outcome_json(&outcome)),
            recovery: move_recovery_json(),
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket move request"))
}

/// `POST /api/tickets/{id}/revert?workspace=<name>`
///
/// Revert a ticket to a specific historical revision, identified by its
/// 1-based `revision` number. The revert is forward-only: a new history entry
/// is appended; no history is erased.
pub async fn revert_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<MutationWorkspaceParam>,
    headers: HeaderMap,
    Json(body): Json<RevertTicketBody>,
) -> Response {
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };

    let revision = body.revision;
    let author = author_from_headers(&headers);
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let revisions = match store.get_history(&id) {
            Ok(revisions) => revisions,
            Err(e) => return storage_err(e, &request_id),
        };

        let target_rev = match revisions
            .iter()
            .find(|revision_entry| revision_entry.rev == revision)
        {
            Some(revision_entry) => revision_entry.clone(),
            None => {
                return ApiError::bad_request(
                    "revision_not_found",
                    &format!(
                        "revision {} does not exist for this ticket",
                        revision
                    ),
                    &request_id,
                )
                .into_response_with_status(StatusCode::BAD_REQUEST);
            },
        };

        match store.apply_revert(&id, target_rev.fields, author.as_deref()) {
            Ok(_new_rev) => current_ticket_response(
                &store,
                &request_id,
                &workspace,
                &id,
            ),
            Err(e) => storage_err(e, &request_id),
        }
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket revert request"))
}

/// `POST /api/tickets/{id}/undo?workspace=<name>`
///
/// Undo the last state/field transition on a ticket by reverting to the
/// second-to-last history revision, bypassing state-machine validation.
pub async fn undo_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<MutationWorkspaceParam>,
    headers: HeaderMap,
) -> Response {
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };

    let author = author_from_headers(&headers);
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let revisions = match store.get_history(&id) {
            Ok(revisions) => revisions,
            Err(e) => return storage_err(e, &request_id),
        };

        if revisions.len() < 2 {
            return ApiError::bad_request(
                "no_previous_revision",
                "ticket has no previous revision to undo",
                &request_id,
            )
            .into_response_with_status(StatusCode::UNPROCESSABLE_ENTITY);
        }

        let prev_fields = revisions[revisions.len() - 2].fields.clone();

        match store.apply_revert(&id, prev_fields, author.as_deref()) {
            Ok(_new_rev) => current_ticket_response(
                &store,
                &request_id,
                &workspace,
                &id,
            ),
            Err(e) => storage_err(e, &request_id),
        }
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket undo request"))
}

/// `DELETE /api/tickets/{id}?workspace=<name>`
///
/// Delete a ticket permanently, removing its folder from disk. Emits a `ticket.delete` SSE event.
pub async fn delete_ticket(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<MutationWorkspaceParam>,
) -> Response {
    let (workspace, store) =
        match state.resolve_public_workspace_request(&params.workspace, &rid.0)
        {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        // Capture ref before deletion while entity still exists in index.
        let ticket_ref = match ticket_ref_for_id(&store, &workspace, &id) {
            Ok(ticket_ref) => ticket_ref,
            Err(e) => return storage_err(e, &task_request_id),
        };
        match store.delete(&id) {
            Ok(()) => {
                Json(DeleteResponse {
                    request_id: task_request_id.clone(),
                    active_workspace: workspace.clone(),
                    workspace: workspace.clone(),
                    id: id.to_string(),
                    ticket_ref,
                })
                .into_response()
            },
            Err(e) => storage_err(e, &task_request_id),
        }
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket delete request"))
}

fn author_from_headers(headers: &HeaderMap) -> Option<String> {
    extract_bearer_token(headers).map(str::to_string)
}

fn indexed_created_at(
    store: &ticket_api::storage::store::TicketStore,
    id: &Uuid,
) -> chrono::DateTime<chrono::Utc> {
    store
        .get_indexed(id)
        .ok()
        .flatten()
        .map(|ticket| ticket.created_at)
        .unwrap_or_else(chrono::Utc::now)
}

fn current_ticket_response(
    store: &ticket_api::storage::store::TicketStore,
    request_id: &str,
    workspace: &str,
    id: &Uuid,
) -> Response {
    let manifest = match store.get(id) {
        Ok(manifest) => manifest,
        Err(e) => return storage_err(e, request_id),
    };

    Json(MutationResponse {
        request_id: request_id.to_string(),
        active_workspace: workspace.to_string(),
        workspace: workspace.to_string(),
        ticket: TicketDetail {
            id: manifest.id.to_string(),
            ticket_ref: match ticket_ref_for_id(store, workspace, id) {
                Ok(ticket_ref) => ticket_ref,
                Err(e) => return storage_err(e, request_id),
            },
            created_at: indexed_created_at(store, id),
            fields: manifest.extra,
        },
    })
    .into_response()
}

fn move_plan_json(report: &ticket_api::storage::move_planner::MovePreflightReport) -> Value {
    serde_json::json!({
        "supported": report.supported(),
        "source_workspace_root": normalize_display_path(&report.source_workspace_root),
        "target_workspace_root": normalize_display_path(&report.target_workspace_root),
        "source_store_root": normalize_display_path(&report.source_store_root),
        "target_store_root": normalize_display_path(&report.target_store_root),
        "source_ticket_path": normalize_display_path(&report.source_entity_path),
        "destination_ticket_path": normalize_display_path(&report.destination_entity_path),
        "path_reference_files": report.path_reference_files.iter().map(|p| normalize_display_path(p)).collect::<Vec<_>>(),
        "reference_visibility": report.reference_visibility,
        "active_board_entries": report.active_board_entries,
        "historical_board_entries": report.historical_board_entries,
        "active_leases": report.active_leases,
        "blockers": report.blockers,
        "captured_at": report.captured_at,
    })
}

fn move_outcome_json(outcome: &ticket_api::storage::move_execution::MoveExecutionOutcome) -> Value {
    serde_json::json!({
        "resumed": outcome.resumed,
        "rolled_back": outcome.rolled_back,
        "journal": {
            "id": outcome.journal.id,
            "ticket_id": outcome.journal.entity_id,
            "phase": outcome.journal.phase,
            "steps": outcome.journal.steps,
            "rollback_steps": outcome.journal.rollback_steps,
            "failure": outcome.journal.failure,
            "next_recovery_step": outcome.journal.next_recovery_step,
            "rewritten_path_files": outcome.journal.rewritten_path_files,
            "manual_followups": outcome.journal.manual_followups,
            "migrated_board_entries": outcome.journal.migrated_board_entries,
            "created_at": outcome.journal.created_at,
            "updated_at": outcome.journal.updated_at,
        }
    })
}

fn move_recovery_json() -> Value {
    serde_json::json!({
        "resume": "ticket move --resume <journal-uuid>",
        "rollback": "ticket move --rollback <journal-uuid>",
    })
}

fn normalize_workspace_root(value: &str) -> std::path::PathBuf {
    let path = std::path::PathBuf::from(value);
    std::fs::canonicalize(&path).unwrap_or(path)
}

fn normalize_display_path(path: &std::path::Path) -> String {
    ticket_api::workspace::normalize_path_for_display(path)
}
