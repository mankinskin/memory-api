use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};

use viewer_api::error::{ApiError, RequestIdExt};

use crate::error::spec_err;
use crate::state::SpecAppState;

#[derive(Deserialize)]
pub struct AddSectionRequest {
    pub name: String,
    pub content: String,
}

#[derive(Serialize)]
pub struct SectionsResponse {
    pub request_id: String,
    pub spec: String,
    pub count: usize,
    pub sections: Vec<String>,
}

/// GET /api/specs/:id/sections
pub async fn list_sections(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<String>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);
    match store.list_sections(&id) {
        Ok(sections) => Json(SectionsResponse {
            request_id: rid.0,
            spec: id,
            count: sections.len(),
            sections,
        })
        .into_response(),
        Err(e) => spec_err(e, &rid.0),
    }
}

/// GET /api/specs/:id/sections/:name
pub async fn get_section(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path((id, name)): Path<(String, String)>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);

    let uuid = match store.resolve_id(&id) {
        Ok(u) => u,
        Err(e) => return spec_err(e, &rid.0),
    };
    let indexed = match store.entity_store().get_indexed(&uuid) {
        Ok(Some(i)) => i,
        Ok(None) => {
            return ApiError::not_found("spec", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
        Err(e) => return crate::error::storage_err(e, &rid.0),
    };
    let file_name = if name.ends_with(".md") {
        name.clone()
    } else {
        format!("{name}.md")
    };
    let path = indexed.path.join("sections").join(&file_name);
    match std::fs::read_to_string(&path) {
        Ok(content) => Json(serde_json::json!({
            "request_id": rid.0,
            "spec": id,
            "section": name,
            "content": content,
        }))
        .into_response(),
        Err(_) => ApiError::not_found("section", &rid.0)
            .into_response_with_status(StatusCode::NOT_FOUND),
    }
}

/// POST /api/specs/:id/sections
pub async fn add_section(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<String>,
    Json(req): Json<AddSectionRequest>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);
    match store.add_section(&id, &req.name, &req.content) {
        Ok(()) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "request_id": rid.0,
                "spec": id,
                "section": req.name,
                "status": "ok",
            })),
        )
            .into_response(),
        Err(e) => spec_err(e, &rid.0),
    }
}

/// DELETE /api/specs/:id/sections/:name
pub async fn delete_section(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path((id, name)): Path<(String, String)>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);
    match store.delete_section(&id, &name) {
        Ok(()) => Json(serde_json::json!({
            "request_id": rid.0,
            "spec": id,
            "section": name,
            "status": "ok",
        }))
        .into_response(),
        Err(e) => spec_err(e, &rid.0),
    }
}
