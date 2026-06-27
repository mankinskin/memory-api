//! Route table for `ticket serve`.

use axum::{
    Router,
    middleware,
    routing::{
        get,
        patch,
        post,
    },
};

use viewer_api::middleware::request_id::add_request_id;

use super::{
    AppState,
    handlers,
    middleware as mw,
};

/// Build the full Axum router.
pub fn build_router(state: AppState) -> Router {
    let read_routes = Router::new()
        .route("/healthz", get(handlers::health::healthz))
        .route(
            "/api/workspaces",
            get(handlers::workspaces::list_workspaces),
        )
        .route("/api/tickets", get(handlers::tickets::list_tickets))
        .route("/api/tickets/{id}", get(handlers::tickets::get_ticket))
        .route(
            "/api/tickets/{id}/description",
            get(handlers::tickets::get_ticket_description),
        )
        .route(
            "/api/tickets/{id}/history",
            get(handlers::tickets::get_ticket_history),
        )
        .route(
            "/api/tickets/{id}/files",
            get(handlers::tickets::list_ticket_files),
        )
        .route(
            "/api/tickets/{id}/asset",
            get(handlers::tickets::get_ticket_asset),
        )
        .route("/api/workflow/next", get(handlers::workflow::workflow_next))
        .route(
            "/api/workflow/blockers",
            get(handlers::workflow::workflow_blockers),
        )
        .route(
            "/api/workflow/unblocked-by",
            get(handlers::workflow::workflow_unblocked_by),
        )
        .route("/api/edges", get(handlers::edges::list_edges))
        .route("/api/schema", get(handlers::schema::list_schemas))
        .route("/api/schema/{type_id}", get(handlers::schema::get_schema))
        .route("/api/graph/workspace", get(handlers::graph::workspace_graph))
        .route("/api/graph/subgraph", get(handlers::graph::subgraph))
        .route("/api/graph/topgraph", get(handlers::graph::topgraph))
        .route("/api/graph/health", get(handlers::graph::health_check))
        .route("/api/stream", get(handlers::stream::stream_handler));

    let write_routes = Router::new()
        .route("/api/batch", post(handlers::batch::batch_tickets))
        .route("/api/tickets", post(handlers::tickets::create_ticket))
        .route(
            "/api/tickets/{id}",
            patch(handlers::tickets::update_ticket)
                .delete(handlers::tickets::delete_ticket),
        )
        .route(
            "/api/tickets/{id}/close",
            post(handlers::tickets::close_ticket),
        )
        .route(
            "/api/tickets/{id}/cancel",
            post(handlers::tickets::cancel_ticket),
        )
        .route(
            "/api/tickets/{id}/move",
            post(handlers::tickets::move_ticket),
        )
        .route(
            "/api/tickets/{id}/undo",
            post(handlers::tickets::undo_ticket),
        )
        .route(
            "/api/tickets/{id}/revert",
            post(handlers::tickets::revert_ticket),
        )
        .route(
            "/api/edges",
            post(handlers::edges::add_edge)
                .delete(handlers::edges::remove_edge),
        )
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            mw::write_auth,
        ));

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
    use crate::serve::{
        AppState,
        StreamBroker,
        WorkspaceRegistry,
    };

    use axum::{
        body::{
            Body,
            to_bytes,
        },
        http::{
            Method,
            Request,
            StatusCode,
            header,
        },
    };
    use std::{
        collections::BTreeMap,
        sync::Arc,
    };
    use ticket_api::{
        model::filesystem::ScanRoot,
        storage::store::TicketStore,
    };
    use tower::ServiceExt;
    use uuid::Uuid;

    fn primary_workspace_name(dir: &std::path::Path) -> String {
        crate::serve::registry::canonical_workspace_name_for_index_root(
            dir,
            "workspace",
        )
    }

    fn make_router(dir: &std::path::Path) -> axum::Router {
        let store = Arc::new(TicketStore::init(dir).expect("open store"));
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

    /// Build a router around an already-opened store (avoids double-open of the SQLite database).
    fn make_router_from_store(store: Arc<TicketStore>) -> axum::Router {
        let state = AppState::new(
            Arc::new(WorkspaceRegistry::single_opened(store)),
            Arc::new(StreamBroker::new()),
        );
        build_router(state)
    }

    /// Create a ticket via the store and return its UUID string.
    fn create_ticket(
        dir: &std::path::Path,
        title: &str,
    ) -> (Arc<TicketStore>, Uuid) {
        let store = Arc::new(TicketStore::init(dir).expect("open store"));
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
        let workspace = primary_workspace_name(dir.path());

        // Advance state so there is a revision 1 (new) and revision 2 (ready).
        store
            .update(&id, BTreeMap::new(), None, Some("ready"), None, None)
            .expect("advance to ready");

        let app = make_router_from_store(Arc::clone(&store));

        let body = serde_json::json!({ "revision": 1 }).to_string();
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/tickets/{id}/revert?workspace={workspace}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let payload: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["ticket"]["fields"]["state"], "new");
        assert_eq!(payload["ticket"]["fields"]["title"], "Router revert test");
        // request_id header is injected by middleware — must be present.
        assert!(payload.get("request_id").is_some());
        assert_eq!(payload["workspace"], workspace);
    }

    #[tokio::test]
    async fn revert_route_returns_400_for_missing_revision() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (_store, id) = create_ticket(dir.path(), "T");
        let workspace = primary_workspace_name(dir.path());

        let app = make_router_from_store(Arc::clone(&_store));

        // revision 99 does not exist — only revision 1 was created.
        let body = serde_json::json!({ "revision": 99 }).to_string();
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!("/api/tickets/{id}/revert?workspace={workspace}"))
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);

        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let payload: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["code"], "revision_not_found");
    }

    #[tokio::test]
    async fn revert_route_returns_404_for_unknown_ticket() {
        let dir = tempfile::tempdir().expect("tempdir");
        let app = make_router(dir.path());
        let workspace = primary_workspace_name(dir.path());

        let fake_id = Uuid::new_v4();
        let body = serde_json::json!({ "revision": 1 }).to_string();
        let request = Request::builder()
            .method(Method::POST)
            .uri(format!(
                "/api/tickets/{fake_id}/revert?workspace={workspace}"
            ))
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
        let app = make_router_from_store(Arc::clone(&_store));
        let workspace = primary_workspace_name(dir.path());

        // GET is not registered for the revert path.
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/tickets/{id}/revert?workspace={workspace}"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn history_route_returns_200_with_revision_entries() {
        let dir = tempfile::tempdir().expect("tempdir");
        let (store, id) = create_ticket(dir.path(), "History smoke");
        let workspace = primary_workspace_name(dir.path());

        // Add a second revision so history has 2 entries.
        store
            .update(&id, BTreeMap::new(), None, Some("ready"), None, None)
            .expect("advance state");

        let app = make_router_from_store(Arc::clone(&store));

        let request = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/tickets/{id}/history?workspace={workspace}"))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let payload: serde_json::Value =
            serde_json::from_slice(&bytes).unwrap();
        assert_eq!(payload["count"], 2);
        // Oldest-first: first entry is the initial creation revision.
        assert_eq!(payload["entries"][0]["rev"], 1);
        assert_eq!(payload["entries"][0]["fields"]["state"], "new");
    }

    /// Verify that multiple concurrent subgraph requests all complete without
    /// deadlocking.  This exercises the `spawn_blocking` path in the graph
    /// handlers: if blocking storage I/O were performed on an async worker
    /// thread, the single-threaded test runtime would stall and the timeouts
    /// below would fire.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_subgraph_requests_all_complete() {
        use std::sync::Arc;
        use tokio::time::{
            Duration,
            timeout,
        };

        let dir = tempfile::tempdir().expect("tempdir");
        let store =
            Arc::new(TicketStore::init(dir.path()).expect("open store"));
        let workspace = primary_workspace_name(dir.path());
        store
            .add_scan_root(ScanRoot {
                path: dir.path().join("tickets"),
                label: "default".into(),
            })
            .expect("add scan root");

        // Create 8 tickets so each concurrent request has a unique root.
        let ids: Vec<Uuid> = (0..8)
            .map(|i| {
                store
                    .create(
                        None,
                        "tracker-improvement",
                        Some(&format!("Concurrent ticket {i}")),
                        None,
                        BTreeMap::new(),
                        None,
                        None,
                    )
                    .expect("create ticket")
            })
            .collect();

        let app = make_router_from_store(Arc::clone(&store));

        let handles: Vec<_> = ids
            .iter()
            .map(|id| {
                // `Router` implements `Clone` — each task gets its own clone.
                let app = app.clone();
                let id = *id;
                let workspace = workspace.clone();
                tokio::spawn(async move {
                    let req = Request::builder()
                        .uri(format!(
                            "/api/graph/subgraph?workspace={workspace}&root={id}&depth=2"
                        ))
                        .body(Body::empty())
                        .unwrap();
                    timeout(Duration::from_secs(5), app.oneshot(req))
                        .await
                        .expect("request should complete within 5 s")
                        .expect("oneshot should not error")
                })
            })
            .collect();

        for handle in handles {
            let resp = handle.await.expect("task panicked");
            assert_eq!(
                resp.status(),
                StatusCode::OK,
                "subgraph request returned non-200"
            );
        }
    }

    fn open_workspace_store(dir: &std::path::Path) -> Arc<TicketStore> {
        let store = Arc::new(TicketStore::init(dir).expect("open store"));
        store
            .add_scan_root(ScanRoot {
                path: dir.join("tickets"),
                label: "default".into(),
            })
            .expect("add scan root");
        store
    }
    /// Verify that multiple concurrent ticket-list requests all complete.
    /// The list handler hits the storage layer on every call; running 8 at
    /// once confirms there is no mutex starvation or deadlock.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_list_requests_all_complete() {
        use std::sync::Arc;
        use tokio::time::{
            Duration,
            timeout,
        };
        let dir = tempfile::tempdir().expect("tempdir");
        let store =
            Arc::new(TicketStore::init(dir.path()).expect("open store"));
        let workspace = primary_workspace_name(dir.path());
        store
            .add_scan_root(ScanRoot {
                path: dir.path().join("tickets"),
                label: "default".into(),
            })
            .expect("add scan root");

        // Create a few tickets so the list response is non-trivial.
        for i in 0..5 {
            store
                .create(
                    None,
                    "tracker-improvement",
                    Some(&format!("List ticket {i}")),
                    None,
                    BTreeMap::new(),
                    None,
                    None,
                )
                .expect("create ticket");
        }

        let app = make_router_from_store(Arc::clone(&store));

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let app = app.clone();
                let workspace = workspace.clone();
                tokio::spawn(async move {
                    let req = Request::builder()
                        .uri(format!("/api/tickets?workspace={workspace}"))
                        .body(Body::empty())
                        .unwrap();
                    timeout(Duration::from_secs(5), app.oneshot(req))
                        .await
                        .expect("request should complete within 5 s")
                        .expect("oneshot should not error")
                })
            })
            .collect();

        for handle in handles {
            let resp = handle.await.expect("task panicked");
            assert_eq!(resp.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn descendant_ticket_ref_from_list_is_followable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let child_dir = dir.path().join("child");
        std::fs::create_dir_all(&child_dir).expect("create child dir");

        let parent_store = open_workspace_store(dir.path());
        let child_store = open_workspace_store(child_dir.as_path());

        let child_id = child_store
            .create(
                None,
                "tracker-improvement",
                Some("Child ticket"),
                None,
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create child ticket");

        parent_store
            .add_scan_root(ScanRoot {
                path: child_dir.join("tickets"),
                label: "child".into(),
            })
            .expect("add child scan root");
        parent_store.scan(false).expect("scan child workspace");

        let app = make_router_from_store(Arc::clone(&parent_store));
        let workspace = primary_workspace_name(dir.path());

        let list_request = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/tickets?workspace={workspace}"))
            .body(Body::empty())
            .unwrap();

        let list_response = app.clone().oneshot(list_request).await.unwrap();
        assert_eq!(list_response.status(), StatusCode::OK);

        let list_bytes = to_bytes(list_response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let list_payload: serde_json::Value =
            serde_json::from_slice(&list_bytes).unwrap();
        assert_eq!(
            list_payload["items"][0]["ticket_ref"]["workspace"],
            "child"
        );
        assert_eq!(
            list_payload["items"][0]["ticket_ref"]["id"],
            child_id.to_string()
        );

        let detail_request = Request::builder()
            .method(Method::GET)
            .uri(format!("/api/tickets/{child_id}?workspace=child"))
            .body(Body::empty())
            .unwrap();

        let detail_response = app.oneshot(detail_request).await.unwrap();
        assert_eq!(detail_response.status(), StatusCode::OK);

        let detail_bytes = to_bytes(detail_response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let detail_payload: serde_json::Value =
            serde_json::from_slice(&detail_bytes).unwrap();
        assert_eq!(detail_payload["active_workspace"], "child");
        assert_eq!(
            detail_payload["ticket"]["ticket_ref"]["workspace"],
            "child"
        );
        assert_eq!(
            detail_payload["ticket"]["ticket_ref"]["id"],
            child_id.to_string()
        );
    }

    #[tokio::test]
    async fn ancestor_graph_ref_from_child_workspace_is_followable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let child_dir = dir.path().join("child");
        std::fs::create_dir_all(&child_dir).expect("create child dir");

        let parent_store = open_workspace_store(dir.path());
        let child_store = open_workspace_store(&child_dir);

        let parent_id = parent_store
            .create(
                None,
                "tracker-improvement",
                Some("Parent ticket"),
                None,
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create parent ticket");
        let child_id = child_store
            .create(
                None,
                "tracker-improvement",
                Some("Child ticket"),
                None,
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create child ticket");

        child_store
            .add_edge(ticket_api::model::edge::EdgeRecord {
                from: child_id,
                to: parent_id,
                kind: "depends_on".into(),
                created_at: chrono::Utc::now(),
            })
            .expect("add mixed-workspace edge");

        let app = make_router_from_store(Arc::clone(&child_store));
        let child_workspace = primary_workspace_name(child_dir.as_path());
        let parent_workspace = primary_workspace_name(dir.path());

        let graph_request = Request::builder()
            .method(Method::GET)
            .uri(format!(
                "/api/graph/subgraph?workspace={child_workspace}&root={child_id}&depth=1"
            ))
            .body(Body::empty())
            .unwrap();

        let graph_response = app.clone().oneshot(graph_request).await.unwrap();
        assert_eq!(graph_response.status(), StatusCode::OK);

        let graph_bytes = to_bytes(graph_response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let graph_payload: serde_json::Value =
            serde_json::from_slice(&graph_bytes).unwrap();

        let parent_node = graph_payload["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|node| node["id"] == parent_id.to_string())
            .expect("parent node present");
        assert_eq!(
            parent_node["ticket_ref"]["workspace"],
            parent_workspace.clone()
        );
        assert_eq!(parent_node["ticket_ref"]["id"], parent_id.to_string());
        assert_eq!(
            graph_payload["edges"][0]["to_ref"]["workspace"],
            parent_workspace.clone()
        );

        let history_request = Request::builder()
            .method(Method::GET)
            .uri(format!(
                "/api/tickets/{parent_id}/history?workspace={parent_workspace}"
            ))
            .body(Body::empty())
            .unwrap();

        let history_response = app.oneshot(history_request).await.unwrap();
        assert_eq!(history_response.status(), StatusCode::OK);

        let history_bytes = to_bytes(history_response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let history_payload: serde_json::Value =
            serde_json::from_slice(&history_bytes).unwrap();
        assert_eq!(
            history_payload["active_workspace"],
            parent_workspace.clone()
        );
        assert_eq!(
            history_payload["ticket_ref"]["workspace"],
            parent_workspace
        );
        assert_eq!(history_payload["ticket_ref"]["id"], parent_id.to_string());
    }

    #[tokio::test]
    async fn workspace_graph_includes_isolated_local_and_cross_workspace_nodes() {
        let dir = tempfile::tempdir().expect("tempdir");
        let child_dir = dir.path().join("child");
        std::fs::create_dir_all(&child_dir).expect("create child dir");

        let parent_store = open_workspace_store(dir.path());
        let child_store = open_workspace_store(child_dir.as_path());

        let parent_id = parent_store
            .create(
                None,
                "tracker-improvement",
                Some("Parent ticket"),
                None,
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create parent ticket");
        let child_id = child_store
            .create(
                None,
                "tracker-improvement",
                Some("Child ticket"),
                None,
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create child ticket");
        let isolated_id = child_store
            .create(
                None,
                "tracker-improvement",
                Some("Isolated ticket"),
                None,
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create isolated ticket");

        child_store
            .add_edge(ticket_api::model::edge::EdgeRecord {
                from: child_id,
                to: parent_id,
                kind: "depends_on".into(),
                created_at: chrono::Utc::now(),
            })
            .expect("add mixed-workspace edge");

        let app = make_router_from_store(Arc::clone(&child_store));
        let workspace_request = Request::builder()
            .method(Method::GET)
            .uri("/api/workspaces")
            .body(Body::empty())
            .unwrap();
        let workspace_response = app.clone().oneshot(workspace_request).await.unwrap();
        assert_eq!(workspace_response.status(), StatusCode::OK);
        let workspace_bytes = to_bytes(
            workspace_response.into_body(),
            1024 * 1024,
        )
        .await
        .unwrap();
        let workspace_payload: serde_json::Value =
            serde_json::from_slice(&workspace_bytes).unwrap();
        let child_workspace = workspace_payload["active_workspace"]
            .as_str()
            .expect("active workspace")
            .to_string();
        let request = Request::builder()
            .method(Method::GET)
            .uri(format!(
                "/api/graph/workspace?workspace={child_workspace}"
            ))
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();

        let nodes = payload["nodes"].as_array().expect("nodes array");
        assert!(
            nodes.iter().any(|node| node["id"] == child_id.to_string()),
            "workspace graph should include the local graph root"
        );
        assert!(
            nodes.iter().any(|node| {
                node["id"] == isolated_id.to_string()
                    && node["ticket_ref"]["workspace"] == child_workspace
            }),
            "workspace graph should include isolated local tickets"
        );
        assert!(
            nodes.iter().any(|node| node["id"] == parent_id.to_string()),
            "workspace graph should include cross-workspace edge endpoints"
        );

        let edges = payload["edges"].as_array().expect("edges array");
        assert!(
            edges.iter().any(|edge| {
                edge["from"] == child_id.to_string()
                    && edge["to"] == parent_id.to_string()
            }),
            "workspace graph should preserve mixed-workspace relationship metadata"
        );
        assert_eq!(payload["truncated"], false);
    }
}
