use std::path::PathBuf;

use clap::{
    Args,
    Parser,
    Subcommand,
};
use serde_json::{
    json,
    Value,
};

use memory_api::workspace;
use session_api::{
    SessionError,
    SessionQuery,
    SessionStoreConfig,
    SessionWorktreeCheckInRequest,
    DEFAULT_SKELETON_PREVIEW_CHARS,
};

const SESSION_STORE_DIR: &str = ".memory-api";

// ── CLI root ───────────────────────────────────────────────────────────────────

#[derive(Debug, Parser)]
#[command(
    name = "session",
    about = "Session system CLI (worktree check-in, lookup, query, transcript peeking)",
    version,
    arg_required_else_help = true
)]
pub struct SessionCli {
    /// Return machine-readable JSON output.
    #[arg(long, global = true, conflicts_with = "toon")]
    pub json: bool,

    /// Return machine-readable TOON output.
    #[arg(long, global = true, conflicts_with = "json")]
    pub toon: bool,

    /// Explicit session store root (the `.memory-api` directory).
    #[arg(long, global = true)]
    pub store_root: Option<PathBuf>,

    /// Workspace/repo root to normalize to the canonical `.memory-api` store.
    /// Lets a tool run from an ancestor checkout target a nested workspace.
    #[arg(long, global = true)]
    pub workspace_root: Option<PathBuf>,

    /// Workspace slug that scopes session storage.
    #[arg(long, global = true, default_value = "default")]
    pub workspace_slug: String,

    #[command(subcommand)]
    pub command: SessionCommand,
}

#[derive(Debug, Subcommand)]
pub enum SessionCommand {
    /// Check a session into its authoritative worktree assignment.
    CheckIn(CheckInArgs),
    /// Look up the worktree assignment for a session.
    Lookup(LookupArgs),
    /// Query stored sessions with optional filters.
    Query(QueryArgs),
    /// Peek a bounded window of transcript turns for a session.
    PeekRange(PeekRangeArgs),
    /// Peek a body-stripped skeleton of a session transcript.
    PeekSkeleton(PeekSkeletonArgs),
}

#[derive(Debug, Args)]
pub struct CheckInArgs {
    /// Session id to check in.
    #[arg(long)]
    pub session_id: String,
    /// Owner (agent) identity claiming the worktree.
    #[arg(long)]
    pub owner_id: String,
    /// Ticket the session is working on.
    #[arg(long)]
    pub ticket_id: String,
    /// Assigned worktree working directory.
    #[arg(long)]
    pub worktree_path: PathBuf,
    /// Branch checked out in the worktree.
    #[arg(long)]
    pub branch: String,
    /// Predecessor session id when rotating from a prior assignment.
    #[arg(long)]
    pub predecessor_session_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct LookupArgs {
    /// Session id to look up.
    #[arg(long)]
    pub session_id: String,
}

#[derive(Debug, Args)]
pub struct QueryArgs {
    /// Filter by session id prefix.
    #[arg(long)]
    pub session_id_prefix: Option<String>,
    /// Filter by conversation id.
    #[arg(long)]
    pub conversation_id: Option<String>,
    /// Filter by agent id.
    #[arg(long)]
    pub agent_id: Option<String>,
    /// Free-text filter across session content.
    #[arg(long)]
    pub text: Option<String>,
    /// Maximum number of sessions to return.
    #[arg(long)]
    pub limit: Option<usize>,
}

#[derive(Debug, Args)]
pub struct PeekRangeArgs {
    /// Session id to peek.
    #[arg(long)]
    pub session_id: String,
    /// Inclusive start turn index (0-based).
    #[arg(long, default_value_t = 0)]
    pub start: usize,
    /// Exclusive end turn index (0-based). Defaults to the end of the transcript.
    #[arg(long)]
    pub end: Option<usize>,
}

#[derive(Debug, Args)]
pub struct PeekSkeletonArgs {
    /// Session id to peek.
    #[arg(long)]
    pub session_id: String,
    /// Maximum preview characters retained per turn.
    #[arg(long, default_value_t = DEFAULT_SKELETON_PREVIEW_CHARS)]
    pub preview_chars: usize,
}

// ── output helpers ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineOutputFormat {
    Json,
    Toon,
}

