use std::collections::BTreeMap;

use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

use ticket_api::error::StorageError;
use ticket_api::model::edge::EdgeRecord;
use ticket_api::storage::TicketStore;

use super::commands;
use super::*;

#[derive(Debug)]
enum BatchUndoOp {
    Delete { id: Uuid },
    RestoreUpdate {
        id: Uuid,
        saved_extra: BTreeMap<String, Value>,
        saved_state: Option<String>,
    },
    RemoveEdge { from: Uuid, to: Uuid, kind: String },
}

fn apply_batch_undo(undo: BatchUndoOp, store: &TicketStore, errors: &mut Vec<String>) {
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
    }
}

// ── CLI-syntax batch ─────────────────────────────────────────────────────────

#[derive(clap::Parser)]
#[command(name = "ticket")]
struct BatchLineParser {
    #[command(subcommand)]
    command: TicketCommandCli,
}

fn parse_batch_line(line: &str) -> Result<TicketCommandCli, CliRunError> {
    let mut tokens = shell_words::split(line)
        .map_err(|e| CliRunError::InvalidExecPayload(format!("cannot parse line: {e}")))?;
    if tokens.is_empty() {
        return Err(CliRunError::InvalidExecPayload("empty command line".to_string()));
    }
    tokens.insert(0, "ticket".to_string());
    BatchLineParser::try_parse_from(tokens)
        .map(|p| p.command)
        .map_err(|e| CliRunError::InvalidExecPayload(format!("{e}")))
}

fn read_cli_batch_commands(file: Option<std::path::PathBuf>) -> Result<Vec<TicketCommandCli>, CliRunError> {
    use std::fs::File;
    use std::io::{self, BufRead, BufReader};

    let lines: Vec<String> = if let Some(path) = file {
        let f = File::open(&path).map_err(|e| {
            CliRunError::InvalidExecPayload(format!("cannot open batch file {}: {e}", path.display()))
        })?;
        BufReader::new(f)
            .lines()
            .collect::<io::Result<_>>()
            .map_err(StorageError::Io)?
    } else {
        let stdin = io::stdin();
        stdin
            .lock()
            .lines()
            .collect::<io::Result<_>>()
            .map_err(StorageError::Io)?
    };

    let mut cmds = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let cmd = parse_batch_line(trimmed)
            .map_err(|e| CliRunError::InvalidExecPayload(format!("line {}: {e}", i + 1)))?;
        cmds.push(cmd);
    }
    Ok(cmds)
}

fn execute_cli_batch(cmds: Vec<TicketCommandCli>, store: &TicketStore) -> Result<Value, CliRunError> {
    let total = cmds.len();
    let mut results: Vec<Value> = Vec::with_capacity(total);
    let mut undo_stack: Vec<BatchUndoOp> = Vec::with_capacity(total);

    for cmd in cmds {
        let is_create = matches!(cmd, TicketCommandCli::Create(_));
        let is_link = matches!(cmd, TicketCommandCli::Link(_));
        let update_pre: Option<(Uuid, BTreeMap<String, Value>, Option<String>)> =
            if let TicketCommandCli::Update(ref args) = cmd {
                commands::resolve_uuid_prefix(&args.id, store)
                    .ok()
                    .and_then(|id| {
                        store.get_indexed(&id).ok().flatten().map(|indexed| {
                            let mut saved = BTreeMap::new();
                            if let Some(t) = &indexed.title {
                                saved.insert("title".to_string(), Value::String(t.clone()));
                            }
                            (id, saved, indexed.state.clone())
                        })
                    })
            } else {
                None
            };

        match batch_dispatch(cmd, store) {
            Ok(result) => {
                let undo = if is_create {
                    result
                        .get("id")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok())
                        .map(|id| BatchUndoOp::Delete { id })
                } else if let Some((id, saved_extra, saved_state)) = update_pre {
                    Some(BatchUndoOp::RestoreUpdate { id, saved_extra, saved_state })
                } else if is_link {
                    let from = result
                        .get("from")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok());
                    let to = result
                        .get("to")
                        .and_then(|v| v.as_str())
                        .and_then(|s| s.parse().ok());
                    let kind = result
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    match (from, to, kind) {
                        (Some(from), Some(to), Some(kind)) => {
                            Some(BatchUndoOp::RemoveEdge { from, to, kind })
                        }
                        _ => None,
                    }
                } else {
                    None
                };
                if let Some(u) = undo {
                    undo_stack.push(u);
                }
                results.push(result);
            }
            Err(e) => {
                let mut rollback_errors: Vec<String> = Vec::new();
                for undo in undo_stack.into_iter().rev() {
                    apply_batch_undo(undo, store, &mut rollback_errors);
                }
                return Ok(json!({
                    "command": "batch",
                    "status": "error",
                    "completed": results.len(),
                    "total": total,
                    "error": e.to_string(),
                    "rolled_back": rollback_errors.is_empty(),
                    "rollback_errors": rollback_errors,
                }));
            }
        }
    }

    Ok(json!({
        "command": "batch",
        "status": "ok",
        "count": results.len(),
        "results": results,
    }))
}

