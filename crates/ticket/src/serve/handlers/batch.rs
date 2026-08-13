//! Handler for `POST /api/batch`.

use axum::{
    extract::{
        Extension,
        Json,
        State,
    },
    http::StatusCode,
    response::{
        IntoResponse,
        Response,
    },
};
use chrono::Utc;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::{
    Value,
    json,
};
use std::collections::BTreeMap;
use uuid::Uuid;

use ticket_api::{
    model::edge::EdgeRecord,
    storage::store::TicketStore,
};
use viewer_api::error::RequestIdExt;

use crate::serve::{
    AppState,
    error::task_join_err,
};

/// A single mutation command within a batch request body.
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
        #[serde(default)]
        transition_states: Vec<String>,
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

enum BatchUndoOp {
    Delete {
        id: Uuid,
    },
    RestoreUpdate {
        id: Uuid,
        saved_extra: BTreeMap<String, Value>,
        saved_state: Option<String>,
    },
    RemoveEdge {
        from: Uuid,
        to: Uuid,
        kind: String,
    },
}

/// Apply a single rollback operation, appending any error description to `errors`.
fn apply_batch_undo(
    undo: BatchUndoOp,
    store: &TicketStore,
    errors: &mut Vec<String>,
) {
    match undo {
        BatchUndoOp::Delete { id } =>
            if let Err(e) = store.delete(&id) {
                errors.push(format!("rollback delete {id}: {e}"));
            },
        BatchUndoOp::RestoreUpdate {
            id,
            saved_extra,
            saved_state,
        } => {
            if let Err(e) = store.force_restore(&id, saved_extra, saved_state) {
                errors.push(format!("rollback restore {id}: {e}"));
            }
        },
        BatchUndoOp::RemoveEdge { from, to, kind } => {
            let edge = EdgeRecord {
                from,
                to,
                kind,
                created_at: Utc::now(),
            };
            if let Err(e) = store.remove_edge(edge) {
                errors.push(format!("rollback remove_edge {from}->{to}: {e}"));
            }
        },
    }
}

/// Snapshot the full extra-field map and state of a ticket before mutation.
fn snapshot_ticket(
    store: &TicketStore,
    id: &Uuid,
) -> Option<(BTreeMap<String, Value>, Option<String>)> {
    let indexed = store.get_indexed(id).ok()??;
    let manifest = store.get(id).ok()?;
    Some((manifest.extra, indexed.state))
}

/// Dispatch one `BatchCommand` against the store.
fn dispatch_command(
    cmd: BatchCommand,
    store: &TicketStore,
) -> Result<(Value, Option<BatchUndoOp>), ticket_api::error::StorageError> {
    match cmd {
        BatchCommand::Create {
            type_id,
            title,
            fields,
            description,
        } => {
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
        },

        BatchCommand::Update {
            id,
            fields,
            state,
            transition_states,
        } => {
            let pre = snapshot_ticket(store, &id);
            let manifest = store.update(
                &id,
                fields,
                Some(transition_states.as_slice()),
                state.as_deref(),
                None,
                None,
            )?;
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
            let undo = pre.map(|(saved_extra, saved_state)| {
                BatchUndoOp::RestoreUpdate {
                    id,
                    saved_extra,
                    saved_state,
                }
            });
            Ok((result, undo))
        },

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
            let undo = pre.map(|(saved_extra, saved_state)| {
                BatchUndoOp::RestoreUpdate {
                    id,
                    saved_extra,
                    saved_state,
                }
            });
            Ok((result, undo))
        },

        BatchCommand::Cancel { id, reason } => {
            let pre = snapshot_ticket(store, &id);
            let mut patch = BTreeMap::new();
            if let Some(r) = reason {
                patch.insert("cancel_reason".to_string(), Value::String(r));
            }
            let manifest = store.update(
                &id,
                patch,
                Some(&[]),
                Some("cancelled"),
                None,
                None,
            )?;
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
            let undo = pre.map(|(saved_extra, saved_state)| {
                BatchUndoOp::RestoreUpdate {
                    id,
                    saved_extra,
                    saved_state,
                }
            });
            Ok((result, undo))
        },

        BatchCommand::Link { from, to, kind } => {
            let edge = EdgeRecord {
                from,
                to,
                kind: kind.clone(),
                created_at: Utc::now(),
            };
            store.add_edge(edge)?;
            let result = json!({
                "op": "link",
                "from": from.to_string(),
                "to": to.to_string(),
                "kind": kind.clone(),
            });
            Ok((result, Some(BatchUndoOp::RemoveEdge { from, to, kind })))
        },

        BatchCommand::Unlink { from, to, kind } => {
            let edge = EdgeRecord {
                from,
                to,
                kind: kind.clone(),
                created_at: Utc::now(),
            };
            store.remove_edge(edge)?;
            let result = json!({
                "op": "unlink",
                "from": from.to_string(),
                "to": to.to_string(),
                "kind": kind,
            });
            // Unlink has no rollback entry (matches CLI batch behaviour).
            Ok((result, None))
        },
    }
}

/// `POST /api/batch`
pub async fn batch_tickets(
    State(state): State<AppState>,
    Extension(rid): Extension<RequestIdExt>,
    Json(body): Json<BatchBody>,
) -> Response {
    let (workspace, store) =
        match state.resolve_public_workspace_request(&body.workspace, &rid.0) {
            Ok(resolved) => resolved,
            Err(response) => return response,
        };
    let commands = body.commands;
    let total = commands.len();
    let request_id = rid.0.clone();
    let task_request_id = request_id.clone();

    tokio::task::spawn_blocking(move || {
        let mut results: Vec<Value> = Vec::with_capacity(total);
        let mut undo_stack: Vec<BatchUndoOp> = Vec::with_capacity(total);

        for (index, cmd) in commands.into_iter().enumerate() {
            match dispatch_command(cmd, &store) {
                Ok((mut result, undo)) => {
                    result["index"] = json!(index);
                    result["status"] = json!("ok");
                    if let Some(u) = undo {
                        undo_stack.push(u);
                    }
                    results.push(result);
                },
                Err(e) => {
                    let mut rollback_errors: Vec<String> = Vec::new();
                    for undo in undo_stack.into_iter().rev() {
                        apply_batch_undo(undo, &store, &mut rollback_errors);
                    }
                    return (
                        StatusCode::UNPROCESSABLE_ENTITY,
                        Json(json!({
                            "request_id": task_request_id.clone(),
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
                },
            }
        }

        Json(BatchResponse {
            request_id: task_request_id.clone(),
            workspace,
            status: "ok",
            count: results.len(),
            results,
        })
        .into_response()
    })
    .await
    .unwrap_or_else(|_| task_join_err(&request_id, "batch request"))
}

#[cfg(test)]
#[path = "batch/tests.rs"]
mod tests;
