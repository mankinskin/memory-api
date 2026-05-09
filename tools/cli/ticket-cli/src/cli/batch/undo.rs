use std::collections::BTreeMap;

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use ticket_api::model::edge::EdgeRecord;
use ticket_api::storage::TicketStore;

use super::super::commands;
use super::super::{BoardArgs, BoardCommand, TicketCommandCli};

#[derive(Debug)]
pub(super) enum BatchUndoOp {
    Delete { id: Uuid },
    RestoreUpdate {
        id: Uuid,
        saved_extra: BTreeMap<String, Value>,
        saved_state: Option<String>,
    },
    RemoveEdge { from: Uuid, to: Uuid, kind: String },
    BoardCheckOut { ticket_id: Uuid, agent_id: String },
    BoardCheckIn { ticket_id: Uuid, agent_id: String },
}

pub(super) enum BatchUndoContext {
    Create,
    Update {
        id: Uuid,
        saved_extra: BTreeMap<String, Value>,
        saved_state: Option<String>,
    },
    Link,
    BoardCheckIn { ticket_ref: String, agent_id: String },
    BoardCheckOut { ticket_ref: String, agent_id: Option<String> },
}

pub(super) fn apply_batch_undo(undo: BatchUndoOp, store: &TicketStore, errors: &mut Vec<String>) {
    match undo {
        BatchUndoOp::Delete { id } => {
            if let Err(e) = store.delete(&id) {
                errors.push(format!("rollback delete {id}: {e}"));
            }
        }
        BatchUndoOp::RestoreUpdate {
            id,
            saved_extra,
            saved_state,
        } => {
            if let Err(e) = store.force_restore(&id, saved_extra, saved_state) {
                errors.push(format!("rollback restore {id}: {e}"));
            }
        }
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
        }
        BatchUndoOp::BoardCheckOut { ticket_id, agent_id } => {
            if let Err(e) = store.board_check_out(&ticket_id, &agent_id, Some("batch rollback")) {
                errors.push(format!("rollback board_check_out {ticket_id}/{agent_id}: {e}"));
            }
        }
        BatchUndoOp::BoardCheckIn { ticket_id, agent_id } => {
            if let Err(e) = store.board_check_in(&ticket_id, &agent_id, 3600, "batch rollback", vec![]) {
                errors.push(format!("rollback board_check_in {ticket_id}/{agent_id}: {e}"));
            }
        }
    }
}

pub(super) fn capture_batch_undo_context(
    cmd: &TicketCommandCli,
    store: &TicketStore,
) -> Option<BatchUndoContext> {
    match cmd {
        TicketCommandCli::Create(_) => Some(BatchUndoContext::Create),
        TicketCommandCli::Update(args) => capture_update_undo_context(&args.id, store),
        TicketCommandCli::Link(_) => Some(BatchUndoContext::Link),
        TicketCommandCli::Board(BoardArgs { command: BoardCommand::CheckIn { id, agent, .. } }) => {
            Some(BatchUndoContext::BoardCheckIn {
                ticket_ref: id.clone(),
                agent_id: agent.clone(),
            })
        }
        TicketCommandCli::Board(BoardArgs { command: BoardCommand::CheckOut { id, agent, .. } }) => {
            Some(BatchUndoContext::BoardCheckOut {
                ticket_ref: id.clone(),
                agent_id: agent.clone(),
            })
        }
        _ => None,
    }
}

pub(super) fn batch_undo_from_result(
    context: BatchUndoContext,
    result: &Value,
    store: &TicketStore,
) -> Option<BatchUndoOp> {
    match context {
        BatchUndoContext::Create => create_undo_from_result(result),
        BatchUndoContext::Update {
            id,
            saved_extra,
            saved_state,
        } => Some(BatchUndoOp::RestoreUpdate {
            id,
            saved_extra,
            saved_state,
        }),
        BatchUndoContext::Link => link_undo_from_result(result),
        BatchUndoContext::BoardCheckIn {
            ticket_ref,
            agent_id,
        } => board_check_in_undo(result, &ticket_ref, agent_id, store),
        BatchUndoContext::BoardCheckOut {
            ticket_ref,
            agent_id,
        } => board_check_out_undo(result, &ticket_ref, agent_id, store),
    }
}

fn capture_update_undo_context(id: &str, store: &TicketStore) -> Option<BatchUndoContext> {
    let ticket_id = commands::resolve_uuid_prefix(id, store).ok()?;
    let indexed = store.get_indexed(&ticket_id).ok().flatten()?;

    let mut saved_extra = BTreeMap::new();
    if let Some(title) = &indexed.title {
        saved_extra.insert("title".to_string(), Value::String(title.clone()));
    }

    Some(BatchUndoContext::Update {
        id: ticket_id,
        saved_extra,
        saved_state: indexed.state.clone(),
    })
}

fn create_undo_from_result(result: &Value) -> Option<BatchUndoOp> {
    result_uuid(result, "id").map(|id| BatchUndoOp::Delete { id })
}

fn link_undo_from_result(result: &Value) -> Option<BatchUndoOp> {
    let from = result_uuid(result, "from")?;
    let to = result_uuid(result, "to")?;
    let kind = result_string(result, "kind")?;
    Some(BatchUndoOp::RemoveEdge { from, to, kind })
}

fn board_check_in_undo(
    result: &Value,
    ticket_ref: &str,
    agent_id: String,
    store: &TicketStore,
) -> Option<BatchUndoOp> {
    if let Some(ticket_id) = result_uuid(result, "ticket_id") {
        let resolved_agent = result_string(result, "agent_id").unwrap_or(agent_id);
        return Some(BatchUndoOp::BoardCheckOut {
            ticket_id,
            agent_id: resolved_agent,
        });
    }

    commands::resolve_uuid_prefix(ticket_ref, store)
        .ok()
        .map(|ticket_id| BatchUndoOp::BoardCheckOut { ticket_id, agent_id })
}

fn board_check_out_undo(
    result: &Value,
    ticket_ref: &str,
    agent_id: Option<String>,
    store: &TicketStore,
) -> Option<BatchUndoOp> {
    if let Some(ticket_id) = result_uuid(result, "ticket_id") {
        let resolved_agent = result_string(result, "agent_id").or(agent_id)?;
        return Some(BatchUndoOp::BoardCheckIn {
            ticket_id,
            agent_id: resolved_agent,
        });
    }

    let ticket_id = commands::resolve_uuid_prefix(ticket_ref, store).ok()?;
    agent_id.map(|agent_id| BatchUndoOp::BoardCheckIn { ticket_id, agent_id })
}

fn result_uuid(result: &Value, field: &str) -> Option<Uuid> {
    result.get(field)?.as_str()?.parse().ok()
}

fn result_string(result: &Value, field: &str) -> Option<String> {
    result.get(field)?.as_str().map(str::to_string)
}