pub enum CliOutput {
    Machine(Value, MachineOutputFormat),
    Text(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CliRunError {
    #[error("session error: {0}")]
    Session(#[from] SessionError),
    #[error("serialization error: {0}")]
    Serialization(String),
}

// ── entry point ───────────────────────────────────────────────────────────────

pub fn run(cli: SessionCli) -> Result<CliOutput, CliRunError> {
    let store_root = workspace::resolve_requested_store_root(
        cli.store_root.as_deref(),
        cli.workspace_root.as_deref(),
        None,
        SESSION_STORE_DIR,
    );
    let config = SessionStoreConfig::new(store_root, cli.workspace_slug.clone());

    let payload = dispatch(&config, cli.command)?;

    match machine_output_format(cli.json, cli.toon) {
        Some(format) => Ok(CliOutput::Machine(payload, format)),
        None => Ok(CliOutput::Text(render_human(&payload))),
    }
}

fn dispatch(
    config: &SessionStoreConfig,
    command: SessionCommand,
) -> Result<Value, CliRunError> {
    match command {
        SessionCommand::CheckIn(args) => {
            let receipt = config.check_in_worktree(SessionWorktreeCheckInRequest {
                session_id: args.session_id,
                owner_id: args.owner_id,
                ticket_id: args.ticket_id,
                worktree_path: args.worktree_path,
                branch: args.branch,
                predecessor_session_id: args.predecessor_session_id,
            })?;
            to_value(&receipt)
        },
        SessionCommand::Lookup(args) => {
            let receipt = config.lookup_worktree(&args.session_id)?;
            to_value(&receipt)
        },
        SessionCommand::Query(args) => {
            let query = SessionQuery {
                session_id_prefix: args.session_id_prefix,
                conversation_id: args.conversation_id,
                agent_id: args.agent_id,
                text: args.text,
                limit: args.limit,
            };
            let sessions = config.query_sessions(&query)?;
            to_value(&json!({
                "count": sessions.len(),
                "sessions": sessions,
            }))
        },
        SessionCommand::PeekRange(args) => {
            let range = config.peek_range(&args.session_id, args.start, args.end)?;
            to_value(&range)
        },
        SessionCommand::PeekSkeleton(args) => {
            let skeleton = config.peek_skeleton(&args.session_id, args.preview_chars)?;
            to_value(&skeleton)
        },
    }
}

fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, CliRunError> {
    serde_json::to_value(value).map_err(|err| CliRunError::Serialization(err.to_string()))
}

fn render_human(payload: &Value) -> String {
    serde_json::to_string_pretty(payload).unwrap_or_else(|_| format!("{payload:?}"))
}

pub fn error_output(
    message: &str,
    format: Option<MachineOutputFormat>,
) -> String {
    let payload = json!({"status": "error", "message": message});
    match format {
        Some(MachineOutputFormat::Json) => payload.to_string(),
        Some(MachineOutputFormat::Toon) => toon_format::encode_default(&payload)
            .unwrap_or_else(|_| format!("status: error\nmessage: {message}")),
        None => message.to_string(),
    }
}

pub fn render_machine_output(
    payload: &Value,
    format: MachineOutputFormat,
) -> Result<String, String> {
    match format {
        MachineOutputFormat::Json => {
            serde_json::to_string_pretty(payload).map_err(|err| err.to_string())
        },
        MachineOutputFormat::Toon => {
            toon_format::encode_default(payload).map_err(|err| err.to_string())
        },
    }
}

pub fn machine_output_format(
    as_json: bool,
    as_toon: bool,
) -> Option<MachineOutputFormat> {
    if as_json {
        Some(MachineOutputFormat::Json)
    } else if as_toon {
        Some(MachineOutputFormat::Toon)
    } else {
        None
    }
}

pub fn requested_machine_output_format_from_args() -> Option<MachineOutputFormat> {
    machine_output_format(
        std::env::args().any(|arg| arg == "--json"),
        std::env::args().any(|arg| arg == "--toon"),
    )
}

pub fn parse_cli_from<I, T>(args: I) -> Result<SessionCli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    SessionCli::try_parse_from(args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_check_in_command() {
        let cli = parse_cli_from([
            "session",
            "check-in",
            "--session-id",
            "sess-1",
            "--owner-id",
            "agent-1",
            "--ticket-id",
            "ticket-1",
            "--worktree-path",
            "/repo/wt",
            "--branch",
            "feature/x",
        ])
        .expect("parse check-in");

        assert_eq!(cli.workspace_slug, "default");
        match cli.command {
            SessionCommand::CheckIn(args) => {
                assert_eq!(args.session_id, "sess-1");
                assert_eq!(args.branch, "feature/x");
                assert!(args.predecessor_session_id.is_none());
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_peek_range_defaults() {
        let cli = parse_cli_from([
            "session",
            "peek-range",
            "--session-id",
            "sess-1",
        ])
        .expect("parse peek-range");

        match cli.command {
            SessionCommand::PeekRange(args) => {
                assert_eq!(args.start, 0);
                assert!(args.end.is_none());
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn json_and_toon_conflict() {
        let result = parse_cli_from([
            "session",
            "--json",
            "--toon",
            "lookup",
            "--session-id",
            "sess-1",
        ]);
        assert!(result.is_err());
    }
}
