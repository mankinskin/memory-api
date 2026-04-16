//! Route table for `ticket serve`.

use axum::{
    Router,
    middleware,
    routing::{delete, get, patch, post},
};

use viewer_api::middleware::request_id::add_request_id;

use super::{AppState, handlers, middleware as mw};

/// Build the full Axum router.
pub fn build_router(state: AppState) -> Router {
    let read_routes = Router::new()
        .route("/healthz", get(handlers::health::healthz))
        .route("/api/workspaces", get(handlers::workspaces::list_workspaces))
        .route("/api/tickets", get(handlers::tickets::list_tickets))
        .route("/api/tickets/{id}", get(handlers::tickets::get_ticket))
        .route("/api/tickets/{id}/description", get(handlers::tickets::get_ticket_description))
        .route("/api/tickets/{id}/history", get(handlers::tickets::get_ticket_history))
        .route("/api/edges", get(handlers::edges::list_edges))
        .route("/api/schema", get(handlers::schema::list_schemas))
        .route("/api/schema/{type_id}", get(handlers::schema::get_schema))
        .route("/api/graph/subgraph", get(handlers::graph::subgraph))
        .route("/api/graph/topgraph", get(handlers::graph::topgraph))
        .route("/api/graph/health", get(handlers::graph::health_check))
        .route("/api/stream", get(handlers::stream::stream_handler));

    let write_routes = Router::new()
        .route("/api/batch", post(handlers::batch::batch_tickets))
        .route("/api/tickets", post(handlers::tickets::create_ticket))
        .route(
            "/api/tickets/{id}",
            patch(handlers::tickets::update_ticket).delete(handlers::tickets::delete_ticket),
        )
        .route("/api/tickets/{id}/close", post(handlers::tickets::close_ticket))
        .route("/api/tickets/{id}/cancel", post(handlers::tickets::cancel_ticket))
        .route("/api/tickets/{id}/undo", post(handlers::tickets::undo_ticket))
        .route("/api/tickets/{id}/revert", post(handlers::tickets::revert_ticket))
        .route(
            "/api/edges",
            post(handlers::edges::add_edge).delete(handlers::edges::remove_edge),
        )
        .route_layer(middleware::from_fn_with_state(state.clone(), mw::write_auth));

    read_routes
        .merge(write_routes)
        .layer(middleware::from_fn(add_request_id))
        .with_state(state)
}

#[cfg(test)]
mod tests {
    //! HTTP-level integration tests for the revert route.
    //!
    //! These tests drive the **full Axum router** (route dispatch, middleware,
    //! request parsing, response serialisation) using `tower::ServiceExt` — no
    //! real TCP socket required.

    use super::build_router;
    use crate::serve::{AppState, StreamBroker, WorkspaceRegistry};

    use axum::{
        body::{Body, to_bytes},
        http::{Method, Request, StatusCode, header},
    };
    use std::{collections::BTreeMap, sync::Arc};
    use ticket_api::{model::filesystem::ScanRoot, storage::store::TicketStore};
    use tower::ServiceExt;
    use uuid::Uuid;

    fn make_router(dir: &std::path::Path) -> axum::Router {
        let store = Arc::new(TicketStore::open(dir).expect("open store"));
        store
            .add_scan_root(ScanRoot {
                path: dir.join("tickets"),
                label: "default".into(),
            })
            .expect("add scan root");
        let state = AppState::new(
            Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&store))),
            Arc::new(StreamBroker::new()),
        );
        build_router(state)
    }

    /// Create a ticket via the store and return its UUID string.
    fn create_ticket(dir: &std::path::Path, title: &str) -> (Arc<TicketStore>, Uuid) {
        let store = Arc::new(TicketStore::open(dir).expect("open store"));
        store
            .add_scan_root(ScanRoot {
                path: dir.join("tickets"),
                label: "default".into(),
            })
            .expect("add scan root");
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some(title),
                None,
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create ticket");
        (store, id)
    }

    #[tokio::test]
    async fn revert_route_returns_200_with_restored_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (store, id) = create_ticket(dir.path(), "Router revert test");

        // Advance state so there is a revision 1 (new) and revision 2 (ready).
        store
            .update(&id, BTreeMap::new(), None, Some("ready"), None, None)
            .expect("advance to ready");

        let app = make_router(dir.path());

        let body = serde_json::json!({ "revision": 1 }).to_string();
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/tickets/{id}/revert?workspace=default"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["ticket"]["fields"]["state"], "new");
        assert_eq!(payload["ticket"]["fields"]["title"], "Router revert test");
        // request_id header is injected by middleware — must be present.
        assert!(payload.get("request_id").is_some());
        assert_eq!(payload["workspace"], "default");
    }

    #[tokio::test]
    async fn revert_route_returns_400_for_missing_revision() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (_store, id) = create_ticket(dir.path(), "T");

        let app = make_router(dir.path());

        // revision 99 does not exist — only revision 1 was created.
        let body = serde_json::json!({ "revision": 99 }).to_string();
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/tickets/{id}/revert?workspace=default"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["code"], "revision_not_found");
    }

    #[tokio::test]
    async fn revert_route_returns_404_for_unknown_ticket() {
        let dir = tempfile::tempdir().expect("tempdir");
        let app = make_router(dir.path());

        let fake_id = Uuid::new_v4();
        let body = serde_json::json!({ "revision": 1 }).to_string();
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/tickets/{fake_id}/revert?workspace=default"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn revert_route_rejects_wrong_http_method() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (_store, id) = create_ticket(dir.path(), "T");
        let app = make_router(dir.path());

        // GET is not registered for the revert path.
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/tickets/{id}/revert?workspace=default"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn history_route_returns_200_with_revision_entries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (store, id) = create_ticket(dir.path(), "History smoke");

        // Add a second revision so history has 2 entries.
        store
            .update(&id, BTreeMap::new(), None, Some("ready"), None, None)
            .expect("advance state");

        let app = make_router(dir.path());

        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/tickets/{id}/history?workspace=default"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["count"], 2);
        // Oldest-first: first entry is the initial creation revision.
        assert_eq!(payload["entries"][0]["rev"], 1);
        assert_eq!(payload["entries"][0]["fields"]["state"], "new");
    }
}
