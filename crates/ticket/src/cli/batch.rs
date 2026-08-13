use serde_json::{
    Value,
    json,
};

use ticket_api::{
    error::StorageError,
    storage::TicketStore,
};

use super::*;

#[path = "batch/dispatch.rs"]
mod batch_dispatch_impl;
#[path = "batch/undo.rs"]
mod batch_undo_impl;

use self::{
    batch_dispatch_impl::batch_dispatch,
    batch_undo_impl::{
        BatchUndoOp,
        apply_batch_undo,
        batch_undo_from_result,
        capture_batch_undo_context,
    },
};

// ── CLI-syntax batch ─────────────────────────────────────────────────────────

#[derive(clap::Parser)]
#[command(name = "ticket")]
struct BatchLineParser {
    #[command(subcommand)]
    command: TicketCommandCli,
}

fn parse_batch_line(line: &str) -> Result<TicketCommandCli, CliRunError> {
    let mut tokens = shell_words::split(line).map_err(|e| {
        CliRunError::InvalidExecPayload(format!("cannot parse line: {e}"))
    })?;
    if tokens.is_empty() {
        return Err(CliRunError::InvalidExecPayload(
            "empty command line".to_string(),
        ));
    }
    tokens.insert(0, "ticket".to_string());
    BatchLineParser::try_parse_from(tokens)
        .map(|p| p.command)
        .map_err(|e| CliRunError::InvalidExecPayload(format!("{e}")))
}

fn read_cli_batch_commands(
    file: Option<std::path::PathBuf>
) -> Result<Vec<TicketCommandCli>, CliRunError> {
    use std::{
        fs::File,
        io::{
            self,
            BufRead,
            BufReader,
        },
    };

    let lines: Vec<String> = if let Some(path) = file {
        let f = File::open(&path).map_err(|e| {
            CliRunError::InvalidExecPayload(format!(
                "cannot open batch file {}: {e}",
                path.display()
            ))
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
        let cmd = parse_batch_line(trimmed).map_err(|e| {
            CliRunError::InvalidExecPayload(format!("line {}: {e}", i + 1))
        })?;
        cmds.push(cmd);
    }
    Ok(cmds)
}

fn execute_cli_batch(
    cmds: Vec<TicketCommandCli>,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let total = cmds.len();
    let mut results: Vec<Value> = Vec::with_capacity(total);
    let mut undo_stack: Vec<BatchUndoOp> = Vec::with_capacity(total);

    for cmd in cmds {
        let undo_context = capture_batch_undo_context(&cmd, store);

        match batch_dispatch(cmd, store) {
            Ok(result) => {
                if let Some(undo) = undo_context.and_then(|context| {
                    batch_undo_from_result(context, &result, store)
                }) {
                    undo_stack.push(undo);
                }
                results.push(result);
            },
            Err(e) => {
                return Ok(batch_error_response(
                    results.len(),
                    total,
                    e,
                    undo_stack,
                    store,
                ));
            },
        }
    }

    Ok(batch_success_response(results))
}

pub(crate) fn cmd_batch(
    args: BatchArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let cmds = read_cli_batch_commands(args.file)?;
    if cmds.is_empty() {
        return Ok(batch_success_response(Vec::new()));
    }
    execute_cli_batch(cmds, store)
}

fn batch_success_response(results: Vec<Value>) -> Value {
    json!({
        "command": "batch",
        "status": "ok",
        "count": results.len(),
        "results": results,
    })
}

fn batch_error_response(
    completed: usize,
    total: usize,
    error: CliRunError,
    undo_stack: Vec<BatchUndoOp>,
    store: &TicketStore,
) -> Value {
    let rollback_errors = rollback_batch(undo_stack, store);
    json!({
        "command": "batch",
        "status": "error",
        "completed": completed,
        "total": total,
        "error": error.to_string(),
        "rolled_back": rollback_errors.is_empty(),
        "rollback_errors": rollback_errors,
    })
}

fn rollback_batch(
    undo_stack: Vec<BatchUndoOp>,
    store: &TicketStore,
) -> Vec<String> {
    let mut rollback_errors = Vec::new();
    for undo in undo_stack.into_iter().rev() {
        apply_batch_undo(undo, store, &mut rollback_errors);
    }
    rollback_errors
}
