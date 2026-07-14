use std::path::PathBuf;

use clap::{
    Args,
    Parser,
    Subcommand,
};
use serde_json::{
    Value,
    json,
};
use uuid::Uuid;

use memory_api::workspace;
use session_api::{
    DEFAULT_PROMPT_SUMMARIZE_THRESHOLD_CHARS,
    DEFAULT_SKELETON_PREVIEW_CHARS,
    PromptPackOptions,
    SessionError,
    SessionQuery,
    SessionRuntimeInitRequest,
    SessionStoreConfig,
    SessionValidationGate,
    SessionWorkflowEdgeKind,
    SessionWorkflowNodeDraft,
    SessionWorkflowNodeKind,
    SessionWorkflowNodeRequirement,
    SessionWorkflowNodeStatus,
    SessionWorktreeCheckInRequest,
};

const SESSION_STORE_DIR: &str = ".session";

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

    /// Explicit session store root (the `.session` directory).
    #[arg(long, global = true)]
    pub store_root: Option<PathBuf>,

    /// Workspace/repo root to normalize to the canonical `.session` store.
    /// Lets a tool run from an ancestor checkout target a nested workspace.
    #[arg(long = "workspace", alias = "workspace-root", global = true)]
    pub workspace_root: Option<PathBuf>,

    /// Workspace slug that scopes session storage.
    #[arg(long, global = true, default_value = "default")]
    pub workspace_slug: String,

    #[command(subcommand)]
    pub command: SessionCommand,
}

#[derive(Debug, Subcommand)]
pub enum SessionCommand {
    /// Initialize or resume durable runtime workspace context.
    Init(InitArgs),
    /// Resume an existing workspace with a new linked run id.
    Resume(ResumeArgs),
    /// Pin an entity URN into runtime context.
    Pin(PinArgs),
    /// Unpin an entity URN from runtime context.
    Unpin(UnpinArgs),
    /// Read headers-only runtime context view.
    View(ViewArgs),
    /// Canonical nested workflow commands (`session workflow <subcommand>`).
    Workflow {
        #[command(subcommand)]
        command: WorkflowCommand,
    },
    /// Add a workflow node.
    WorkflowAddNode(WorkflowAddNodeArgs),
    /// Link two workflow nodes.
    WorkflowAddEdge(WorkflowAddEdgeArgs),
    /// Update workflow node status.
    WorkflowSetStatus(WorkflowSetStatusArgs),
    /// Promote a workflow node to a ticket-backed node.
    WorkflowPromote(WorkflowPromoteArgs),
    /// Render workflow as terminal output.
    WorkflowRenderTerminal(ViewArgs),
    /// Render workflow as Mermaid flowchart output.
    WorkflowRenderMermaid(ViewArgs),
    /// Persist a structured handoff record and render handoff summary.
    Handoff(HandoffArgs),
    /// Explicitly finish a workflow with required gates.
    Finish(FinishArgs),
    /// Check a session into its authoritative worktree assignment.
    CheckIn(CheckInArgs),
    /// Look up the worktree assignment for a session.
    Lookup(LookupArgs),
    /// Query stored sessions with optional filters.
    Query(QueryArgs),
    /// Move a UUID-addressed session to another workspace store.
    Move(MoveArgs),
    /// Peek a bounded window of transcript turns for a session.
    PeekRange(PeekRangeArgs),
    /// Peek a body-stripped skeleton of a session transcript.
    PeekSkeleton(PeekSkeletonArgs),
    /// Peek a prompt-facing compact view of a session transcript.
    PeekPromptPack(PeekPromptPackArgs),
}

