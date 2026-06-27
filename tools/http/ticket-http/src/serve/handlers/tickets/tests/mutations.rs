use axum::{
    Json,
    body::to_bytes,
    extract::{
        Extension,
        Path,
        Query,
        State,
    },
    http::{
        HeaderMap,
        StatusCode,
    },
};
use std::process::Command;
use std::{
    collections::BTreeMap,
    sync::Arc,
};
use viewer_api::error::RequestIdExt;

use super::{
    super::{
        CreateTicketBody,
        MoveTicketBody,
        MutationWorkspaceParam,
        UpdateTicketBody,
        create_ticket,
        move_ticket,
        update_ticket,
    },
    make_state,
    make_store,
};

fn run_git(repo_root: &std::path::Path, args: &[&str]) {
    let status = Command::new("git")
        .current_dir(repo_root)
        .args(args)
        .status()
        .expect("git command");
    assert!(status.success(), "git {args:?} failed: {status}");
}

#[tokio::test]
async fn create_ticket_returns_201_with_new_ticket() {
    let dir = tempfile::tempdir().expect("tempdir");
    let state = make_state(make_store(dir.path()));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = create_ticket(
        State(state),
        Extension(RequestIdExt("rid-create".to_string())),
        Query(MutationWorkspaceParam {
            workspace: workspace.clone(),
        }),
        Json(CreateTicketBody {
            type_id: "tracker-improvement".to_string(),
            title: Some("My new ticket".to_string()),
            fields: None,
            description: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);

    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");

    assert_eq!(payload["workspace"], workspace);
    assert_eq!(payload["active_workspace"], workspace.clone());
    assert_eq!(payload["request_id"], "rid-create");
    assert_eq!(payload["ticket"]["fields"]["title"], "My new ticket");
    assert_eq!(payload["ticket"]["fields"]["state"], "new");
    assert_eq!(payload["ticket"]["ticket_ref"]["workspace"], workspace);
}

#[tokio::test]
async fn create_ticket_with_extra_fields_and_description() {
    let dir = tempfile::tempdir().expect("tempdir");
    let state = make_state(make_store(dir.path()));
    let workspace = state.registry.primary_workspace_name().to_string();

    let mut fields = BTreeMap::new();
    fields.insert(
        "priority".to_string(),
        serde_json::Value::String("high".to_string()),
    );

    let response = create_ticket(
        State(state),
        Extension(RequestIdExt("rid".to_string())),
        Query(MutationWorkspaceParam {
            workspace: workspace.clone(),
        }),
        Json(CreateTicketBody {
            type_id: "tracker-improvement".to_string(),
            title: Some("Ticket with fields".to_string()),
            fields: Some(fields),
            description: Some("## Overview\n\nSome description.".to_string()),
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["ticket"]["fields"]["priority"], "high");
}

#[tokio::test]
async fn update_ticket_patches_fields() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Original"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let mut patch = BTreeMap::new();
    patch.insert(
        "title".to_string(),
        serde_json::Value::String("Updated title".to_string()),
    );

    let response = update_ticket(
        State(state),
        Extension(RequestIdExt("rid-update".to_string())),
        Path(id),
        Query(MutationWorkspaceParam {
            workspace: workspace.clone(),
        }),
        HeaderMap::new(),
        Json(UpdateTicketBody {
            fields: Some(patch),
            state: None,
            transition_states: vec![],
            description: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["ticket"]["fields"]["title"], "Updated title");
}

#[tokio::test]
async fn update_ticket_transitions_state() {
    let dir = tempfile::tempdir().expect("tempdir");
    let store = make_store(dir.path());

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("T"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create");

    let state = make_state(Arc::clone(&store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = update_ticket(
        State(state),
        Extension(RequestIdExt("rid".to_string())),
        Path(id),
        Query(MutationWorkspaceParam {
            workspace: workspace.clone(),
        }),
        HeaderMap::new(),
        Json(UpdateTicketBody {
            fields: None,
            state: Some("ready".to_string()),
            transition_states: vec![],
            description: None,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value =
        serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["ticket"]["fields"]["state"], "ready");
}

#[tokio::test]
async fn move_ticket_dry_run_returns_structured_plan() {
    let dir = tempfile::tempdir().expect("tempdir");
    run_git(dir.path(), &["init"]);

    let source_store = make_store(dir.path());
    let target_workspace = dir.path().join("target-workspace");
    std::fs::create_dir_all(&target_workspace).expect("create target workspace");
    let _target_store = ticket_api::storage::store::TicketStore::init(&target_workspace)
        .expect("init target store");

    let id = source_store
        .create(
            None,
            "tracker-improvement",
            Some("Move dry-run"),
            Some("ready"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create source ticket");

    let state = make_state(Arc::clone(&source_store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = move_ticket(
        State(state),
        Extension(RequestIdExt("rid-move-dry-run".to_string())),
        Path(id),
        Query(MutationWorkspaceParam {
            workspace: workspace.clone(),
        }),
        Json(MoveTicketBody {
            to_workspace_root: target_workspace.to_string_lossy().to_string(),
            dry_run: true,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["mode"], "plan");
    assert_eq!(payload["status"], "ok");
    assert!(payload["plan"]["source_ticket_path"].is_string());
    assert!(payload["plan"]["destination_ticket_path"].is_string());
}

#[tokio::test]
async fn move_ticket_apply_executes_and_returns_journal() {
    let dir = tempfile::tempdir().expect("tempdir");
    run_git(dir.path(), &["init"]);

    let source_store = make_store(dir.path());
    let target_workspace = dir.path().join("target-workspace");
    std::fs::create_dir_all(&target_workspace).expect("create target workspace");
    let target_store = ticket_api::storage::store::TicketStore::init(&target_workspace)
        .expect("init target store");

    let id = source_store
        .create(
            None,
            "tracker-improvement",
            Some("Move apply"),
            Some("ready"),
            BTreeMap::new(),
            None,
            None,
        )
        .expect("create source ticket");

    let state = make_state(Arc::clone(&source_store));
    let workspace = state.registry.primary_workspace_name().to_string();

    let response = move_ticket(
        State(state),
        Extension(RequestIdExt("rid-move-apply".to_string())),
        Path(id),
        Query(MutationWorkspaceParam {
            workspace: workspace.clone(),
        }),
        Json(MoveTicketBody {
            to_workspace_root: target_workspace.to_string_lossy().to_string(),
            dry_run: false,
        }),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), 1024 * 1024)
        .await
        .expect("body");
    let payload: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
    assert_eq!(payload["mode"], "apply");
    assert!(payload["outcome"]["journal"]["id"].is_string());

    assert!(source_store.get_indexed(&id).expect("source lookup").is_none());
    assert!(target_store.get_indexed(&id).expect("target lookup").is_some());
}
