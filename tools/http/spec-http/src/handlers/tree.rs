use std::path::PathBuf;

use axum::{
    extract::{Extension, Path, State},
    response::{IntoResponse, Json, Response},
};

use viewer_api::error::RequestIdExt;

use crate::error::spec_err;
use crate::state::SpecAppState;

/// GET /api/specs/:id/tree — hierarchy subtree.
pub async fn get_tree(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<String>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);

    let root = match store.get(&id) {
        Ok(s) => s,
        Err(e) => return spec_err(e, &rid.0),
    };
    let descendants = match store.subtree(&id) {
        Ok(d) => d,
        Err(e) => return spec_err(e, &rid.0),
    };

    Json(serde_json::json!({
        "request_id": rid.0,
        "root": {
            "id": root.id.to_string(),
            "slug": root.slug(),
            "title": root.title(),
            "state": root.state(),
        },
        "descendants": descendants.iter().map(|c| serde_json::json!({
            "id": c.id.to_string(),
            "slug": c.slug(),
            "title": c.title(),
            "state": c.state(),
            "parent": c.parent(),
        })).collect::<Vec<_>>(),
    }))
    .into_response()
}

/// GET /api/specs/:id/refs — list code references.
pub async fn get_refs(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<String>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);
    match store.get(&id) {
        Ok(spec) => Json(serde_json::json!({
            "request_id": rid.0,
            "id": spec.id.to_string(),
            "count": spec.code_refs.len(),
            "refs": spec.code_refs,
        }))
        .into_response(),
        Err(e) => spec_err(e, &rid.0),
    }
}

#[derive(serde::Deserialize)]
pub struct ValidateRefsRequest {
    #[serde(default = "default_workspace_root")]
    pub workspace_root: String,
}

fn default_workspace_root() -> String {
    ".".to_string()
}

/// POST /api/specs/:id/refs/validate — validate code references.
pub async fn validate_refs(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<ValidateRefsRequest>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);
    let spec = match store.get(&id) {
        Ok(s) => s,
        Err(e) => return spec_err(e, &rid.0),
    };
    let workspace_root = PathBuf::from(&body.workspace_root);
    let results = spec_api::code_ref::validate_refs(&spec.code_refs, &workspace_root);
    let all_valid = results.iter().all(|r| r.file_exists && r.line_range_valid);
    let items: Vec<serde_json::Value> = results
        .iter()
        .map(|r| {
            serde_json::json!({
                "file": r.code_ref.file,
                "symbol": r.code_ref.symbol,
                "kind": format!("{:?}", r.code_ref.kind),
                "file_exists": r.file_exists,
                "line_range_valid": r.line_range_valid,
                "message": r.message,
            })
        })
        .collect();

    Json(serde_json::json!({
        "request_id": rid.0,
        "id": spec.id.to_string(),
        "valid": all_valid,
        "count": items.len(),
        "results": items,
    }))
    .into_response()
}
