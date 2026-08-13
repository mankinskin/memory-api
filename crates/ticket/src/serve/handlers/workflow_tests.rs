use super::*;

use std::{
    collections::BTreeMap,
    sync::Arc,
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
use serde_json::{
    Value,
    json,
};
use ticket_api::{
    model::{
        edge::EdgeRecord,
        filesystem::ScanRoot,
    },
    storage::store::TicketStore,
};
use tower::ServiceExt;

use crate::serve::{
    AppState,
    StreamBroker,
    WorkspaceRegistry,
    routes::build_router,
};

fn workspace_name(dir: &std::path::Path) -> String {
    crate::serve::registry::canonical_workspace_name_for_index_root(
        dir,
        "workspace",
    )
}

fn make_store(dir: &std::path::Path) -> Arc<TicketStore> {
    let store = Arc::new(TicketStore::init(dir).expect("open store"));
    store
        .add_scan_root(ScanRoot {
            path: dir.join("tickets"),
            label: "default".into(),
        })
        .expect("add scan root");
    store
}

fn make_router(store: Arc<TicketStore>) -> axum::Router {
    let state = AppState::new(
        Arc::new(WorkspaceRegistry::single_opened(store)),
        Arc::new(StreamBroker::new()),
    );
    build_router(state)
}

async fn get_json(
    app: axum::Router,
    uri: String,
) -> Value {
    let response = app
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn workflow_next_preserves_recent_actionable_order_and_supports_root_scope()
 {
    let dir = tempfile::tempdir().unwrap();
    let workspace = workspace_name(dir.path());
    let store = make_store(dir.path());
    let app = make_router(Arc::clone(&store));

    let high_fields =
        BTreeMap::from([(String::from("priority"), json!("high"))]);
    let recently_actionable = store
        .create(
            None,
            "tracker-improvement",
            Some("Alpha recently actionable"),
            Some("planned"),
            high_fields.clone(),
            None,
            None,
        )
        .unwrap();
    let steadier_newer = store
        .create(
            None,
            "tracker-improvement",
            Some("Zulu steady ready work"),
            Some("planned"),
            high_fields.clone(),
            None,
            None,
        )
        .unwrap();
    let transient_blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Transient blocker"),
            Some("in-review"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();
    store
        .add_edge(EdgeRecord {
            from: recently_actionable,
            to: transient_blocker,
            kind: String::from("depends_on"),
            created_at: chrono::Utc::now(),
        })
        .unwrap();
    store.close(&transient_blocker, "done", None).unwrap();

    let next = get_json(
        app.clone(),
        format!("/api/workflow/next?workspace={workspace}"),
    )
    .await;
    let items = next["items"].as_array().unwrap();
    assert!(
        items.len() >= 2,
        "expected at least two candidates: {items:?}"
    );
    assert_eq!(items[0]["id"], recently_actionable.to_string());
    assert_eq!(items[1]["id"], steadier_newer.to_string());
    assert!(items[0]["became_actionable_at"].as_str().is_some());
    assert!(items[1]["became_actionable_at"].as_str().is_some());
    assert_eq!(next["scope"]["workspace"], workspace.as_str());
    assert_eq!(next["excluded_by_board"], json!([]));
    assert_eq!(next["warnings"], json!([]));
    assert!(
        next["scope"]["active_index_root"].as_str().is_some(),
        "scope.active_index_root should be present",
    );
    assert!(next["scope"]["filter"].is_null());
    assert!(next["scope"]["root"].is_null());

    let root = store
        .create(
            None,
            "tracker-improvement",
            Some("Root ticket to unblock"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();
    let scoped_blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Scoped blocker"),
            Some("planned"),
            high_fields,
            None,
            None,
        )
        .unwrap();
    let intermediate_blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Intermediate blocker"),
            Some("open"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();
    let nested_leaf = store
        .create(
            None,
            "tracker-improvement",
            Some("Nested actionable blocker"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();

    for (from, to) in [
        (root, scoped_blocker),
        (root, intermediate_blocker),
        (intermediate_blocker, nested_leaf),
    ] {
        store
            .add_edge(EdgeRecord {
                from,
                to,
                kind: String::from("depends_on"),
                created_at: chrono::Utc::now(),
            })
            .unwrap();
    }

    let scoped = get_json(
        app,
        format!("/api/workflow/next?workspace={workspace}&root={root}"),
    )
    .await;
    assert_eq!(scoped["root"]["id"], root.to_string());
    assert_eq!(scoped["reachable_dependencies"], 3);
    assert_eq!(scoped["blocked_dependencies"], 1);
    assert_eq!(scoped["remaining_blocker_count"], 3);
    assert_eq!(scoped["frontier_count"], 2);
    let item_ids = scoped["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect::<std::collections::HashSet<_>>();
    assert!(item_ids.contains(scoped_blocker.to_string().as_str()));
    assert!(item_ids.contains(nested_leaf.to_string().as_str()));
    assert!(!item_ids.contains(intermediate_blocker.to_string().as_str()));
    assert_eq!(scoped["blocker_tree"]["id"], root.to_string());
    assert_eq!(scoped["scope"]["workspace"], workspace.as_str());
    assert_eq!(scoped["excluded_by_board"], json!([]));
    assert_eq!(scoped["warnings"], json!([]));
    assert!(
        scoped["scope"]["active_index_root"].as_str().is_some(),
        "scope.active_index_root should be present in scoped response",
    );
    assert_eq!(scoped["scope"]["root"], root.to_string());
}

#[tokio::test]
async fn workflow_next_filters_board_active_candidates_into_excluded_by_board()
{
    let dir = tempfile::tempdir().unwrap();
    let workspace = workspace_name(dir.path());
    let store = make_store(dir.path());
    let app = make_router(Arc::clone(&store));

    let active = store
        .create(
            None,
            "tracker-improvement",
            Some("Active board ticket"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();
    let free = store
        .create(
            None,
            "tracker-improvement",
            Some("Free candidate ticket"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();

    store
        .board_configure(Some(ticket_api::BoardConfig {
            max_wip: 1,
            stale_after_secs: 3600,
            completed_audit_window_secs: 3600,
        }))
        .unwrap();
    store
        .board_check_in(
            &active,
            "http-parity-agent",
            3600,
            "in flight",
            Vec::new(),
            None,
            None,
            None,
        )
        .unwrap();

    let next =
        get_json(app, format!("/api/workflow/next?workspace={workspace}"))
            .await;
    let items = next["items"].as_array().unwrap();
    let excluded = next["excluded_by_board"].as_array().unwrap();
    let warnings = next["warnings"].as_array().unwrap();

    assert!(items.iter().any(|item| item["id"] == free.to_string()));
    assert!(!items.iter().any(|item| item["id"] == active.to_string()));
    assert!(
        excluded
            .iter()
            .any(|entry| entry["ticket_id"] == active.to_string()),
        "active board ticket must be surfaced in excluded_by_board: {excluded:?}"
    );
    assert!(
        warnings.iter().any(|warning| warning
            .as_str()
            .unwrap_or("")
            .contains("WIP limit reached")),
        "expected WIP warning, got {warnings:?}"
    );
}

#[tokio::test]
async fn workflow_blockers_returns_nested_tree_and_frontier_items() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = workspace_name(dir.path());
    let store = make_store(dir.path());
    let app = make_router(Arc::clone(&store));

    let root = store
        .create(
            None,
            "tracker-improvement",
            Some("Root"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();
    let direct_leaf = store
        .create(
            None,
            "tracker-improvement",
            Some("Direct frontier leaf"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();
    let nested_parent = store
        .create(
            None,
            "tracker-improvement",
            Some("Nested parent"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();
    let nested_leaf = store
        .create(
            None,
            "tracker-improvement",
            Some("Nested frontier leaf"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();

    for (from, to) in [
        (root, nested_parent),
        (root, direct_leaf),
        (nested_parent, nested_leaf),
    ] {
        store
            .add_edge(EdgeRecord {
                from,
                to,
                kind: String::from("depends_on"),
                created_at: chrono::Utc::now(),
            })
            .unwrap();
    }

    let response = get_json(
        app,
        format!("/api/workflow/blockers?workspace={workspace}&root={root}"),
    )
    .await;

    assert_eq!(response["kind"], "blockers");
    assert_eq!(response["root"]["id"], root.to_string());
    assert_eq!(response["root"]["unresolved_frontier_leaf_count"], 2);
    let children = response["root"]["children"].as_array().unwrap();
    assert_eq!(children[0]["id"], direct_leaf.to_string());
    assert_eq!(children[1]["id"], nested_parent.to_string());
    let frontier = response["frontier_items"].as_array().unwrap();
    assert_eq!(frontier.len(), 2);
    assert_eq!(frontier[0]["id"], direct_leaf.to_string());
    assert_eq!(frontier[1]["id"], nested_leaf.to_string());
}

#[tokio::test]
async fn workflow_unblocked_by_returns_nested_tree_and_frontier_items() {
    let dir = tempfile::tempdir().unwrap();
    let workspace = workspace_name(dir.path());
    let store = make_store(dir.path());
    let app = make_router(Arc::clone(&store));

    let root = store
        .create(
            None,
            "tracker-improvement",
            Some("Shared prerequisite"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();
    let direct = store
        .create(
            None,
            "tracker-improvement",
            Some("Direct dependent"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();
    let transitive = store
        .create(
            None,
            "tracker-improvement",
            Some("Transitive dependent"),
            Some("open"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();
    let extra_blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Other blocker"),
            Some("planned"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();
    let still_blocked = store
        .create(
            None,
            "tracker-improvement",
            Some("Still blocked dependent"),
            Some("open"),
            BTreeMap::new(),
            None,
            None,
        )
        .unwrap();

    for (from, to) in [
        (direct, root),
        (transitive, direct),
        (still_blocked, root),
        (still_blocked, extra_blocker),
    ] {
        store
            .add_edge(EdgeRecord {
                from,
                to,
                kind: String::from("depends_on"),
                created_at: chrono::Utc::now(),
            })
            .unwrap();
    }

    let response = get_json(
        app,
        format!("/api/workflow/unblocked-by?workspace={workspace}&root={root}"),
    )
    .await;

    assert_eq!(response["kind"], "unblocked-by");
    assert_eq!(response["root"]["id"], root.to_string());
    assert_eq!(response["reachable_dependents"], 3);
    assert_eq!(response["blocked_dependents"], 2);
    let children = response["root"]["children"].as_array().unwrap();
    assert_eq!(children[0]["id"], direct.to_string());
    let frontier = response["frontier_items"].as_array().unwrap();
    assert_eq!(frontier.len(), 2);
    assert_eq!(frontier[0]["id"], direct.to_string());
    assert_eq!(frontier[1]["id"], still_blocked.to_string());
}
