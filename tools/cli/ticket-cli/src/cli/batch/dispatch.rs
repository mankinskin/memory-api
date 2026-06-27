use serde_json::Value;

use ticket_api::storage::TicketStore;

use super::super::{
    CliRunError,
    TicketCommandCli,
    commands,
};

pub(super) fn batch_dispatch(
    cmd: TicketCommandCli,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    match cmd {
        TicketCommandCli::Create(_)
        | TicketCommandCli::Get(_)
        | TicketCommandCli::Describe(_)
        | TicketCommandCli::Update(_)
        | TicketCommandCli::Repro(_)
        | TicketCommandCli::List(_)
        | TicketCommandCli::Delete(_)
        | TicketCommandCli::Link(_)
        | TicketCommandCli::Unlink(_)
        | TicketCommandCli::Links(_) => batch_dispatch_core(cmd, store),
        TicketCommandCli::Subgraph(_)
        | TicketCommandCli::Topgraph(_)
        | TicketCommandCli::Search(_)
        | TicketCommandCli::Query(_)
        | TicketCommandCli::Health(_)
        | TicketCommandCli::Close(_)
        | TicketCommandCli::Cancel(_)
        | TicketCommandCli::Status(_)
        | TicketCommandCli::ReadyOverview(_)
        | TicketCommandCli::Next(_) => batch_dispatch_query(cmd, store),
        TicketCommandCli::Attach(_)
        | TicketCommandCli::Assets(_)
        | TicketCommandCli::History(_)
        | TicketCommandCli::Diff(_)
        | TicketCommandCli::Revert(_)
        | TicketCommandCli::Audit
        | TicketCommandCli::Fmt(_)
        | TicketCommandCli::Board(_) => batch_dispatch_ops(cmd, store),
        other => batch_dispatch_forbidden(other),
    }
}

fn batch_dispatch_core(
    cmd: TicketCommandCli,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
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
        _ => unreachable!("handled in batch core dispatch"),
    }
}

fn batch_dispatch_query(
    cmd: TicketCommandCli,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    match cmd {
        TicketCommandCli::Subgraph(args) => commands::cmd_subgraph(args, store),
        TicketCommandCli::Topgraph(args) => commands::cmd_topgraph(args, store),
        TicketCommandCli::Search(args) | TicketCommandCli::Query(args) =>
            commands::cmd_search(args, store),
        TicketCommandCli::Health(args) => commands::cmd_health(args, store),
        TicketCommandCli::Close(args) => commands::cmd_close(args, store),
        TicketCommandCli::Cancel(args) => commands::cmd_cancel(args, store),
        TicketCommandCli::Status(args) => commands::cmd_status(args, store),
        TicketCommandCli::ReadyOverview(args) =>
            commands::cmd_ready_overview(args, store),
        TicketCommandCli::Next(args) => commands::cmd_next(args, store),
        _ => unreachable!("handled in batch query dispatch"),
    }
}

fn batch_dispatch_ops(
    cmd: TicketCommandCli,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    match cmd {
        TicketCommandCli::Attach(args) => commands::cmd_attach(args, store),
        TicketCommandCli::Assets(args) => commands::cmd_assets(args, store),
        TicketCommandCli::History(args) => commands::cmd_history(args, store),
        TicketCommandCli::Diff(args) => commands::cmd_diff(args, store),
        TicketCommandCli::Revert(args) => commands::cmd_revert(args, store),
        TicketCommandCli::Audit => commands::cmd_audit(store),
        TicketCommandCli::Fmt(args) => commands::cmd_fmt(args, store),
        TicketCommandCli::Board(args) => commands::cmd_board(args, store),
        _ => unreachable!("handled in batch ops dispatch"),
    }
}

fn batch_dispatch_forbidden(
    cmd: TicketCommandCli
) -> Result<Value, CliRunError> {
    Err(CliRunError::BadRequest(
        forbidden_batch_message(cmd).to_string(),
    ))
}

fn forbidden_batch_message(cmd: TicketCommandCli) -> &'static str {
    match cmd {
        TicketCommandCli::Serve(_) => "'serve' cannot be used in a batch",
        TicketCommandCli::Watch(_) => "'watch' cannot be used in a batch",
        TicketCommandCli::Batch(_) => "'batch' cannot be nested",
        TicketCommandCli::Scan(_) => "'scan' cannot be used in a batch",
        TicketCommandCli::Claim(_)
        | TicketCommandCli::Unclaim(_)
        | TicketCommandCli::Leases =>
            "lease commands cannot be used in a batch",
        TicketCommandCli::AddRoot(_) => "'add-root' cannot be used in a batch",
        TicketCommandCli::ExportCommandSchema =>
            "'export-command-schema' cannot be used in a batch",
        TicketCommandCli::FinalizeMerge(_) =>
            "'finalize-merge' is not supported in a batch",
        TicketCommandCli::Move(_) => "'move' cannot be used in a batch",
        _ => unreachable!("handled before forbidden batch dispatch"),
    }
}
