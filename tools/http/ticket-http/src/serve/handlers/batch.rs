//! Handler for `POST /api/batch`.
//!
//! Executes a sequence of ticket mutation commands transactionally: on any
//! failure, all previously-applied writes are rolled back before returning.
//! Reuses the same underlying `TicketStore` operations used by the CLI
//! `ticket batch` command.

use axum::{
    extract::{Extension, Json, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use uuid::Uuid;

use ticket_api::model::edge::EdgeRecord;
use ticket_api::storage::store::TicketStore;
use viewer_api::error::RequestIdExt;

use crate::serve::AppState;

// ── Request / response types ──────────────────────────────────────────────────

/// A single mutation command within a batch request body.
///
/// Discriminated by the `"op"` field:
/// - `"create"` — create a new ticket
/// - `"update"` — patch fields or state of an existing ticket
/// - `"close"`  — fast-forward a ticket to a terminal state (default `"done"`)
/// - `"cancel"` — transition a ticket to `"cancelled"`
/// - `"link"`   — add a directed edge between two tickets
/// - `"unlink"` — remove a directed edge between two tickets
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum BatchCommand {
    Create {
        #[serde(rename = "type")]
        type_id: String,
        title: Option<String>,
        #[serde(default)]
        fields: BTreeMap<String, Value>,
        description: Option<String>,
    },
    Update {
        id: Uuid,
        #[serde(default)]
        fields: BTreeMap<String, Value>,
        state: Option<String>,
        from_state: Option<String>,
    },
    Close {
        id: Uuid,
        target_state: Option<String>,
    },
    Cancel {
        id: Uuid,
        reason: Option<String>,
    },
    Link {
        from: Uuid,
        to: Uuid,
        kind: String,
    },
    Unlink {
        from: Uuid,
        to: Uuid,
        kind: String,
    },
}

/// Request body for `POST /api/batch`.
#[derive(Deserialize)]
pub struct BatchBody {
    workspace: String,
    commands: Vec<BatchCommand>,
}

/// Response body returned on full success.
#[derive(Serialize)]
pub struct BatchResponse {
    pub request_id: String,
    pub workspace: String,
    pub status: &'static str,
    pub count: usize,
    pub results: Vec<Value>,
}

// ── Undo infrastructure ───────────────────────────────────────────────────────

/// Rollback operation pushed onto the undo stack after each successful command.
enum BatchUndoOp {
    /// Delete a newly-created ticket.
    Delete { id: Uuid },
    /// Restore a ticket's fields and state to a saved snapshot.
    RestoreUpdate {
        id: Uuid,
        saved_extra: BTreeMap<String, Value>,
        saved_state: Option<String>,
    },
    /// Remove an edge that was added by a `link` command.
    RemoveEdge { from: Uuid, to: Uuid, kind: String },
}

/// Apply a single rollback operation, appending any error description to `errors`.
fn apply_batch_undo(undo: BatchUndoOp, store: &TicketStore, errors: &mut Vec<String>) {
    match undo {
        BatchUndoOp::Delete { id } => {
            if let Err(e) = store.delete(&id) {
                errors.push(format!("rollback delete {id}: {e}"));
            }
        }
        BatchUndoOp::RestoreUpdate { id, saved_extra, saved_state } => {
            if let Err(e) = store.force_restore(&id, saved_extra, saved_state) {
                errors.push(format!("rollback restore {id}: {e}"));
            }
        }
        BatchUndoOp::RemoveEdge { from, to, kind } => {
            let edge = EdgeRecord { from, to, kind, created_at: Utc::now() };
            if let Err(e) = store.remove_edge(edge) {
                errors.push(format!("rollback remove_edge {from}->{to}: {e}"));
            }
        }
    }
}

// ── Dispatch ──────────────────────────────────────────────────────────────────

/// Snapshot the full extra-field map and state of a ticket before mutation.
///
/// Returns `None` if the ticket does not exist or any read fails (best-effort;
/// rollback may be incomplete if this pre-capture fails).
fn snapshot_ticket(
    store: &TicketStore,
    id: &Uuid,
) -> Option<(BTreeMap<String, Value>, Option<String>)> {
    let indexed = store.get_indexed(id).ok()??;
    let manifest = store.get(id).ok()?;
    Some((manifest.extra, indexed.state))
}

/// Dispatch one `BatchCommand` against the store.
///
/// Returns `(result_json, optional_undo_op)`.  The caller is responsible for
/// pushing the undo op onto the undo stack and collecting results.
fn dispatch_command(
    cmd: BatchCommand,
    store: &TicketStore,
) -> Result<(Value, Option<BatchUndoOp>), ticket_api::error::StorageError> {
    match cmd {
        BatchCommand::Create { type_id, title, fields, description } => {
            let id = store.create(
                None,
                &type_id,
                title.as_deref(),
                None,
                fields,
                None,
                description.as_deref(),
            )?;
            let manifest = store.get(&id)?;
            let created_at = store
                .get_indexed(&id)
                .ok()
                .flatten()
                .map(|t| t.created_at)
                .unwrap_or_else(Utc::now);
            let result = json!({
                "op": "create",
                "id": id.to_string(),
                "created_at": created_at,
                "fields": manifest.extra,
            });
            Ok((result, Some(BatchUndoOp::Delete { id })))
        }

        BatchCommand::Update { id, fields, state, from_state } => {
            let pre = snapshot_ticket(store, &id);
            let manifest =
                store.update(&id, fields, from_state.as_deref(), state.as_deref(), None, None)?;
            let created_at = store
                .get_indexed(&id)
                .ok()
                .flatten()
                .map(|t| t.created_at)
                .unwrap_or_else(Utc::now);
            let result = json!({
                "op": "update",
                "id": id.to_string(),
                "created_at": created_at,
                "fields": manifest.extra,
            });
            let undo = pre.map(|(saved_extra, saved_state)| BatchUndoOp::RestoreUpdate {
                id,
                saved_extra,
                saved_state,
            });
            Ok((result, undo))
        }

        BatchCommand::Close { id, target_state } => {
            let pre = snapshot_ticket(store, &id);
            let target = target_state.as_deref().unwrap_or("done");
            let (manifest, _path) = store.close(&id, target, None)?;
            let created_at = store
                .get_indexed(&id)
                .ok()
                .flatten()
                .map(|t| t.created_at)
                .unwrap_or_else(Utc::now);
            let result = json!({
                "op": "close",
                "id": id.to_string(),
                "created_at": created_at,
                "fields": manifest.extra,
            });
            let undo = pre.map(|(saved_extra, saved_state)| BatchUndoOp::RestoreUpdate {
                id,
                saved_extra,
                saved_state,
            });
            Ok((result, undo))
        }

        BatchCommand::Cancel { id, reason } => {
            let pre = snapshot_ticket(store, &id);
            let mut patch = BTreeMap::new();
            if let Some(r) = reason {
                patch.insert("cancel_reason".to_string(), Value::String(r));
            }
            let manifest =
                store.update(&id, patch, None, Some("cancelled"), None, None)?;
            let created_at = store
                .get_indexed(&id)
                .ok()
                .flatten()
                .map(|t| t.created_at)
                .unwrap_or_else(Utc::now);
            let result = json!({
                "op": "cancel",
                "id": id.to_string(),
                "created_at": created_at,
                "fields": manifest.extra,
            });
            let undo = pre.map(|(saved_extra, saved_state)| BatchUndoOp::RestoreUpdate {
                id,
                saved_extra,
                saved_state,
            });
            Ok((result, undo))
        }

        BatchCommand::Link { from, to, kind } => {
            let edge = EdgeRecord { from, to, kind: kind.clone(), created_at: Utc::now() };
            store.add_edge(edge)?;
            let result = json!({
                "op": "link",
                "from": from.to_string(),
                "to": to.to_string(),
                "kind": kind.clone(),
            });
            Ok((result, Some(BatchUndoOp::RemoveEdge { from, to, kind })))
        }

        BatchCommand::Unlink { from, to, kind } => {
            let edge = EdgeRecord { from, to, kind: kind.clone(), created_at: Utc::now() };
            store.remove_edge(edge)?;
            let result = json!({
                "op": "unlink",
                "from": from.to_string(),
                "to": to.to_string(),
                "kind": kind,
            });
            // Unlink has no rollback entry (matches CLI batch behaviour).
            Ok((result, None))
        }
    }
}

// ── Handler ───────────────────────────────────────────────────────────────────

/// `POST /api/batch`
///
/// Execute a batch of mutation commands transactionally against a single
/// workspace.  On any failure, all prior writes are rolled back before
/// returning a `422 Unprocessable Entity` response that includes:
/// - `"failed_at"` — zero-based index of the failing command
/// - `"error"` — human-readable error description
/// - `"rolled_back"` — `true` if all rollbacks succeeded
/// - `"rollback_errors"` — list of rollback failure messages (normally empty)
/// - `"results"` — outcomes of commands that ran before the failure
///
/// On success, returns `200 OK` with `{"status": "ok", "results": [...]}`.
pub async fn batch_tickets(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Json(body): Json<BatchBody>,
) -> Response {
    let store = match state.ensure_workspace_runtime(&body.workspace) {
        Some(s) => s,
        None => {
            return viewer_api::error::ApiError::not_found("workspace", &rid.0)
                .into_response_with_status(StatusCode::NOT_FOUND);
        }
    };

    let workspace = body.workspace.clone();
    let total = body.commands.len();
    let mut results: Vec<Value> = Vec::with_capacity(total);
    let mut undo_stack: Vec<BatchUndoOp> = Vec::with_capacity(total);

    for (index, cmd) in body.commands.into_iter().enumerate() {
        match dispatch_command(cmd, &store) {
            Ok((mut result, undo)) => {
                result["index"] = json!(index);
                result["status"] = json!("ok");
                if let Some(u) = undo {
                    undo_stack.push(u);
                }
                results.push(result);
            }
            Err(e) => {
                let mut rollback_errors: Vec<String> = Vec::new();
                for undo in undo_stack.into_iter().rev() {
                    apply_batch_undo(undo, &store, &mut rollback_errors);
                }
                return (
                    StatusCode::UNPROCESSABLE_ENTITY,
                    Json(json!({
                        "request_id": rid.0,
                        "workspace": workspace,
                        "status": "error",
                        "failed_at": index,
                        "error": e.to_string(),
                        "completed": results.len(),
                        "total": total,
                        "rolled_back": rollback_errors.is_empty(),
                        "rollback_errors": rollback_errors,
                        "results": results,
                    })),
                )
                    .into_response();
            }
        }
    }

    Json(BatchResponse {
        request_id: rid.0,
        workspace,
        status: "ok",
        count: results.len(),
        results,
    })
    .into_response()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serve::{AppState, StreamBroker, WorkspaceRegistry};
    use axum::{
        body::{Body, to_bytes},
        http::{Method, Request, StatusCode, header},
    };
    use std::sync::Arc;
    use ticket_api::{model::filesystem::ScanRoot, storage::store::TicketStore};
    use tower::ServiceExt;

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
        crate::serve::routes::build_router(state)
    }

    async fn post_batch(app: axum::Router, body: serde_json::Value) -> (StatusCode, Value) {
        let req = Request::builder()
            .method(Method::POST)
            .uri("/api/batch")
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = app.oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: Value = serde_json::from_slice(&bytes).unwrap();
        (status, json)
    }

    #[tokio::test]
    async fn batch_create_returns_ok() {
        let dir = tempfile::tempdir().unwrap();
        let app = make_router(dir.path());

        let (status, resp) = post_batch(
            app,
            json!({
                "workspace": "default",
                "commands": [
                    {"op": "create", "type": "tracker-improvement", "title": "Batch A"},
                    {"op": "create", "type": "tracker-improvement", "title": "Batch B"},
                ]
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "expected 200, got: {resp}");
        assert_eq!(resp["status"], "ok");
        assert_eq!(resp["count"], 2);
        let results = resp["results"].as_array().unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["op"], "create");
        assert_eq!(results[0]["index"], 0);
        assert_eq!(results[1]["op"], "create");
        assert_eq!(results[1]["index"], 1);
    }

    #[tokio::test]
    async fn batch_rolls_back_on_failure() {
        let dir = tempfile::tempdir().unwrap();
        let app = make_router(dir.path());

        // Command 0 succeeds (create), command 1 fails (invalid type uuid).
        let (status, resp) = post_batch(
            app,
            json!({
                "workspace": "default",
                "commands": [
                    {"op": "create", "type": "tracker-improvement", "title": "Should be rolled back"},
                    // Closing a nonexistent ticket ID will fail.
                    {"op": "close", "id": "00000000-0000-0000-0000-000000000000"},
                ]
            }),
        )
        .await;

        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "expected 422, got: {resp}");
        assert_eq!(resp["status"], "error");
        assert_eq!(resp["failed_at"], 1);
        assert_eq!(
            resp["rolled_back"].as_bool().unwrap_or(false),
            true,
            "rollback must succeed"
        );
    }

    #[tokio::test]
    async fn batch_link_and_unlink() {
        let dir = tempfile::tempdir().unwrap();
        let app = make_router(dir.path());

        // First create two tickets via the store directly.
        let store =
            Arc::new(TicketStore::open(dir.path()).expect("open store"));
        store
            .add_scan_root(ScanRoot {
                path: dir.path().join("tickets"),
                label: "default".into(),
            })
            .expect("add scan root");
        let id_a = store
            .create(None, "tracker-improvement", Some("A"), None, Default::default(), None, None)
            .unwrap();
        let id_b = store
            .create(None, "tracker-improvement", Some("B"), None, Default::default(), None, None)
            .unwrap();

        let app = make_router(dir.path());

        // Link A -> B, then unlink.
        let (status, resp) = post_batch(
            app,
            json!({
                "workspace": "default",
                "commands": [
                    {"op": "link", "from": id_a.to_string(), "to": id_b.to_string(), "kind": "depends_on"},
                    {"op": "unlink", "from": id_a.to_string(), "to": id_b.to_string(), "kind": "depends_on"},
                ]
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK, "expected 200, got: {resp}");
        assert_eq!(resp["status"], "ok");
        assert_eq!(resp["count"], 2);
    }

    #[tokio::test]
    async fn batch_unknown_workspace_returns_404() {
        let dir = tempfile::tempdir().unwrap();
        let app = make_router(dir.path());

        let (status, _) = post_batch(
            app,
            json!({
                "workspace": "nonexistent",
                "commands": [
                    {"op": "create", "type": "tracker-improvement", "title": "x"},
                ]
            }),
        )
        .await;

        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn batch_empty_commands_returns_ok() {
        let dir = tempfile::tempdir().unwrap();
        let app = make_router(dir.path());

        let (status, resp) = post_batch(
            app,
            json!({
                "workspace": "default",
                "commands": []
            }),
        )
        .await;

        assert_eq!(status, StatusCode::OK);
        assert_eq!(resp["status"], "ok");
        assert_eq!(resp["count"], 0);
    }
}
