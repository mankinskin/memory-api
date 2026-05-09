use axum::{
    body::{Body, to_bytes},
    http::{Method, Request, StatusCode, header},
};
use tower::ServiceExt;

use super::support::{make_app, seed_spec};

#[tokio::test]
async fn list_sections_returns_empty_for_new_spec() {
    let dir = tempfile::tempdir().unwrap();
    let id = seed_spec(dir.path(), "sections-me", "Sections Spec");

    let resp = make_app(dir.path())
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/specs/{id}/sections"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["count"], 0);
}

#[tokio::test]
async fn add_section_then_list_shows_section() {
    let dir = tempfile::tempdir().unwrap();
    let id = seed_spec(dir.path(), "section-lifecycle", "Section Lifecycle");

    let add_body = serde_json::json!({
        "name": "risks",
        "content": "# Risks\nNone known.",
    })
    .to_string();

    let add_resp = make_app(dir.path())
        .oneshot(
            Request::builder()
                .method(Method::POST)
                .uri(format!("/api/specs/{id}/sections"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(add_body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(add_resp.status(), StatusCode::CREATED);

    let list_resp = make_app(dir.path())
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/specs/{id}/sections"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(list_resp.status(), StatusCode::OK);
    let bytes = to_bytes(list_resp.into_body(), 4096).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["count"], 1);
    assert_eq!(payload["sections"][0], "risks.md");
}

#[tokio::test]
async fn get_refs_returns_empty_list_for_new_spec() {
    let dir = tempfile::tempdir().unwrap();
    let id = seed_spec(dir.path(), "refs-me", "Refs Spec");

    let resp = make_app(dir.path())
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/specs/{id}/refs"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["count"], 0);
}

#[tokio::test]
async fn get_tree_returns_spec_with_no_descendants() {
    let dir = tempfile::tempdir().unwrap();
    let id = seed_spec(dir.path(), "tree-root", "Tree Root");

    let resp = make_app(dir.path())
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri(format!("/api/specs/{id}/tree"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = to_bytes(resp.into_body(), 4096).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["root"]["id"], id);
    assert_eq!(payload["descendants"], serde_json::json!([]));
}