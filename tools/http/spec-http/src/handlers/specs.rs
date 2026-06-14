use axum::{
    extract::{
        Extension,
        Path,
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
use serde_json::Value;
use std::collections::BTreeMap;

use spec_api::SpecManifest;
use viewer_api::error::RequestIdExt;

use crate::{
    error::spec_err,
    state::SpecAppState,
};

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
    #[serde(default)]
    pub fields: BTreeMap<String, Value>,
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

fn matches_query(
    spec: &SpecManifest,
    query: &str,
) -> bool {
    let needle = query.trim().to_lowercase();
    if needle.is_empty() {
        return true;
    }

    [spec.title(), spec.slug(), spec.component(), spec.state()]
        .into_iter()
        .flatten()
        .any(|field| field.to_lowercase().contains(&needle))
}

fn matches_list_params(
    spec: &SpecManifest,
    params: &ListParams,
) -> bool {
    if let Some(state) = params.state.as_deref() {
        if spec.state() != Some(state) {
            return false;
        }
    }

    if let Some(component) = params.component.as_deref() {
        if spec.component() != Some(component) {
            return false;
        }
    }

    params
        .query
        .as_deref()
        .is_none_or(|query| matches_query(spec, query))
}

// ── Handlers ──────────────────────────────────────────────────────────────────

pub async fn list_specs(
    State(state): State<SpecAppState>,
    Extension(rid): Extension<RequestIdExt>,
    Query(params): Query<ListParams>,
) -> Response {
    let mut store = state.store.lock().await;
    let _ = store.scan(false);

    let all = match store.entity_store().list_indexed() {
        Ok(a) => a,
        Err(e) => return crate::error::storage_err(e, &rid.0),
    };

    let limit = params.limit.unwrap_or(usize::MAX);
    let items: Vec<SpecSummary> = all
        .iter()
        .filter_map(|indexed| store.get(&indexed.id.to_string()).ok())
        .filter(|spec| matches_list_params(spec, &params))
        .map(|spec| spec_to_summary(&spec))
        .take(limit)
        .collect();

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
        },
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
    manifest.extra.extend(req.fields.clone());
    manifest.set_slug(&req.slug);
    manifest.set_title(&req.title);
    manifest.set_component(&req.component);
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

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs,
        path::PathBuf,
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
    use serde::Deserialize;
    use serde_json::json;
    use spec_api::{
        SpecManifest,
        SpecStore,
    };
    use tempfile::tempdir;
    use tower::ServiceExt;

    use super::*;
    use crate::{
        build_router,
        state::SpecAppState,
    };

    #[derive(Debug, Deserialize)]
    struct ContractParityFixture {
        fields: BTreeMap<String, Value>,
        fulfillment_update: BTreeMap<String, Value>,
        search_query: String,
        expected_health_issue: String,
    }

    fn load_contract_parity_fixture() -> ContractParityFixture {
        let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../test-fixtures/spec-contract-parity.json");
        serde_json::from_str(&fs::read_to_string(fixture_path).unwrap())
            .unwrap()
    }

    async fn response_json(response: axum::response::Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn create_test_app() -> (tempfile::TempDir, axum::Router) {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        fs::create_dir_all(repo.join(".spec")).unwrap();
        let store = SpecStore::init(&repo.join(".spec")).unwrap();
        (dir, build_router(SpecAppState::new(store)))
    }

    #[test]
    fn matches_query_checks_title_slug_component_and_state() {
        let mut spec = SpecManifest::new(
            "context-stack/graph-induction",
            "Graph Induction",
            "context-stack",
        );
        spec.set_state("draft");

        assert!(matches_query(&spec, "graph"));
        assert!(matches_query(&spec, "context-stack"));
        assert!(matches_query(&spec, "draft"));
        assert!(!matches_query(&spec, "viewer-api"));
    }

    #[test]
    fn matches_query_treats_blank_input_as_match_all() {
        let spec =
            SpecManifest::new("spec-viewer", "Spec Viewer", "spec-viewer");

        assert!(matches_query(&spec, ""));
        assert!(matches_query(&spec, "   "));
    }

    #[tokio::test]
    async fn http_routes_round_trip_structured_contract_fields() {
        let (_dir, app) = create_test_app();
        let fixture = load_contract_parity_fixture();

        let create_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/specs")
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "title": "Structured contract parity spec",
                            "slug": "contract/structured-parity",
                            "component": "context-engine",
                            "scope": "public",
                            "fields": fixture.fields.clone(),
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(create_response.status(), StatusCode::CREATED);
        let created = response_json(create_response).await;
        let spec_id = created["id"].as_str().unwrap().to_string();

        let health_before = response_json(
            app.clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/specs/health?id={spec_id}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await;

        assert_eq!(health_before["issues_count"], 1);
        assert_eq!(
            health_before["issues"][0]["issue"],
            fixture.expected_health_issue
        );

        let update_response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("PATCH")
                    .uri(format!("/api/specs/{spec_id}"))
                    .header("content-type", "application/json")
                    .body(Body::from(
                        serde_json::to_vec(&json!({
                            "fields": fixture.fulfillment_update.clone(),
                        }))
                        .unwrap(),
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(update_response.status(), StatusCode::OK);

        let fetched = response_json(
            app.clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/specs/{spec_id}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await;

        assert_eq!(
            fetched["spec"]["fields"]["contract_mode"],
            "expectation-oriented"
        );
        assert_eq!(
            fetched["spec"]["fields"]["fulfillment_summaries"][0]["status"],
            "satisfied"
        );

        let searched = response_json(
            app.clone()
                .oneshot(
                    Request::builder()
                        .uri(format!(
                            "/api/specs/search?q={}",
                            fixture.search_query.replace(' ', "%20")
                        ))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await;

        assert_eq!(searched["count"], 1);
        assert_eq!(searched["items"][0]["id"], spec_id);

        let health_after = response_json(
            app.clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/api/specs/health?id={spec_id}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap(),
        )
        .await;

        assert_eq!(health_after["issues_count"], 0);
    }
}
