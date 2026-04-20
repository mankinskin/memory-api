use axum::{
    extract::{Extension, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use spec_api::SpecManifest;
use viewer_api::error::RequestIdExt;

use crate::error::spec_err;
use crate::state::SpecAppState;

// ── Query/Path extractors ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ListParams {
    pub state: Option<String>,
    pub component: Option<String>,
    pub query: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Deserialize)]
pub struct SearchParams {
    pub q: String,
    pub limit: Option<usize>,
}

// ── Response types ────────────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct SpecSummary {
    pub id: String,
    pub slug: Option<String>,
    pub title: Option<String>,
    pub state: Option<String>,
    pub component: Option<String>,
}

#[derive(Serialize)]
pub struct SpecListResponse {
    pub request_id: String,
    pub count: usize,
    pub items: Vec<SpecSummary>,
}

#[derive(Serialize)]
pub struct SpecDetailResponse {
    pub request_id: String,
    pub spec: SpecDetail,
}

#[derive(Serialize)]
pub struct SpecDetail {
    pub id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub fields: BTreeMap<String, Value>,
    pub code_refs: Vec<spec_api::code_ref::CodeRef>,
}

#[derive(Serialize)]
pub struct SpecFullResponse {
    pub request_id: String,
    pub spec: SpecDetail,
    pub body: String,
    pub sections: Vec<String>,
}

// ── Create request ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateSpecRequest {
    pub title: String,
    pub slug: String,
    pub component: String,
    pub parent: Option<String>,
    pub scope: Option<String>,
    pub body: Option<String>,
}

#[derive(Serialize)]
pub struct CreateSpecResponse {
    pub request_id: String,
    pub id: String,
    pub slug: String,
}

// ── Update request ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct UpdateSpecRequest {
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
    pub to_state: Option<String>,
    pub body: Option<String>,
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn spec_to_summary(spec: &SpecManifest) -> SpecSummary {
    SpecSummary {
        id: spec.id.to_string(),
        slug: spec.slug().map(str::to_string),
        title: spec.title().map(str::to_string),
        state: spec.state().map(str::to_string),
        component: spec.component().map(str::to_string),
    }
}

fn spec_to_detail(spec: &SpecManifest) -> SpecDetail {
    SpecDetail {
        id: spec.id.to_string(),
        created_at: spec.created_at,
        fields: spec.extra.clone(),
        code_refs: spec.code_refs.clone(),
    }
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn list_specs(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<ListParams>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);

    let all = match store.entity_store().list_indexed(false) {
        Ok(a) => a,
        Err(e) => return crate::error::storage_err(e, &rid.0),
    };

    let mut items = Vec::new();
    for indexed in &all {
        let spec = match store.get(&indexed.id.to_string()) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if let Some(ref st) = params.state {
            if spec.state().map(str::to_string).as_deref() != Some(st.as_str()) {
                continue;
            }
        }
        if let Some(ref comp) = params.component {
            if spec.component().map(str::to_string).as_deref() != Some(comp.as_str()) {
                continue;
            }
        }
        items.push(spec_to_summary(&spec));
        if let Some(limit) = params.limit {
            if items.len() >= limit {
                break;
            }
        }
    }

    Json(SpecListResponse {
        request_id: rid.0,
        count: items.len(),
        items,
    })
    .into_response()
}

pub async fn search_specs(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<SearchParams>,
) -> Response {
    let store = state.store.lock().await;
    let limit = params.limit.unwrap_or(20).min(100);
    match store.entity_store().search(&params.q, limit) {
        Ok(results) => {
            let items: Vec<SpecSummary> = results
                .iter()
                .map(|r| SpecSummary {
                    id: r.id.to_string(),
                    slug: None,
                    title: r.title.clone(),
                    state: r.state.clone(),
                    component: None,
                })
                .collect();
            Json(SpecListResponse {
                request_id: rid.0,
                count: items.len(),
                items,
            })
            .into_response()
        }
        Err(e) => crate::error::storage_err(e, &rid.0),
    }
}

/// GET /api/specs/:id — accepts UUID, UUID prefix, or slug.
pub async fn get_spec(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<String>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);
    match store.get(&id) {
        Ok(spec) => Json(SpecDetailResponse {
            request_id: rid.0,
            spec: spec_to_detail(&spec),
        })
        .into_response(),
        Err(e) => spec_err(e, &rid.0),
    }
}

/// GET /api/specs/:id/full — includes body and sections list.
pub async fn get_spec_full(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<String>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);
    let (spec, body) = match store.get_full(&id) {
        Ok(r) => r,
        Err(e) => return spec_err(e, &rid.0),
    };
    let sections = match store.list_sections(&id) {
        Ok(s) => s,
        Err(e) => return spec_err(e, &rid.0),
    };
    Json(SpecFullResponse {
        request_id: rid.0,
        spec: spec_to_detail(&spec),
        body,
        sections,
    })
    .into_response()
}

/// POST /api/specs — create a new spec.
pub async fn create_spec(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Json(req): Json<CreateSpecRequest>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);

    let mut manifest = SpecManifest::new(&req.slug, &req.title, &req.component);
    if let Some(parent) = &req.parent {
        match store.resolve_id(parent) {
            Ok(pid) => manifest.set_parent(&pid.to_string()),
            Err(e) => return spec_err(e, &rid.0),
        }
    }
    if let Some(scope) = &req.scope {
        manifest.set_scope(scope);
    }
    let body = req.body.as_deref().unwrap_or("");

    match store.create(&manifest, body, None) {
        Ok(id) => (
            StatusCode::CREATED,
            Json(CreateSpecResponse {
                request_id: rid.0,
                id: id.to_string(),
                slug: req.slug,
            }),
        )
            .into_response(),
        Err(e) => spec_err(e, &rid.0),
    }
}

/// PATCH /api/specs/:id — update fields, state, and/or body.
pub async fn update_spec(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSpecRequest>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);

    if let Some(body) = &req.body {
        if let Err(e) = store.update_body(&id, body) {
            return spec_err(e, &rid.0);
        }
    }

    match store.update(&id, req.fields, req.to_state.as_deref()) {
        Ok(spec) => Json(SpecDetailResponse {
            request_id: rid.0,
            spec: spec_to_detail(&spec),
        })
        .into_response(),
        Err(e) => spec_err(e, &rid.0),
    }
}

/// DELETE /api/specs/:id — soft-delete.
pub async fn delete_spec(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Path(id): Path<String>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);
    match store.delete(&id) {
        Ok(()) => Json(serde_json::json!({
            "request_id": rid.0,
            "status": "ok",
        }))
        .into_response(),
        Err(e) => spec_err(e, &rid.0),
    }
}
