use axum::{
    body::{
        Body,
        to_bytes,
    },
    http::{
        Method,
        Request,
        StatusCode,
    },
};
use tower::ServiceExt;

use doc_http::{
    DocAppState,
    build_router,
};

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

#[tokio::test]
async fn workspace_endpoint_reports_packages() {
    let dir = temp_workspace();
    let app = make_app(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/docs/workspace")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 16 * 1024).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["package_count"], 2);
    assert_eq!(payload["packages"][0]["doc_target_count"], 1);
}

#[tokio::test]
async fn artifacts_endpoint_reports_existing_and_missing_outputs() {
    let dir = temp_workspace();
    write_file(&dir.path().join("target/doc/alpha_crate/index.html"), "<html>alpha</html>");
    write_file(&dir.path().join("target/doc/alpha_crate.json"), "{\"crate\":\"alpha\"}");

    let app = make_app(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/docs/artifacts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 32 * 1024).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["artifact_count"], 2);
    assert!(payload["artifacts"].as_array().unwrap().iter().any(|artifact| {
        artifact["package_name"] == "alpha-crate"
            && artifact["html_exists"] == true
            && artifact["rustdoc_json_exists"] == true
    }));
    assert!(payload["artifacts"].as_array().unwrap().iter().any(|artifact| {
        artifact["package_name"] == "beta-tool"
            && artifact["html_exists"] == false
            && artifact["rustdoc_json_exists"] == false
    }));
}

#[tokio::test]
async fn html_artifact_endpoint_serves_existing_file() {
    let dir = temp_workspace();
    write_file(&dir.path().join("target/doc/alpha_crate/index.html"), "<html>alpha</html>");

    let app = make_app(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/docs/artifacts/alpha-crate/alpha_crate/html")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body = to_bytes(resp.into_body(), 8 * 1024).await.unwrap();
    assert_eq!(&body[..], b"<html>alpha</html>");
}

#[tokio::test]
async fn rustdoc_json_endpoint_returns_404_when_missing() {
    let dir = temp_workspace();
    let app = make_app(dir.path());

    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/api/docs/artifacts/beta-tool/beta-tool/rustdoc-json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = to_bytes(resp.into_body(), 8 * 1024).await.unwrap();
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(payload["code"], "doc.artifact_not_found");
}

fn make_app(root: &std::path::Path) -> axum::Router {
    build_router(DocAppState::new(root.to_path_buf()))
}

fn temp_workspace() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();

    write_file(
        &dir.path().join("Cargo.toml"),
        r#"[workspace]
members = ["alpha", "beta"]
resolver = "2"
"#,
    );

    write_file(
        &dir.path().join("alpha/Cargo.toml"),
        r#"[package]
name = "alpha-crate"
version = "0.1.0"
edition = "2024"
"#,
    );
    write_file(
        &dir.path().join("alpha/src/lib.rs"),
        "pub fn alpha() -> &'static str { \"alpha\" }\n",
    );

    write_file(
        &dir.path().join("beta/Cargo.toml"),
        r#"[package]
name = "beta-tool"
version = "0.2.0"
edition = "2024"
"#,
    );
    write_file(
        &dir.path().join("beta/src/main.rs"),
        "fn main() {}\n",
    );

    dir
}

fn write_file(
    path: &std::path::Path,
    contents: &str,
) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, contents).unwrap();
}