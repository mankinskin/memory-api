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
use uuid::Uuid;

use ticket_api::storage::ticket_fs::{
    LoadedPart,
    TicketFs,
};

use viewer_api::error::RequestIdExt;

use crate::serve::{
    AppState,
    error::{
        storage_err,
        task_join_err,
    },
};

use super::{
    mutations::author_from_headers,
    types::{
        ListPartsParam,
        ListPartsResponse,
        PartItem,
        PartResponse,
        TicketIdParam,
        WriteAmendmentBody,
        WritePartBody,
        ticket_ref_for_id,
    },
};

fn part_item(
    part: &LoadedPart,
    with_content: bool,
) -> PartItem {
    PartItem {
        id: part.id,
        kind: part.kind.clone(),
        path: part.path.clone(),
        frozen: part.frozen,
        created_at: part.created_at,
        supersedes: part.supersedes,
        implicit: part.implicit,
        content: with_content.then(|| part.content.clone()),
    }
}

/// `GET /api/tickets/{id}/parts?workspace=<name>&with_content=<bool>`
///
/// List a ticket's content parts, including frozen state and any orphaned
/// part files (reported separately, never silently adopted).
pub async fn list_parts(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<ListPartsParam>,
) -> Response {
    let (workspace, store) = match state
        .resolve_public_workspace_request(&params.workspace, &rid.0)
    {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };
    let with_content = params.with_content;
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let indexed = match store.get_indexed(&id) {
            Ok(Some(indexed)) => indexed,
            Ok(None) =>
                return storage_err(
                    ticket_api::error::StorageError::NotFound(id),
                    &request_id,
                ),
            Err(e) => return storage_err(e, &request_id),
        };
        let manifest = match TicketFs::read(&indexed.path) {
            Ok(manifest) => manifest,
            Err(e) => return storage_err(e, &request_id),
        };
        let report = match TicketFs::load_parts(&indexed.path, &manifest) {
            Ok(report) => report,
            Err(e) => return storage_err(e, &request_id),
        };
        let ticket_ref = match ticket_ref_for_id(&store, &workspace, &id) {
            Ok(ticket_ref) => ticket_ref,
            Err(e) => return storage_err(e, &request_id),
        };

        let parts: Vec<PartItem> = report
            .parts
            .iter()
            .map(|part| part_item(part, with_content))
            .collect();
        let orphans: Vec<String> = report
            .orphans
            .iter()
            .map(|path| path.display().to_string())
            .collect();

        Json(ListPartsResponse {
            request_id,
            active_workspace: workspace.clone(),
            workspace,
            id: id.to_string(),
            ticket_ref,
            count: parts.len(),
            parts,
            orphans,
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket list-parts request"))
}

/// `GET /api/tickets/{id}/parts/{part_id}?workspace=<name>`
///
/// Get a single ticket content part, addressed by its opaque part id.
pub async fn get_part(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path((id, part_id)): Path<(Uuid, Uuid)>,
    Query(params): Query<TicketIdParam>,
) -> Response {
    let (workspace, store) = match state
        .resolve_public_workspace_request(&params.workspace, &rid.0)
    {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        let indexed = match store.get_indexed(&id) {
            Ok(Some(indexed)) => indexed,
            Ok(None) =>
                return storage_err(
                    ticket_api::error::StorageError::NotFound(id),
                    &request_id,
                ),
            Err(e) => return storage_err(e, &request_id),
        };
        let manifest = match TicketFs::read(&indexed.path) {
            Ok(manifest) => manifest,
            Err(e) => return storage_err(e, &request_id),
        };
        let report = match TicketFs::load_parts(&indexed.path, &manifest) {
            Ok(report) => report,
            Err(e) => return storage_err(e, &request_id),
        };
        let Some(part) = report.find(part_id) else {
            return storage_err(
                ticket_api::error::StorageError::Other(format!(
                    "part '{part_id}' was not found on ticket '{id}'"
                )),
                &request_id,
            );
        };
        let ticket_ref = match ticket_ref_for_id(&store, &workspace, &id) {
            Ok(ticket_ref) => ticket_ref,
            Err(e) => return storage_err(e, &request_id),
        };

        Json(PartResponse {
            request_id,
            active_workspace: workspace.clone(),
            workspace,
            id: id.to_string(),
            ticket_ref,
            part: part_item(part, true),
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket get-part request"))
}

/// `POST /api/tickets/{id}/parts?workspace=<name>`
///
/// Write a ticket content part: updates an existing part via `part_id`, or
/// creates a new part of `kind`. Routed through `TicketStore::write_part`,
/// so the frozen-part write gate always applies; a rejected write surfaces
/// the full `FrozenPartWrite` error text (see `serve::error::storage_err`).
pub async fn write_part(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<TicketIdParam>,
    headers: HeaderMap,
    Json(body): Json<WritePartBody>,
) -> Response {
    let (workspace, store) = match state
        .resolve_public_workspace_request(&params.workspace, &rid.0)
    {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };
    let author = author_from_headers(&headers);
    let part_id = body.part_id.unwrap_or_else(Uuid::new_v4);
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        match store.write_part(
            &id,
            part_id,
            &body.kind,
            &body.content,
            author.as_deref(),
        ) {
            Ok(_manifest) => {},
            Err(e) => return storage_err(e, &request_id),
        }

        respond_with_part(&store, &workspace, &request_id, id, part_id)
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket write-part request"))
}

/// `POST /api/tickets/{id}/parts/amendment?workspace=<name>`
///
/// Write an `amendment` part that supersedes another (typically frozen)
/// part, recording a correction without unfreezing the original.
pub async fn write_amendment(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<Uuid>,
    Query(params): Query<TicketIdParam>,
    headers: HeaderMap,
    Json(body): Json<WriteAmendmentBody>,
) -> Response {
    let (workspace, store) = match state
        .resolve_public_workspace_request(&params.workspace, &rid.0)
    {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };
    let author = author_from_headers(&headers);
    let part_id = body.part_id.unwrap_or_else(Uuid::new_v4);
    let supersedes = body.supersedes;
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        match store.write_amendment_part(
            &id,
            part_id,
            &body.content,
            supersedes,
            author.as_deref(),
        ) {
            Ok(_manifest) => {},
            Err(e) => return storage_err(e, &request_id),
        }

        respond_with_part(&store, &workspace, &request_id, id, part_id)
    })
    .await
    .unwrap_or_else(|_| {
        task_join_err(&request_id, "ticket write-amendment request")
    })
}

/// `POST /api/tickets/{id}/parts/{part_id}/undo?workspace=<name>`
///
/// Restore a part to the content it held immediately before its most recent
/// write. Rejected (with the full `FrozenPartWrite` text) if the part is
/// currently frozen — undo is a write, not a privileged bypass of the gate.
pub async fn undo_part(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path((id, part_id)): Path<(Uuid, Uuid)>,
    Query(params): Query<TicketIdParam>,
    headers: HeaderMap,
) -> Response {
    let (workspace, store) = match state
        .resolve_public_workspace_request(&params.workspace, &rid.0)
    {
        Ok(resolved) => resolved,
        Err(response) => return response,
    };
    let author = author_from_headers(&headers);
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let request_id = task_request_id.clone();
        match store.undo_part(&id, part_id, author.as_deref()) {
            Ok(_manifest) => {},
            Err(e) => return storage_err(e, &request_id),
        }

        respond_with_part(&store, &workspace, &request_id, id, part_id)
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "ticket undo-part request"))
}

fn respond_with_part(
    store: &ticket_api::storage::store::TicketStore,
    workspace: &str,
    request_id: &str,
    id: Uuid,
    part_id: Uuid,
) -> Response {
    let indexed = match store.get_indexed(&id) {
        Ok(Some(indexed)) => indexed,
        Ok(None) =>
            return storage_err(
                ticket_api::error::StorageError::NotFound(id),
                request_id,
            ),
        Err(e) => return storage_err(e, request_id),
    };
    let manifest = match TicketFs::read(&indexed.path) {
        Ok(manifest) => manifest,
        Err(e) => return storage_err(e, request_id),
    };
    let report = match TicketFs::load_parts(&indexed.path, &manifest) {
        Ok(report) => report,
        Err(e) => return storage_err(e, request_id),
    };
    let Some(part) = report.find(part_id) else {
        return storage_err(
            ticket_api::error::StorageError::Other(format!(
                "part '{part_id}' was not found on ticket '{id}' after write"
            )),
            request_id,
        );
    };
    let ticket_ref = match ticket_ref_for_id(store, workspace, &id) {
        Ok(ticket_ref) => ticket_ref,
        Err(e) => return storage_err(e, request_id),
    };

    (
        StatusCode::OK,
        Json(PartResponse {
            request_id: request_id.to_string(),
            active_workspace: workspace.to_string(),
            workspace: workspace.to_string(),
            id: id.to_string(),
            ticket_ref,
            part: part_item(part, true),
        }),
    )
        .into_response()
}
