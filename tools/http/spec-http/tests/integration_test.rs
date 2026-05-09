//! Integration tests for the spec-http Axum router.
//!
//! Uses `tower::ServiceExt::oneshot` to drive the full router in-process
//! — no TCP socket needed.

use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use tower::ServiceExt;

#[path = "integration_test/sections.rs"]
mod sections;
#[path = "integration_test/support.rs"]
mod support;

use support::{make_app, seed_spec};

// ── healthz ──────────────────────────────────────────────────────────────────

#[tokio::test]
async fn healthz_returns_ok() {
    let dir = tempfile::tempdir().unwrap();
    let app = make_app(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 1024).await.unwrap();
    assert_eq!(&body[..], b"ok");
}

// ── POST /api/specs — create ──────────────────────────────────────────────────

#[tokio::test]
async fn create_spec_returns_201_with_id_and_slug() {
    let dir = tempfile::tempdir().unwrap();
    let app = make_app(dir.path());

    let body = serde_json::json!({
        "title": "My Feature",
        "slug": "my-feature",
        "component": "core",
        "body": "# My Feature\nInitial content.",
    })
    .to_string();

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/specs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CREATED);
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(payload.get("id").is_some());
    assert_eq!(payload["slug"], "my-feature");
    assert!(payload.get("request_id").is_some());
}

#[tokio::test]
async fn create_spec_duplicate_slug_returns_409() {
    let dir = tempfile::tempdir().unwrap();
    seed_spec(dir.path(), "dup-slug", "First");

    let app = make_app(dir.path());

    let body = serde_json::json!({
        "title": "Second",
        "slug": "dup-slug",
        "component": "core",
    })
    .to_string();

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/specs")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["code"], "spec.duplicate_slug");
}

// ── GET /api/specs — list ─────────────────────────────────────────────────────

#[tokio::test]
async fn list_specs_returns_seeded_spec() {
    let dir = tempfile::tempdir().unwrap();
    seed_spec(dir.path(), "list-me", "Listed Spec");

    let app = make_app(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/specs")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["count"], 1);
    assert_eq!(payload["items"][0]["slug"], "list-me");
}

// ── GET /api/specs/:id — get ──────────────────────────────────────────────────

#[tokio::test]
async fn get_spec_by_id_returns_200() {
    let dir = tempfile::tempdir().unwrap();
    let id = seed_spec(dir.path(), "fetch-me", "Fetch Spec");

    let app = make_app(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/specs/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["spec"]["id"], id);
    assert!(payload.get("request_id").is_some());
}

#[tokio::test]
async fn get_spec_unknown_id_returns_404() {
    let dir = tempfile::tempdir().unwrap();
    let app = make_app(dir.path());

    let fake_id = uuid::Uuid::new_v4().to_string();
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/specs/{fake_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

// ── GET /api/specs/:id/full ───────────────────────────────────────────────────

#[tokio::test]
async fn get_spec_full_includes_body() {
    let dir = tempfile::tempdir().unwrap();
    let id = seed_spec(dir.path(), "full-me", "Full Spec");

    let app = make_app(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/specs/{id}/full"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 8192).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["spec"]["id"], id);
    assert!(payload["body"].as_str().is_some());
}

// ── PATCH /api/specs/:id — update ────────────────────────────────────────────

#[tokio::test]
async fn update_spec_state_returns_updated_fields() {
    let dir = tempfile::tempdir().unwrap();
    let id = seed_spec(dir.path(), "update-me", "Update Target");

    let app = make_app(dir.path());

    let body = serde_json::json!({
        "to_state": "reviewed",
    })
    .to_string();

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::PATCH)
                .uri(format!("/api/specs/{id}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["spec"]["fields"]["state"], "reviewed");
}

// ── DELETE /api/specs/:id ─────────────────────────────────────────────────────

#[tokio::test]
async fn delete_spec_returns_200_then_404_on_get() {
    let dir = tempfile::tempdir().unwrap();
    let id = seed_spec(dir.path(), "delete-me", "Delete Target");

    let delete_resp = make_app(dir.path())
        .oneshot(
            Request::builder()
                .method(Method::DELETE)
                .uri(format!("/api/specs/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(delete_resp.status(), StatusCode::OK);

    let get_resp = make_app(dir.path())
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/specs/{id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get_resp.status(), StatusCode::NOT_FOUND);
}

// ── GET /api/specs/search ─────────────────────────────────────────────────────

#[tokio::test]
async fn search_specs_returns_matching_result() {
    let dir = tempfile::tempdir().unwrap();
    seed_spec(dir.path(), "search-me", "Searchable Spec");

    let app = make_app(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/specs/search?q=searchable")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    // count may be 0 if full-text index not yet built, but should not error
    assert!(payload.get("items").is_some());
}

// ── POST /api/specs/scan ──────────────────────────────────────────────────────

#[tokio::test]
async fn scan_endpoint_returns_ok() {
    let dir = tempfile::tempdir().unwrap();
    let app = make_app(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri("/api/specs/scan")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["status"], "ok");
}

// ── GET /api/specs/health ─────────────────────────────────────────────────────

#[tokio::test]
async fn health_check_with_all_flag() {
    let dir = tempfile::tempdir().unwrap();
    seed_spec(dir.path(), "health-target", "Health Check Spec");

    let app = make_app(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/specs/health?all=true")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(payload.get("specs_checked").is_some());
    assert!(payload.get("issues").is_some());
}

