use std::path::PathBuf;

use axum::{
    extract::{Extension, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::Deserialize;

use viewer_api::error::{ApiError, RequestIdExt};

use crate::error::{spec_err, storage_err};
use crate::state::SpecAppState;

/// GET /healthz
pub async fn healthz() -> &'static str {
    "ok"
}

#[derive(Deserialize)]
pub struct HealthParams {
    pub id: Option<String>,
    #[serde(default)]
    pub all: bool,
}

/// GET /api/specs/health
pub async fn health_check(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<HealthParams>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);

    let specs = if params.all {
        match store.entity_store().list_indexed(false) {
            Ok(all) => all
                .iter()
                .filter_map(|e| store.get(&e.id.to_string()).ok())
                .collect::<Vec<_>>(),
            Err(e) => return storage_err(e, &rid.0),
        }
    } else if let Some(id) = &params.id {
        match store.get(id) {
            Ok(s) => vec![s],
            Err(e) => return spec_err(e, &rid.0),
        }
    } else {
        return ApiError::new("spec.missing_param", "provide id or all=true", &rid.0)
            .into_response_with_status(StatusCode::BAD_REQUEST);
    };

    let mut issues = Vec::new();
    for spec in &specs {
        if spec.slug().is_none() {
            issues.push(serde_json::json!({"id": spec.id.to_string(), "issue": "missing slug"}));
        }
        if spec.title().is_none() {
            issues.push(serde_json::json!({"id": spec.id.to_string(), "issue": "missing title"}));
        }
        if spec.component().is_none() {
            issues.push(
                serde_json::json!({"id": spec.id.to_string(), "issue": "missing component"}),
            );
        }
    }

    Json(serde_json::json!({
        "request_id": rid.0,
        "specs_checked": specs.len(),
        "issues_count": issues.len(),
        "issues": issues,
    }))
    .into_response()
}

#[derive(Deserialize)]
pub struct ScanParams {
    #[serde(default)]
    pub force: bool,
}

/// POST /api/specs/scan
pub async fn scan(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<ScanParams>,
) -> Response {
    let mut store = state.store.lock().await;
    match store.scan(params.force) {
        Ok(report) => Json(serde_json::json!({
            "request_id": rid.0,
            "status": "ok",
            "force": params.force,
            "integrated": report.integrated,
            "pruned": report.pruned,
            "diagnostics_count": report.diagnostics.len(),
        }))
        .into_response(),
        Err(e) => spec_err(e, &rid.0),
    }
}

#[derive(Deserialize)]
pub struct AddRootRequest {
    pub path: String,
    pub label: Option<String>,
}

/// POST /api/specs/add-root
pub async fn add_root(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Json(req): Json<AddRootRequest>,
) -> Response {
    let store = state.store.lock().await;
    let path = PathBuf::from(&req.path);
    let label = req.label.unwrap_or_else(|| {
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("specs")
            .to_string()
    });
    match store.entity_store().add_scan_root(memory_api::model::filesystem::ScanRoot {
        path: path.clone(),
        label: label.clone(),
    }) {
        Ok(()) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "request_id": rid.0,
                "status": "ok",
                "path": path,
                "label": label,
            })),
        )
            .into_response(),
        Err(e) => storage_err(e, &rid.0),
    }
}