fn batch_dispatch(cmd: TicketCommandCli, store: &TicketStore) -> Result<Value, CliRunError> {
    match cmd {
        TicketCommandCli::Create(args) => commands::cmd_create(args, store),
        TicketCommandCli::Get(args) => commands::cmd_get(args, store),
        TicketCommandCli::Describe(args) => commands::cmd_describe(args, store),
        TicketCommandCli::Update(args) => commands::cmd_update(args, store),
        TicketCommandCli::Repro(args) => commands::cmd_repro(args, store),
        TicketCommandCli::List(args) => commands::cmd_list(args, store),
        TicketCommandCli::Delete(args) => commands::cmd_delete(args, store),
        TicketCommandCli::Link(args) => commands::cmd_link(args, store),
        TicketCommandCli::Unlink(args) => commands::cmd_unlink(args, store),
        TicketCommandCli::Links(args) => commands::cmd_links(args, store),
        TicketCommandCli::Subgraph(args) => commands::cmd_subgraph(args, store),
        TicketCommandCli::Topgraph(args) => commands::cmd_topgraph(args, store),
        TicketCommandCli::Search(args) | TicketCommandCli::Query(args) => {
            commands::cmd_search(args, store)
        }
        TicketCommandCli::Health(args) => commands::cmd_health(args, store),
        TicketCommandCli::Close(args) => commands::cmd_close(args, store),
        TicketCommandCli::Cancel(args) => commands::cmd_cancel(args, store),
        TicketCommandCli::Status(args) => commands::cmd_status(args, store),
        TicketCommandCli::ReadyOverview(args) => commands::cmd_ready_overview(args, store),
        TicketCommandCli::Next(args) => commands::cmd_next(args, store),
        TicketCommandCli::Attach(args) => commands::cmd_attach(args, store),
        TicketCommandCli::Assets(args) => commands::cmd_assets(args, store),
        TicketCommandCli::History(args) => commands::cmd_history(args, store),
        TicketCommandCli::Diff(args) => commands::cmd_diff(args, store),
        TicketCommandCli::Revert(args) => commands::cmd_revert(args, store),
        TicketCommandCli::Audit => commands::cmd_audit(store),
        TicketCommandCli::Fmt(args) => commands::cmd_fmt(args, store),
        TicketCommandCli::Serve(_) => {
            Err(CliRunError::BadRequest("'serve' cannot be used in a batch".to_string()))
        }
        TicketCommandCli::Watch(_) => {
            Err(CliRunError::BadRequest("'watch' cannot be used in a batch".to_string()))
        }
        TicketCommandCli::Batch(_) => {
            Err(CliRunError::BadRequest("'batch' cannot be nested".to_string()))
        }
        TicketCommandCli::Scan(_) => {
            Err(CliRunError::BadRequest("'scan' cannot be used in a batch".to_string()))
        }
        TicketCommandCli::Claim(_) | TicketCommandCli::Unclaim(_) | TicketCommandCli::Leases => {
            Err(CliRunError::BadRequest(
                "lease commands cannot be used in a batch".to_string(),
            ))
        }
        TicketCommandCli::AddRoot(_) => {
            Err(CliRunError::BadRequest("'add-root' cannot be used in a batch".to_string()))
        }
        TicketCommandCli::ExportCommandSchema => Err(CliRunError::BadRequest(
            "'export-command-schema' cannot be used in a batch".to_string(),
        )),
        TicketCommandCli::Workspace(_) => {
            Err(CliRunError::BadRequest("'workspace' cannot be used in a batch".to_string()))
        }
        TicketCommandCli::FinalizeMerge(_) => Err(CliRunError::BadRequest(
            "'finalize-merge' is not supported in a batch".to_string(),
        )),
    }
}

pub(crate) fn cmd_batch(args: BatchArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let cmds = read_cli_batch_commands(args.file)?;
    if cmds.is_empty() {
        return Ok(json!({
            "command": "batch",
            "status": "ok",
            "count": 0,
            "results": [],
        }));
    }
    execute_cli_batch(cmds, store)
}