/// Canonical nested workflow subcommands. These mirror the flat
/// `workflow-*` variants, which are retained as compatibility aliases.
#[derive(Debug, Subcommand)]
pub enum WorkflowCommand {
    /// Add a workflow node.
    AddNode(WorkflowAddNodeArgs),
    /// Link two workflow nodes.
    AddEdge(WorkflowAddEdgeArgs),
    /// Update workflow node status.
    SetStatus(WorkflowSetStatusArgs),
    /// Promote a workflow node to a ticket-backed node.
    Promote(WorkflowPromoteArgs),
    /// Render workflow as terminal output.
    RenderTerminal(ViewArgs),
    /// Render workflow as Mermaid flowchart output.
    RenderMermaid(ViewArgs),
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

#[derive(Debug, Args)]
pub struct PeekPromptPackArgs {
    /// Session id to peek.
    #[arg(long)]
    pub session_id: String,
    /// Maximum preview characters retained per included turn.
    #[arg(long, default_value_t = DEFAULT_SKELETON_PREVIEW_CHARS)]
    pub preview_chars: usize,
    /// Content-length threshold above which entries are summarized.
    #[arg(long, default_value_t = DEFAULT_PROMPT_SUMMARIZE_THRESHOLD_CHARS)]
    pub summarize_threshold_chars: usize,
}

#[derive(Debug, Args)]
pub struct MoveArgs {
    /// Session UUID to move (required unless --resume/--rollback is used).
    pub id: Option<String>,
    /// Destination workspace root.
    #[arg(long = "to-workspace-root")]
    pub to_workspace_root: Option<PathBuf>,
    /// Plan only; do not execute the move.
    #[arg(long, default_value_t = false)]
    pub dry_run: bool,
    /// Resume an interrupted move from a journal UUID.
    #[arg(long)]
    pub resume: Option<String>,
    /// Roll back a move from a journal UUID.
    #[arg(long)]
    pub rollback: Option<String>,
}

#[derive(Debug, Args)]
pub struct InitArgs {
    #[arg(long)]
    pub workspace_session_id: Option<String>,
    #[arg(long)]
    pub predecessor_run_id: Option<String>,
    #[arg(long, default_value_t = false)]
    pub force_new_run: bool,
}

#[derive(Debug, Args)]
pub struct ResumeArgs {
    #[arg(long)]
    pub workspace_session_id: String,
    #[arg(long)]
    pub predecessor_run_id: String,
}

#[derive(Debug, Args)]
pub struct PinArgs {
    #[arg(long)]
    pub workspace_session_id: String,
    #[arg(long)]
    pub entity_urn: String,
    #[arg(long)]
    pub relation: Option<String>,
    #[arg(long)]
    pub reason: Option<String>,
}

#[derive(Debug, Args)]
pub struct UnpinArgs {
    #[arg(long)]
    pub workspace_session_id: String,
    #[arg(long)]
    pub entity_urn: String,
}

#[derive(Debug, Args)]
pub struct ViewArgs {
    #[arg(long)]
    pub workspace_session_id: String,
}

#[derive(Debug, Args)]
pub struct WorkflowAddNodeArgs {
    #[arg(long)]
    pub workspace_session_id: String,
    #[arg(long)]
    pub node_id: Option<String>,
    #[arg(long)]
    pub kind: String,
    #[arg(long)]
    pub requirement: String,
    #[arg(long)]
    pub title: String,
    #[arg(long)]
    pub ticket_urn: Option<String>,
    #[arg(long)]
    pub cached_ticket_title: Option<String>,
    #[arg(long)]
    pub validation_spec_id: Option<String>,
}

#[derive(Debug, Args)]
pub struct WorkflowAddEdgeArgs {
    #[arg(long)]
    pub workspace_session_id: String,
    #[arg(long)]
    pub from: String,
    #[arg(long)]
    pub to: String,
    #[arg(long)]
    pub kind: String,
}

#[derive(Debug, Args)]
pub struct WorkflowSetStatusArgs {
    #[arg(long)]
    pub workspace_session_id: String,
    #[arg(long)]
    pub node_id: String,
    #[arg(long)]
    pub status: String,
    #[arg(long)]
    pub deferred_reason: Option<String>,
}

#[derive(Debug, Args)]
pub struct WorkflowPromoteArgs {
    #[arg(long)]
    pub workspace_session_id: String,
    #[arg(long)]
    pub node_id: String,
    #[arg(long)]
    pub ticket_urn: String,
    #[arg(long)]
    pub cached_ticket_title: Option<String>,
}

#[derive(Debug, Args)]
pub struct HandoffArgs {
    #[arg(long)]
    pub workspace_session_id: String,
    /// JSON array of validation gates.
    #[arg(long)]
    pub validation_json: Option<String>,
}

#[derive(Debug, Args)]
pub struct FinishArgs {
    #[arg(long)]
    pub workspace_session_id: String,
    /// JSON array of validation gates.
    #[arg(long)]
    pub validation_json: Option<String>,
    /// Optional-node ids explicitly deferred.
    #[arg(long = "defer-node")]
    pub deferred_optional_node_ids: Vec<String>,
}

// ── output helpers ──────────────────────────────────────────────────────────

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineOutputFormat {
    Json,
    Toon,
}

#[derive(Debug)]
pub enum CliOutput {
    Machine(Value, MachineOutputFormat),
    Text(String),
}

#[derive(Debug, thiserror::Error)]
pub enum CliRunError {
    #[error("session error: {0}")]
    Session(#[from] SessionError),
    #[error("bad request: {0}")]
    BadRequest(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}

// ── entry point ───────────────────────────────────────────────────────────────

pub fn run(cli: SessionCli) -> Result<CliOutput, CliRunError> {
    if matches!(cli.command, SessionCommand::CheckIn(_))
        && cli.store_root.is_none()
        && cli.workspace_root.is_none()
    {
        return Err(CliRunError::BadRequest(
            "entity creation requires explicit --workspace <path> or --store-root <path>".to_string(),
        ));
    }

    let store_root = workspace::resolve_requested_store_root(
        cli.store_root.as_deref(),
        cli.workspace_root.as_deref(),
        None,
        SESSION_STORE_DIR,
    );
    let config =
        SessionStoreConfig::new(store_root, cli.workspace_slug.clone());

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
        SessionCommand::Init(args) => {
            let result =
                config.init_runtime_context(SessionRuntimeInitRequest {
                    workspace_session_id: args.workspace_session_id,
                    predecessor_run_id: args.predecessor_run_id,
                    force_new_run: args.force_new_run,
                })?;
            to_value(&result)
        },
        SessionCommand::Resume(args) => {
            let result = config.resume_workspace_context(
                &args.workspace_session_id,
                &args.predecessor_run_id,
            )?;
            to_value(&result)
        },
        SessionCommand::Pin(args) => {
            let context = config.pin_runtime_entity(
                &args.workspace_session_id,
                &args.entity_urn,
                args.relation,
                args.reason,
            )?;
            to_value(&context)
        },
        SessionCommand::Unpin(args) => {
            let context = config.unpin_runtime_entity(
                &args.workspace_session_id,
                &args.entity_urn,
            )?;
            to_value(&context)
        },
        SessionCommand::View(args) => {
            let view =
                config.view_runtime_context(&args.workspace_session_id)?;
            to_value(&view)
        },
        SessionCommand::Workflow { command } =>
            handle_workflow_command(&config, command),
        SessionCommand::WorkflowAddNode(args) =>
            handle_workflow_command(&config, WorkflowCommand::AddNode(args)),
        SessionCommand::WorkflowAddEdge(args) =>
            handle_workflow_command(&config, WorkflowCommand::AddEdge(args)),
        SessionCommand::WorkflowSetStatus(args) =>
            handle_workflow_command(&config, WorkflowCommand::SetStatus(args)),
        SessionCommand::WorkflowPromote(args) =>
            handle_workflow_command(&config, WorkflowCommand::Promote(args)),
        SessionCommand::WorkflowRenderTerminal(args) =>
            handle_workflow_command(
                &config,
                WorkflowCommand::RenderTerminal(args),
            ),
        SessionCommand::WorkflowRenderMermaid(args) => handle_workflow_command(
            &config,
            WorkflowCommand::RenderMermaid(args),
        ),
        SessionCommand::Handoff(args) => {
            let result = config.create_handoff_result(
                &args.workspace_session_id,
                parse_validation_gates(args.validation_json.as_deref())?,
                None,
            )?;
            to_value(&result)
        },
        SessionCommand::Finish(args) => {
            let result = config.finish_workflow(
                &args.workspace_session_id,
                parse_validation_gates(args.validation_json.as_deref())?,
                args.deferred_optional_node_ids,
                None,
            )?;
            to_value(&result)
        },
        SessionCommand::CheckIn(args) => {
            let receipt =
                config.check_in_worktree(SessionWorktreeCheckInRequest {
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
        SessionCommand::Move(args) => move_command(config, args),
        SessionCommand::PeekRange(args) => {
            let range =
                config.peek_range(&args.session_id, args.start, args.end)?;
            to_value(&range)
        },
        SessionCommand::PeekSkeleton(args) => {
            let skeleton =
                config.peek_skeleton(&args.session_id, args.preview_chars)?;
            to_value(&skeleton)
        },
        SessionCommand::PeekPromptPack(args) => {
            let pack = config.peek_prompt_pack(
                &args.session_id,
                PromptPackOptions {
                    preview_chars: args.preview_chars,
                    summarize_threshold_chars: args.summarize_threshold_chars,
                },
            )?;
            to_value(&pack)
        },
    }
}

/// Shared handler for canonical nested (`session workflow <subcommand>`) and
/// flat (`session workflow-<subcommand>`) command forms.
fn handle_workflow_command(
    config: &SessionStoreConfig,
    command: WorkflowCommand,
) -> Result<Value, CliRunError> {
    match command {
        WorkflowCommand::AddNode(args) => {
            let context = config.workflow_add_node(
                &args.workspace_session_id,
                SessionWorkflowNodeDraft {
                    node_id: args.node_id,
                    kind: parse_node_kind(&args.kind)?,
                    requirement: parse_requirement(&args.requirement)?,
                    title: args.title,
                    ticket_urn: args.ticket_urn,
                    cached_ticket_title: args.cached_ticket_title,
                    validation_spec_id: args.validation_spec_id,
                },
            )?;
            to_value(&context)
        },
        WorkflowCommand::AddEdge(args) => {
            let context = config.workflow_add_edge(
                &args.workspace_session_id,
                &args.from,
                &args.to,
                parse_edge_kind(&args.kind)?,
            )?;
            to_value(&context)
        },
        WorkflowCommand::SetStatus(args) => {
            let context = config.workflow_update_node_status(
                &args.workspace_session_id,
                &args.node_id,
                parse_node_status(&args.status)?,
                args.deferred_reason,
            )?;
            to_value(&context)
        },
        WorkflowCommand::Promote(args) => {
            let context = config.workflow_promote_node_to_ticket(
                &args.workspace_session_id,
                &args.node_id,
                &args.ticket_urn,
                args.cached_ticket_title,
            )?;
            to_value(&context)
        },
        WorkflowCommand::RenderTerminal(args) => {
            let rendered = config
                .workflow_render_terminal(&args.workspace_session_id, None)?;
            to_value(&json!({"render": rendered}))
        },
        WorkflowCommand::RenderMermaid(args) => {
            let rendered = config
                .workflow_render_mermaid(&args.workspace_session_id, None)?;
            to_value(&json!({"render": rendered}))
        },
    }
}

fn parse_validation_gates(
    raw: Option<&str>
) -> Result<Vec<SessionValidationGate>, CliRunError> {
    let Some(raw) = raw else {
        return Ok(vec![]);
    };
    serde_json::from_str::<Vec<SessionValidationGate>>(raw).map_err(|err| {
        CliRunError::BadRequest(format!(
            "invalid --validation-json payload: {err}"
        ))
    })
}

fn parse_node_kind(
    value: &str
) -> Result<SessionWorkflowNodeKind, CliRunError> {
    match value {
        "ticket" => Ok(SessionWorkflowNodeKind::Ticket),
        "action" => Ok(SessionWorkflowNodeKind::Action),
        "decision" => Ok(SessionWorkflowNodeKind::Decision),
        "checkpoint" => Ok(SessionWorkflowNodeKind::Checkpoint),
        "validation" => Ok(SessionWorkflowNodeKind::Validation),
        _ => Err(CliRunError::BadRequest(format!(
            "invalid workflow node kind: {value}"
        ))),
    }
}

fn parse_requirement(
    value: &str
) -> Result<SessionWorkflowNodeRequirement, CliRunError> {
    match value {
        "required" => Ok(SessionWorkflowNodeRequirement::Required),
        "optional" => Ok(SessionWorkflowNodeRequirement::Optional),
        _ => Err(CliRunError::BadRequest(format!(
            "invalid workflow requirement: {value}"
        ))),
    }
}

fn parse_edge_kind(
    value: &str
) -> Result<SessionWorkflowEdgeKind, CliRunError> {
    match value {
        "depends-on" | "depends_on" => Ok(SessionWorkflowEdgeKind::DependsOn),
        "order" => Ok(SessionWorkflowEdgeKind::Order),
        _ => Err(CliRunError::BadRequest(format!(
            "invalid workflow edge kind: {value}"
        ))),
    }
}

fn parse_node_status(
    value: &str
) -> Result<SessionWorkflowNodeStatus, CliRunError> {
    match value {
        "pending" => Ok(SessionWorkflowNodeStatus::Pending),
        "in-progress" | "in_progress" =>
            Ok(SessionWorkflowNodeStatus::InProgress),
        "blocked" => Ok(SessionWorkflowNodeStatus::Blocked),
        "done" => Ok(SessionWorkflowNodeStatus::Done),
        "deferred" => Ok(SessionWorkflowNodeStatus::Deferred),
        _ => Err(CliRunError::BadRequest(format!(
            "invalid workflow status: {value}"
        ))),
    }
}

fn move_command(
    config: &SessionStoreConfig,
    args: MoveArgs,
) -> Result<Value, CliRunError> {
    if args.resume.is_some() && args.rollback.is_some() {
        return Err(CliRunError::BadRequest(
            "move accepts only one of --resume or --rollback".to_string(),
        ));
    }

    if let Some(journal_id) = args.resume.as_deref() {
        let journal_id = journal_id.parse::<Uuid>().map_err(|error| {
            CliRunError::BadRequest(format!(
                "invalid --resume journal UUID: {error}"
            ))
        })?;
        let outcome = config.resume_move_with_journal(journal_id)?;
        return to_value(&json!({
            "command": "move",
            "status": "ok",
            "mode": "resume",
            "journal_id": outcome.journal.id,
            "phase": outcome.journal.phase,
            "recovery": recovery_hint(),
        }));
    }

    if let Some(journal_id) = args.rollback.as_deref() {
        let journal_id = journal_id.parse::<Uuid>().map_err(|error| {
            CliRunError::BadRequest(format!(
                "invalid --rollback journal UUID: {error}"
            ))
        })?;
        let outcome = config.rollback_move_with_journal(journal_id)?;
        return to_value(&json!({
            "command": "move",
            "status": "ok",
            "mode": "rollback",
            "journal_id": outcome.journal.id,
            "phase": outcome.journal.phase,
            "recovery": recovery_hint(),
        }));
    }

    let id = args.id.as_deref().ok_or_else(|| {
        CliRunError::BadRequest(
            "move requires <id> unless --resume/--rollback is used".to_string(),
        )
    })?;
    let to_workspace_root =
        args.to_workspace_root.as_deref().ok_or_else(|| {
            CliRunError::BadRequest(
                "move requires --to-workspace-root in plan/execute mode"
                    .to_string(),
            )
        })?;

    let session_id = id.parse::<Uuid>().map_err(|error| {
        CliRunError::BadRequest(format!("invalid session UUID: {error}"))
    })?;
    let target_workspace_root =
        workspace::canonicalize_workspace_root_strict(to_workspace_root)
            .map_err(|error| {
                CliRunError::BadRequest(format!(
                    "workspace root canonicalization failed for '{}': {error}",
                    to_workspace_root.display()
                ))
            })?;
    let report =
        config.plan_move_preflight(&session_id, &target_workspace_root)?;

    if args.dry_run || !report.supported() {
        return to_value(&json!({
            "command": "move",
            "status": if report.supported() { "ok" } else { "blocked" },
            "mode": "plan",
            "dry_run": true,
            "session_id": session_id,
            "plan": move_plan_json(&report)?,
            "recovery": recovery_hint(),
        }));
    }

    let outcome = config.execute_move_with_journal(&report)?;
    to_value(&json!({
        "command": "move",
        "status": "ok",
        "mode": "execute",
        "session_id": session_id,
        "plan": move_plan_json(&report)?,
        "outcome": move_outcome_json(&outcome)?,
        "recovery": recovery_hint(),
    }))
}

fn move_plan_json(
    report: &memory_api::storage::move_kernel::MovePlan
) -> Result<Value, CliRunError> {
    Ok(json!({
        "supported": report.supported(),
        "entity_id": report.entity_id,
        "source_workspace_root": path_display(&report.source_workspace_root)?,
        "target_workspace_root": path_display(&report.target_workspace_root)?,
        "source_store_root": path_display(&report.source_store_root)?,
        "target_store_root": path_display(&report.target_store_root)?,
        "source_git_worktree_root": path_display(&report.source_git_worktree_root)?,
        "target_git_worktree_root": path_display(&report.target_git_worktree_root)?,
        "git_worktree_topology": report.git_worktree_topology,
        "source_entity_path": path_display(&report.source_entity_path)?,
        "destination_entity_path": path_display(&report.destination_entity_path)?,
        "inbound_related_entity_ids": report.inbound_related_entity_ids,
        "outbound_related_entity_ids": report.outbound_related_entity_ids,
        "reference_visibility": report.reference_visibility,
        "active_board_entries": report.active_board_entries,
        "historical_board_entries": report.historical_board_entries,
        "active_leases": report.active_leases,
        "path_reference_files": report.path_reference_files
            .iter()
            .map(|path| path_display(path))
            .collect::<Result<Vec<_>, _>>()?,
        "blockers": report.blockers,
        "captured_at": report.captured_at,
    }))
}

fn move_outcome_json(
    outcome: &memory_api::storage::move_kernel::MoveOutcome
) -> Result<Value, CliRunError> {
    Ok(json!({
        "resumed": outcome.resumed,
        "rolled_back": outcome.rolled_back,
        "journal": {
            "id": outcome.journal.id,
            "entity_id": outcome.journal.entity_id,
            "source_store_root": path_display(&outcome.journal.source_store_root)?,
            "target_store_root": path_display(&outcome.journal.target_store_root)?,
            "source_entity_path": path_display(&outcome.journal.source_entity_path)?,
            "destination_entity_path": path_display(&outcome.journal.destination_entity_path)?,
            "phase": outcome.journal.phase,
            "created_at": outcome.journal.created_at,
            "updated_at": outcome.journal.updated_at,
            "steps": outcome.journal.steps,
            "rollback_steps": outcome.journal.rollback_steps,
            "lock_paths": outcome.journal.lock_paths,
            "migrated_board_entries": outcome.journal.migrated_board_entries,
            "rewritten_path_files": outcome.journal.rewritten_path_files,
            "manual_followups": outcome.journal.manual_followups,
            "failure": outcome.journal.failure,
            "next_recovery_step": outcome.journal.next_recovery_step,
        },
    }))
}

fn recovery_hint() -> Value {
    json!({
        "resume": "session move --resume <journal-uuid>",
        "rollback": "session move --rollback <journal-uuid>",
    })
}

fn path_display(path: &std::path::Path) -> Result<String, CliRunError> {
    workspace::normalize_path_for_display_strict(path).map_err(|error| {
        CliRunError::BadRequest(format!(
            "path payload normalization failed for '{}': {error}",
            path.display()
        ))
    })
}

fn to_value<T: serde::Serialize>(value: &T) -> Result<Value, CliRunError> {
    serde_json::to_value(value)
        .map_err(|err| CliRunError::Serialization(err.to_string()))
}

fn render_human(payload: &Value) -> String {
    serde_json::to_string_pretty(payload)
        .unwrap_or_else(|_| format!("{payload:?}"))
}

pub fn error_output(
    message: &str,
    format: Option<MachineOutputFormat>,
) -> String {
    let payload = json!({"status": "error", "message": message});
    match format {
        Some(MachineOutputFormat::Json) => payload.to_string(),
        Some(MachineOutputFormat::Toon) =>
            toon_format::encode_default(&payload).unwrap_or_else(|_| {
                format!("status: error\nmessage: {message}")
            }),
        None => message.to_string(),
    }
}

pub fn render_machine_output(
    payload: &Value,
    format: MachineOutputFormat,
) -> Result<String, String> {
    match format {
        MachineOutputFormat::Json =>
            serde_json::to_string_pretty(payload).map_err(|err| err.to_string()),
        MachineOutputFormat::Toon =>
            toon_format::encode_default(payload).map_err(|err| err.to_string()),
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

pub fn requested_machine_output_format_from_args() -> Option<MachineOutputFormat>
{
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
    use chrono::Utc;
    use session_api::{
        CopilotHookMessage,
        CopilotHookPayload,
        SessionCaptureRequest,
        SessionRole,
        SessionRuntimeInitRequest,
    };
    use std::process::Command;
    use tempfile::tempdir;

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
    fn parses_canonical_nested_workflow_add_node() {
        let cli = parse_cli_from([
            "session",
            "workflow",
            "add-node",
            "--workspace-session-id",
            "ws-1",
            "--kind",
            "action",
            "--requirement",
            "required",
            "--title",
            "do the thing",
        ])
        .expect("parse nested workflow add-node");

        match cli.command {
            SessionCommand::Workflow {
                command: WorkflowCommand::AddNode(args),
            } => {
                assert_eq!(args.workspace_session_id, "ws-1");
                assert_eq!(args.kind, "action");
                assert_eq!(args.requirement, "required");
                assert_eq!(args.title, "do the thing");
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_flat_workflow_add_node_alias() {
        let cli = parse_cli_from([
            "session",
            "workflow-add-node",
            "--workspace-session-id",
            "ws-1",
            "--kind",
            "action",
            "--requirement",
            "required",
            "--title",
            "do the thing",
        ])
        .expect("parse flat workflow-add-node alias");

        match cli.command {
            SessionCommand::WorkflowAddNode(args) => {
                assert_eq!(args.workspace_session_id, "ws-1");
                assert_eq!(args.title, "do the thing");
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn parses_canonical_nested_workflow_render_terminal() {
        let cli = parse_cli_from([
            "session",
            "workflow",
            "render-terminal",
            "--workspace-session-id",
            "ws-1",
        ])
        .expect("parse nested workflow render-terminal");

        assert!(matches!(
            cli.command,
            SessionCommand::Workflow {
                command: WorkflowCommand::RenderTerminal(_),
            }
        ));
    }

    #[test]
    fn parses_peek_range_defaults() {
        let cli =
            parse_cli_from(["session", "peek-range", "--session-id", "sess-1"])
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

    #[test]
    fn parses_move_command() {
        let cli = parse_cli_from([
            "session",
            "move",
            "7b3a7c62-1f3f-45d6-b8a1-f2b83e3d9f71",
            "--to-workspace-root",
            "/repo/target",
        ])
        .expect("parse move");

        match cli.command {
            SessionCommand::Move(args) => {
                assert_eq!(
                    args.id.as_deref(),
                    Some("7b3a7c62-1f3f-45d6-b8a1-f2b83e3d9f71")
                );
                assert_eq!(
                    args.to_workspace_root,
                    Some(PathBuf::from("/repo/target"))
                );
            },
            other => panic!("unexpected command: {other:?}"),
        }
    }

    #[test]
    fn move_roundtrip_executes_against_target_workspace() {
        let temp = tempdir().unwrap();
        let repo_root = temp.path().join("repo");
        std::fs::create_dir_all(&repo_root).unwrap();
        Command::new("git")
            .current_dir(&repo_root)
            .args(["init"])
            .status()
            .expect("git init")
            .success()
            .then_some(())
            .expect("git init failed");

        let source_store_root = repo_root.join(".session");
        std::fs::create_dir_all(&source_store_root).unwrap();
        let target_workspace_root = repo_root.join("target-workspace");
        std::fs::create_dir_all(target_workspace_root.join(".session"))
            .unwrap();

        let session_id = "7b3a7c62-1f3f-45d6-b8a1-f2b83e3d9f71";
        let config = SessionStoreConfig::new(
            source_store_root.clone(),
            "default".to_string(),
        );
        let payload = CopilotHookPayload {
            session_id: session_id.to_string(),
            workspace_slug: "default".to_string(),
            captured_at: Utc::now(),
            conversation_id: Some("conv-1".to_string()),
            agent_id: Some("agent-1".to_string()),
            model: None,
            trigger: None,
            messages: vec![CopilotHookMessage {
                role: SessionRole::User,
                content: "move me".to_string(),
                tool_name: None,
                captured_at: None,
                event_meta: None,
            }],
            events: vec![],
            runtime: None,
        };
        config
            .persist_capture(SessionCaptureRequest::copilot(payload))
            .expect("seed session");

        let cli = parse_cli_from([
            "session",
            "--json",
            "--store-root",
            source_store_root.to_string_lossy().as_ref(),
            "move",
            session_id,
            "--to-workspace-root",
            target_workspace_root.to_string_lossy().as_ref(),
        ])
        .expect("parse move");

        match run(cli).expect("run move") {
            CliOutput::Machine(value, _) => {
                assert_eq!(value["status"], "ok");
                assert_eq!(value["mode"], "execute");
                assert!(value["outcome"]["journal"]["id"].is_string());
            },
            other => panic!("unexpected output: {other:?}"),
        }

        let target_config = SessionStoreConfig::new(
            target_workspace_root.join(".session"),
            "default".to_string(),
        );
        assert!(matches!(
            config.read_session(session_id),
            Err(SessionError::NotFound { .. })
        ));
        assert_eq!(
            target_config.read_session(session_id).unwrap().session_id,
            session_id
        );
    }

    #[test]
    fn handoff_reference_completeness_includes_durable_identity_fields() {
        let temp = tempdir().unwrap();
        let config = SessionStoreConfig::new(
            temp.path().join(".session"),
            "default".to_string(),
        );

        let init = config
            .init_runtime_context(SessionRuntimeInitRequest::default())
            .expect("init runtime context");
        let workspace_session_id = init.context.workspace_session_id;

        let rendered = config
            .render_handoff_terminal(
                &workspace_session_id,
                vec![session_api::SessionValidationGate {
                    validation_spec_id: "val-handoff-reference-completeness"
                        .to_string(),
                    required: true,
                    outcome: Some("passed".to_string()),
                }],
                None,
            )
            .expect("render handoff");

        assert!(rendered.contains("workspace_session_id:"));
        assert!(rendered.contains("outgoing_run_id:"));
        assert!(rendered.contains("handoff "));
        assert!(
            rendered
                .contains("resume: session-cli resume --workspace-session-id")
        );
    }
}
