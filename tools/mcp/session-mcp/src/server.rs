use std::path::PathBuf;

use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    ServiceExt,
    handler::server::{
        tool::ToolRouter,
        wrapper::Parameters,
    },
    model::*,
    schemars::{
        self,
        JsonSchema,
    },
    tool,
    tool_handler,
    tool_router,
    transport::stdio,
};
use serde::{
    Deserialize,
    Serialize,
};
use uuid::Uuid;

use memory_api::workspace;
use session_api::{
    DEFAULT_SKELETON_PREVIEW_CHARS,
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

// ── Input types ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct CheckInInput {
    /// Concrete workspace path, repo root, .session store path, or path inside that store.
    pub workspace: String,
    /// Session id to check in.
    pub session_id: String,
    /// Owner (agent) identity claiming the worktree.
    pub owner_id: String,
    /// Ticket the session is working on.
    pub ticket_id: String,
    /// Assigned worktree working directory.
    pub worktree_path: String,
    /// Branch checked out in the worktree.
    pub branch: String,
    /// Predecessor session id when rotating from a prior assignment.
    #[serde(default)]
    pub predecessor_session_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct LookupInput {
    /// Session id to look up.
    pub session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct QueryInput {
    /// Filter by session id prefix.
    #[serde(default)]
    pub session_id_prefix: Option<String>,
    /// Filter by conversation id.
    #[serde(default)]
    pub conversation_id: Option<String>,
    /// Filter by agent id.
    #[serde(default)]
    pub agent_id: Option<String>,
    /// Free-text filter across session content.
    #[serde(default)]
    pub text: Option<String>,
    /// Maximum number of sessions to return.
    #[serde(default)]
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeekRangeInput {
    /// Session id to peek.
    pub session_id: String,
    /// Inclusive start turn index (0-based).
    #[serde(default)]
    pub start: usize,
    /// Exclusive end turn index (0-based). Defaults to the end of the transcript.
    #[serde(default)]
    pub end: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct PeekSkeletonInput {
    /// Session id to peek.
    pub session_id: String,
    /// Maximum preview characters retained per turn.
    #[serde(default)]
    pub preview_chars: Option<usize>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionMoveInput {
    /// Session UUID to move.
    pub id: String,
    /// Destination workspace root.
    pub to_workspace_root: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SessionMoveJournalInput {
    /// Move journal UUID.
    pub id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimeInitInput {
    pub workspace: String,
    #[serde(default)]
    pub workspace_session_id: Option<String>,
    #[serde(default)]
    pub predecessor_run_id: Option<String>,
    #[serde(default)]
    pub force_new_run: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimeResumeInput {
    pub workspace: String,
    pub workspace_session_id: String,
    pub predecessor_run_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimePinInput {
    pub workspace: String,
    pub workspace_session_id: String,
    pub entity_urn: String,
    #[serde(default)]
    pub relation: Option<String>,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimeUnpinInput {
    pub workspace: String,
    pub workspace_session_id: String,
    pub entity_urn: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimeViewInput {
    pub workspace: String,
    pub workspace_session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkflowAddNodeInput {
    pub workspace: String,
    pub workspace_session_id: String,
    #[serde(default)]
    pub node_id: Option<String>,
    pub kind: String,
    pub requirement: String,
    pub title: String,
    #[serde(default)]
    pub ticket_urn: Option<String>,
    #[serde(default)]
    pub cached_ticket_title: Option<String>,
    #[serde(default)]
    pub validation_spec_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkflowAddEdgeInput {
    pub workspace: String,
    pub workspace_session_id: String,
    pub from: String,
    pub to: String,
    pub kind: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkflowSetStatusInput {
    pub workspace: String,
    pub workspace_session_id: String,
    pub node_id: String,
    pub status: String,
    #[serde(default)]
    pub deferred_reason: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkflowPromoteInput {
    pub workspace: String,
    pub workspace_session_id: String,
    pub node_id: String,
    pub ticket_urn: String,
    #[serde(default)]
    pub cached_ticket_title: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WorkflowRenderInput {
    pub workspace: String,
    pub workspace_session_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimeHandoffInput {
    pub workspace: String,
    pub workspace_session_id: String,
    #[serde(default)]
    pub validation: Vec<ValidationGateInput>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RuntimeFinishInput {
    pub workspace: String,
    pub workspace_session_id: String,
    #[serde(default)]
    pub validation: Vec<ValidationGateInput>,
    #[serde(default)]
    pub deferred_optional_node_ids: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ValidationGateInput {
    pub validation_spec_id: String,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub outcome: Option<String>,
}

impl From<ValidationGateInput> for SessionValidationGate {
    fn from(value: ValidationGateInput) -> Self {
        Self {
            validation_spec_id: value.validation_spec_id,
            required: value.required,
            outcome: value.outcome,
        }
    }
}

// ── Server ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct SessionServer {
    store_root: PathBuf,
    workspace_slug: String,
    tool_router: ToolRouter<Self>,
}

impl SessionServer {
    pub fn new(
        store_root: PathBuf,
        workspace_slug: String,
    ) -> Self {
        Self {
            store_root,
            workspace_slug,
            tool_router: Self::tool_router(),
        }
    }

    fn config(&self) -> SessionStoreConfig {
        SessionStoreConfig::new(
            self.store_root.clone(),
            self.workspace_slug.clone(),
        )
    }

    fn config_for_workspace(
        &self,
        workspace_selector: &str,
    ) -> Result<SessionStoreConfig, McpError> {
        let workspace_selector =
            workspace::validate_explicit_workspace_selector(Some(
                workspace_selector,
            ))
            .map_err(|err| McpError::invalid_params(err.to_string(), None))?;
        let store_root = workspace::resolve_store_root_from(
            std::path::Path::new(workspace_selector),
            ".session",
        );
        Ok(SessionStoreConfig::new(
            store_root,
            self.workspace_slug.clone(),
        ))
    }

    fn json_result<T: Serialize>(
        value: &T
    ) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string(value).map_err(|err| {
            McpError::internal_error(format!("serialization: {err}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    fn session_err(err: SessionError) -> McpError {
        match &err {
            SessionError::NotFound { .. }
            | SessionError::MissingSessionId
            | SessionError::MissingOwnerId
            | SessionError::MissingTicketId
            | SessionError::EmptyWorktreePath
            | SessionError::EmptyWorktreeBranch
            | SessionError::InvalidSessionId(_)
            | SessionError::InvalidWorkspaceSessionId(_)
            | SessionError::InvalidEntityUrn(_)
            | SessionError::InvalidWorkspaceSlug(_)
            | SessionError::MissingWorktreeAssignment { .. }
            | SessionError::SessionOwnershipMismatch { .. }
            | SessionError::WorktreeConflict { .. }
            | SessionError::CrossSessionReuseRequiresAdopt { .. }
            | SessionError::RuntimeContextNotFound { .. }
            | SessionError::FinishBlocked { .. }
            | SessionError::Move(_) =>
                McpError::invalid_params(err.to_string(), None),
            _ =>
                McpError::internal_error(format!("session error: {err}"), None),
        }
    }

    fn move_plan_json(
        report: &memory_api::storage::move_kernel::MovePlan
    ) -> Result<serde_json::Value, McpError> {
        Ok(serde_json::json!({
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
                .map(|p| path_display(p))
                .collect::<Result<Vec<_>, _>>()?,
            "blockers": report.blockers,
            "captured_at": report.captured_at,
        }))
    }

    fn move_outcome_json(
        outcome: &memory_api::storage::move_kernel::MoveOutcome
    ) -> Result<serde_json::Value, McpError> {
        Ok(serde_json::json!({
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
}

fn path_display(path: &std::path::Path) -> Result<String, McpError> {
    workspace::normalize_path_for_display_strict(path).map_err(|error| {
        McpError::invalid_params(
            format!(
                "path payload normalization failed for '{}': {error}",
                path.display()
            ),
            None,
        )
    })
}

fn parse_node_kind(value: &str) -> Result<SessionWorkflowNodeKind, McpError> {
    match value {
        "ticket" => Ok(SessionWorkflowNodeKind::Ticket),
        "action" => Ok(SessionWorkflowNodeKind::Action),
        "decision" => Ok(SessionWorkflowNodeKind::Decision),
        "checkpoint" => Ok(SessionWorkflowNodeKind::Checkpoint),
        "validation" => Ok(SessionWorkflowNodeKind::Validation),
        _ => Err(McpError::invalid_params(
            format!("invalid workflow node kind: {value}"),
            None,
        )),
    }
}

fn parse_requirement(
    value: &str
) -> Result<SessionWorkflowNodeRequirement, McpError> {
    match value {
        "required" => Ok(SessionWorkflowNodeRequirement::Required),
        "optional" => Ok(SessionWorkflowNodeRequirement::Optional),
        _ => Err(McpError::invalid_params(
            format!("invalid workflow requirement: {value}"),
            None,
        )),
    }
}

fn parse_edge_kind(value: &str) -> Result<SessionWorkflowEdgeKind, McpError> {
    match value {
        "depends-on" | "depends_on" => Ok(SessionWorkflowEdgeKind::DependsOn),
        "order" => Ok(SessionWorkflowEdgeKind::Order),
        _ => Err(McpError::invalid_params(
            format!("invalid workflow edge kind: {value}"),
            None,
        )),
    }
}

fn parse_node_status(
    value: &str
) -> Result<SessionWorkflowNodeStatus, McpError> {
    match value {
        "pending" => Ok(SessionWorkflowNodeStatus::Pending),
        "in-progress" | "in_progress" =>
            Ok(SessionWorkflowNodeStatus::InProgress),
        "blocked" => Ok(SessionWorkflowNodeStatus::Blocked),
        "done" => Ok(SessionWorkflowNodeStatus::Done),
        "deferred" => Ok(SessionWorkflowNodeStatus::Deferred),
        _ => Err(McpError::invalid_params(
            format!("invalid workflow status: {value}"),
            None,
        )),
    }
}

// ── Tool implementations ──────────────────────────────────────────────────────

#[tool_router]
impl SessionServer {
    #[tool(
        name = "session_runtime_init",
        description = "Initialize or resume durable runtime context for a workspace session."
    )]
    pub async fn session_runtime_init(
        &self,
        Parameters(input): Parameters<RuntimeInitInput>,
    ) -> Result<CallToolResult, McpError> {
        let result = self
            .config_for_workspace(&input.workspace)?
            .init_runtime_context(SessionRuntimeInitRequest {
                workspace_session_id: input.workspace_session_id,
                predecessor_run_id: input.predecessor_run_id,
                force_new_run: input.force_new_run,
            })
            .map_err(Self::session_err)?;
        Self::json_result(&result)
    }

    #[tool(
        name = "session_runtime_resume",
        description = "Resume an existing durable workspace session using predecessor run lineage."
    )]
    pub async fn session_runtime_resume(
        &self,
        Parameters(input): Parameters<RuntimeResumeInput>,
    ) -> Result<CallToolResult, McpError> {
        let result = self
            .config_for_workspace(&input.workspace)?
            .resume_workspace_context(
                &input.workspace_session_id,
                &input.predecessor_run_id,
            )
            .map_err(Self::session_err)?;
        Self::json_result(&result)
    }

    #[tool(
        name = "session_runtime_pin",
        description = "Pin an entity URN into runtime workspace context."
    )]
    pub async fn session_runtime_pin(
        &self,
        Parameters(input): Parameters<RuntimePinInput>,
    ) -> Result<CallToolResult, McpError> {
        let context = self
            .config_for_workspace(&input.workspace)?
            .pin_runtime_entity(
                &input.workspace_session_id,
                &input.entity_urn,
                input.relation,
                input.reason,
            )
            .map_err(Self::session_err)?;
        Self::json_result(&context)
    }

    #[tool(
        name = "session_runtime_unpin",
        description = "Unpin an entity URN from runtime workspace context."
    )]
    pub async fn session_runtime_unpin(
        &self,
        Parameters(input): Parameters<RuntimeUnpinInput>,
    ) -> Result<CallToolResult, McpError> {
        let context = self
            .config_for_workspace(&input.workspace)?
            .unpin_runtime_entity(
                &input.workspace_session_id,
                &input.entity_urn,
            )
            .map_err(Self::session_err)?;
        Self::json_result(&context)
    }

    #[tool(
        name = "session_runtime_view",
        description = "Read headers-only runtime workspace context view."
    )]
    pub async fn session_runtime_view(
        &self,
        Parameters(input): Parameters<RuntimeViewInput>,
    ) -> Result<CallToolResult, McpError> {
        let view = self
            .config_for_workspace(&input.workspace)?
            .view_runtime_context(&input.workspace_session_id)
            .map_err(Self::session_err)?;
        Self::json_result(&view)
    }

    #[tool(
        name = "session_runtime_render_instructions",
        description = "Render a focused instruction set from the workspace session's pinned rule URNs."
    )]
    pub async fn session_runtime_render_instructions(
        &self,
        Parameters(input): Parameters<RuntimeViewInput>,
    ) -> Result<CallToolResult, McpError> {
        let render = self
            .config_for_workspace(&input.workspace)?
            .render_pinned_rule_instructions(&input.workspace_session_id)
            .map_err(Self::session_err)?;
        Self::json_result(&serde_json::json!({"render": render}))
    }

    #[tool(
        name = "session_workflow_add_node",
        description = "Add a node to the durable session workflow graph."
    )]
    pub async fn session_workflow_add_node(
        &self,
        Parameters(input): Parameters<WorkflowAddNodeInput>,
    ) -> Result<CallToolResult, McpError> {
        let context = self
            .config_for_workspace(&input.workspace)?
            .workflow_add_node(
                &input.workspace_session_id,
                SessionWorkflowNodeDraft {
                    node_id: input.node_id,
                    kind: parse_node_kind(&input.kind)?,
                    requirement: parse_requirement(&input.requirement)?,
                    title: input.title,
                    ticket_urn: input.ticket_urn,
                    cached_ticket_title: input.cached_ticket_title,
                    validation_spec_id: input.validation_spec_id,
                },
            )
            .map_err(Self::session_err)?;
        Self::json_result(&context)
    }

    #[tool(
        name = "session_workflow_add_edge",
        description = "Add a directed edge between workflow nodes."
    )]
    pub async fn session_workflow_add_edge(
        &self,
        Parameters(input): Parameters<WorkflowAddEdgeInput>,
    ) -> Result<CallToolResult, McpError> {
        let context = self
            .config_for_workspace(&input.workspace)?
            .workflow_add_edge(
                &input.workspace_session_id,
                &input.from,
                &input.to,
                parse_edge_kind(&input.kind)?,
            )
            .map_err(Self::session_err)?;
        Self::json_result(&context)
    }

    #[tool(
        name = "session_workflow_set_status",
        description = "Update workflow node status and optional deferred reason."
    )]
    pub async fn session_workflow_set_status(
        &self,
        Parameters(input): Parameters<WorkflowSetStatusInput>,
    ) -> Result<CallToolResult, McpError> {
        let context = self
            .config_for_workspace(&input.workspace)?
            .workflow_update_node_status(
                &input.workspace_session_id,
                &input.node_id,
                parse_node_status(&input.status)?,
                input.deferred_reason,
            )
            .map_err(Self::session_err)?;
        Self::json_result(&context)
    }

    #[tool(
        name = "session_workflow_promote",
        description = "Promote a workflow node to a ticket-backed node while preserving identity."
    )]
    pub async fn session_workflow_promote(
        &self,
        Parameters(input): Parameters<WorkflowPromoteInput>,
    ) -> Result<CallToolResult, McpError> {
        let context = self
            .config_for_workspace(&input.workspace)?
            .workflow_promote_node_to_ticket(
                &input.workspace_session_id,
                &input.node_id,
                &input.ticket_urn,
                input.cached_ticket_title,
            )
            .map_err(Self::session_err)?;
        Self::json_result(&context)
    }

    #[tool(
        name = "session_workflow_render_terminal",
        description = "Render the workflow graph as deterministic terminal text."
    )]
    pub async fn session_workflow_render_terminal(
        &self,
        Parameters(input): Parameters<WorkflowRenderInput>,
    ) -> Result<CallToolResult, McpError> {
        let render = self
            .config_for_workspace(&input.workspace)?
            .workflow_render_terminal(&input.workspace_session_id, None)
            .map_err(Self::session_err)?;
        Self::json_result(&serde_json::json!({"render": render}))
    }

    #[tool(
        name = "session_workflow_render_mermaid",
        description = "Render the workflow graph as deterministic Mermaid flowchart text."
    )]
    pub async fn session_workflow_render_mermaid(
        &self,
        Parameters(input): Parameters<WorkflowRenderInput>,
    ) -> Result<CallToolResult, McpError> {
        let render = self
            .config_for_workspace(&input.workspace)?
            .workflow_render_mermaid(&input.workspace_session_id, None)
            .map_err(Self::session_err)?;
        Self::json_result(&serde_json::json!({"render": render}))
    }

    #[tool(
        name = "session_handoff",
        description = "Persist structured handoff record before rendering handoff summary."
    )]
    pub async fn session_handoff(
        &self,
        Parameters(input): Parameters<RuntimeHandoffInput>,
    ) -> Result<CallToolResult, McpError> {
        let result = self
            .config_for_workspace(&input.workspace)?
            .create_handoff_result(
                &input.workspace_session_id,
                input.validation.into_iter().map(Into::into).collect(),
                None,
            )
            .map_err(Self::session_err)?;
        Self::json_result(&result)
    }

    #[tool(
        name = "session_finish",
        description = "Explicitly finish workflow, enforcing required node and validation gates."
    )]
    pub async fn session_finish(
        &self,
        Parameters(input): Parameters<RuntimeFinishInput>,
    ) -> Result<CallToolResult, McpError> {
        let result = self
            .config_for_workspace(&input.workspace)?
            .finish_workflow(
                &input.workspace_session_id,
                input.validation.into_iter().map(Into::into).collect(),
                input.deferred_optional_node_ids,
                None,
            )
            .map_err(Self::session_err)?;
        Self::json_result(&result)
    }

    #[tool(
        name = "session_check_in",
        description = "Check a session into its authoritative worktree assignment and return the resolved receipt."
    )]
    pub async fn session_check_in(
        &self,
        Parameters(input): Parameters<CheckInInput>,
    ) -> Result<CallToolResult, McpError> {
        let receipt = self
            .config_for_workspace(&input.workspace)?
            .check_in_worktree(SessionWorktreeCheckInRequest {
                session_id: input.session_id,
                owner_id: input.owner_id,
                ticket_id: input.ticket_id,
                worktree_path: PathBuf::from(input.worktree_path),
                branch: input.branch,
                predecessor_session_id: input.predecessor_session_id,
            })
            .map_err(Self::session_err)?;
        Self::json_result(&receipt)
    }

    #[tool(
        name = "session_lookup",
        description = "Look up the authoritative worktree assignment for a session."
    )]
    pub async fn session_lookup(
        &self,
        Parameters(input): Parameters<LookupInput>,
    ) -> Result<CallToolResult, McpError> {
        let receipt = self
            .config()
            .lookup_worktree(&input.session_id)
            .map_err(Self::session_err)?;
        Self::json_result(&receipt)
    }

    #[tool(
        name = "session_query",
        description = "Query stored sessions with optional id-prefix, conversation, agent, text, and limit filters."
    )]
    pub async fn session_query(
        &self,
        Parameters(input): Parameters<QueryInput>,
    ) -> Result<CallToolResult, McpError> {
        let query = SessionQuery {
            session_id_prefix: input.session_id_prefix,
            conversation_id: input.conversation_id,
            agent_id: input.agent_id,
            text: input.text,
            limit: input.limit,
        };
        let sessions = self
            .config()
            .query_sessions(&query)
            .map_err(Self::session_err)?;
        Self::json_result(&serde_json::json!({
            "count": sessions.len(),
            "sessions": sessions,
        }))
    }

    #[tool(
        name = "session_peek_range",
        description = "Peek a bounded window of transcript turns for a session."
    )]
    pub async fn session_peek_range(
        &self,
        Parameters(input): Parameters<PeekRangeInput>,
    ) -> Result<CallToolResult, McpError> {
        let range = self
            .config()
            .peek_range(&input.session_id, input.start, input.end)
            .map_err(Self::session_err)?;
        Self::json_result(&range)
    }

    #[tool(
        name = "session_peek_skeleton",
        description = "Peek a body-stripped skeleton overview of a session transcript."
    )]
    pub async fn session_peek_skeleton(
        &self,
        Parameters(input): Parameters<PeekSkeletonInput>,
    ) -> Result<CallToolResult, McpError> {
        let preview_chars = input
            .preview_chars
            .unwrap_or(DEFAULT_SKELETON_PREVIEW_CHARS);
        let skeleton = self
            .config()
            .peek_skeleton(&input.session_id, preview_chars)
            .map_err(Self::session_err)?;
        Self::json_result(&skeleton)
    }

    #[tool(
        name = "session_move_preflight",
        description = "Read-only preflight plan for moving a session to another workspace store."
    )]
    pub async fn session_move_preflight(
        &self,
        Parameters(input): Parameters<SessionMoveInput>,
    ) -> Result<CallToolResult, McpError> {
        let session_id = input.id.parse::<Uuid>().map_err(|error| {
            McpError::invalid_params(
                format!("invalid session UUID: {error}"),
                None,
            )
        })?;
        let target_workspace_root = workspace::canonicalize_workspace_root_strict(
            std::path::Path::new(&input.to_workspace_root),
        )
        .map_err(|error| {
            McpError::invalid_params(
                format!(
                    "workspace root canonicalization failed for '{}': {error}",
                    input.to_workspace_root
                ),
                None,
            )
        })?;
        let report = self
            .config()
            .plan_move_preflight(&session_id, &target_workspace_root)
            .map_err(Self::session_err)?;

        Self::json_result(&serde_json::json!({
            "command": "move",
            "status": if report.supported() { "ok" } else { "blocked" },
            "mode": "preflight",
            "id": session_id,
            "plan": Self::move_plan_json(&report)?,
            "recovery": {"resume": "session move --resume <journal-uuid>", "rollback": "session move --rollback <journal-uuid>"},
        }))
    }

    #[tool(
        name = "session_move_apply",
        description = "Execute a supported session move to another workspace store."
    )]
    pub async fn session_move_apply(
        &self,
        Parameters(input): Parameters<SessionMoveInput>,
    ) -> Result<CallToolResult, McpError> {
        let session_id = input.id.parse::<Uuid>().map_err(|error| {
            McpError::invalid_params(
                format!("invalid session UUID: {error}"),
                None,
            )
        })?;
        let target_workspace_root = workspace::canonicalize_workspace_root_strict(
            std::path::Path::new(&input.to_workspace_root),
        )
        .map_err(|error| {
            McpError::invalid_params(
                format!(
                    "workspace root canonicalization failed for '{}': {error}",
                    input.to_workspace_root
                ),
                None,
            )
        })?;
        let report = self
            .config()
            .plan_move_preflight(&session_id, &target_workspace_root)
            .map_err(Self::session_err)?;
        if !report.supported() {
            return Err(McpError::invalid_params(
                "move preflight blocked; run session_move_preflight for details".to_string(),
                None,
            ));
        }
        let outcome = self
            .config()
            .execute_move_with_journal(&report)
            .map_err(Self::session_err)?;

        Self::json_result(&serde_json::json!({
            "command": "move",
            "status": "ok",
            "mode": "apply",
            "id": session_id,
            "plan": Self::move_plan_json(&report)?,
            "outcome": Self::move_outcome_json(&outcome)?,
            "recovery": {"resume": "session move --resume <journal-uuid>", "rollback": "session move --rollback <journal-uuid>"},
        }))
    }

    #[tool(
        name = "session_move_resume",
        description = "Resume an interrupted session move from a journal id."
    )]
    pub async fn session_move_resume(
        &self,
        Parameters(input): Parameters<SessionMoveJournalInput>,
    ) -> Result<CallToolResult, McpError> {
        let journal = input.id.parse::<Uuid>().map_err(|error| {
            McpError::invalid_params(
                format!("invalid journal id: {error}"),
                None,
            )
        })?;
        let outcome = self
            .config()
            .resume_move_with_journal(journal)
            .map_err(Self::session_err)?;

        Self::json_result(&serde_json::json!({
            "command": "move",
            "status": "ok",
            "mode": "resume",
            "journal_id": outcome.journal.id,
            "phase": outcome.journal.phase,
            "recovery": {"resume": "session move --resume <journal-uuid>", "rollback": "session move --rollback <journal-uuid>"},
        }))
    }

    #[tool(
        name = "session_move_rollback",
        description = "Roll back a session move from a journal id."
    )]
    pub async fn session_move_rollback(
        &self,
        Parameters(input): Parameters<SessionMoveJournalInput>,
    ) -> Result<CallToolResult, McpError> {
        let journal = input.id.parse::<Uuid>().map_err(|error| {
            McpError::invalid_params(
                format!("invalid journal id: {error}"),
                None,
            )
        })?;
        let outcome = self
            .config()
            .rollback_move_with_journal(journal)
            .map_err(Self::session_err)?;

        Self::json_result(&serde_json::json!({
            "command": "move",
            "status": "ok",
            "mode": "rollback",
            "journal_id": outcome.journal.id,
            "phase": outcome.journal.phase,
            "recovery": {"resume": "session move --resume <journal-uuid>", "rollback": "session move --rollback <journal-uuid>"},
        }))
    }
}

// ── MCP handler trait ─────────────────────────────────────────────────────────

#[tool_handler]
impl ServerHandler for SessionServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: env!("CARGO_PKG_NAME").to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            instructions: Some(
                "session-mcp provides direct access to the session store. Use named tools for session worktree check-in, lookup, query, move, and transcript peeking."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ── Server startup ────────────────────────────────────────────────────────────

pub async fn run_mcp_server(
    store_root: PathBuf,
    workspace_slug: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = SessionServer::new(store_root, workspace_slug);

    tracing::info!(
        "Starting session-mcp server on stdio (direct store access)"
    );

    let service = server.serve(stdio()).await.inspect_err(|err| {
        eprintln!("Server error: {err:?}");
    })?;

    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;
    use serde_json::Value;
    use session_api::{
        CopilotHookMessage,
        CopilotHookPayload,
        SessionCaptureRequest,
        SessionError,
        SessionRole,
        SessionStoreConfig,
    };
    use std::process::Command;
    use tempfile::tempdir;

    use super::*;

    fn seed(
        config: &SessionStoreConfig,
        session_id: &str,
        agent: &str,
    ) {
        let payload = CopilotHookPayload {
            session_id: session_id.to_string(),
            workspace_slug: "default".to_string(),
            captured_at: Utc::now(),
            conversation_id: None,
            agent_id: Some(agent.to_string()),
            model: None,
            trigger: None,
            messages: vec![CopilotHookMessage {
                role: SessionRole::User,
                content: "alpha body\nbeta".to_string(),
                tool_name: None,
                captured_at: None,
                event_meta: None,
            }],
            events: vec![],
            runtime: None,
        };
        config
            .persist_capture(SessionCaptureRequest::copilot(payload))
            .expect("seed");
    }

    fn extract_json(result: rmcp::model::CallToolResult) -> Value {
        let text = result
            .content
            .iter()
            .find_map(|content| {
                if let rmcp::model::RawContent::Text(text) = &content.raw {
                    Some(text.text.clone())
                } else {
                    None
                }
            })
            .expect("text content");
        serde_json::from_str(&text).expect("parse json")
    }

    #[tokio::test]
    async fn check_in_then_lookup() {
        let dir = tempdir().unwrap();
        let store_root = dir.path().join(".session");
        let worktree = dir.path().join("wt");
        let server =
            SessionServer::new(store_root.clone(), "default".to_string());

        let receipt = server
            .session_check_in(Parameters(CheckInInput {
                workspace: store_root.display().to_string(),
                session_id: "s1".to_string(),
                owner_id: "agent".to_string(),
                ticket_id: "t1".to_string(),
                worktree_path: worktree.to_string_lossy().to_string(),
                branch: "feature/x".to_string(),
                predecessor_session_id: None,
            }))
            .await
            .expect("check-in");
        assert!(!receipt.is_error.unwrap_or(false));

        let lookup = server
            .session_lookup(Parameters(LookupInput {
                session_id: "s1".to_string(),
            }))
            .await
            .expect("lookup");
        assert!(!lookup.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn runtime_render_instructions_returns_only_pinned_rules() {
        let dir = tempdir().unwrap();
        let session_root = dir.path().join(".session");
        let config = SessionStoreConfig::new(&session_root, "default");
        let init = config
            .init_runtime_context(Default::default())
            .expect("init runtime");
        let mut rule_store =
            rule_api::RuleStore::open_or_init(&dir.path().join(".rule"))
                .expect("rule store");
        let rule = rule_api::RuleManifest::new(
            "session/mcp/render",
            "MCP render",
            ".instructions",
            "mcp-render",
            "Pinned MCP guidance.",
        );
        let rule_id = rule_store.create(&rule, None).expect("create rule");
        config
            .pin_runtime_entity(
                &init.context.workspace_session_id,
                &format!("ce://default/rules/{rule_id}"),
                None,
                None,
            )
            .expect("pin rule");
        let server = SessionServer::new(session_root.clone(), "default".into());

        let result = server
            .session_runtime_render_instructions(Parameters(RuntimeViewInput {
                workspace: session_root.display().to_string(),
                workspace_session_id: init.context.workspace_session_id,
            }))
            .await
            .expect("render instructions");
        let payload = extract_json(result);
        assert!(
            payload["render"]
                .as_str()
                .unwrap()
                .contains("Pinned MCP guidance.")
        );
    }

    #[tokio::test]
    async fn query_and_peek() {
        let dir = tempdir().unwrap();
        let store_root = dir.path().join(".session");
        let server =
            SessionServer::new(store_root.clone(), "default".to_string());
        let config = SessionStoreConfig::new(store_root, "default".to_string());
        seed(&config, "s2", "agent-2");

        let query = server
            .session_query(Parameters(QueryInput {
                session_id_prefix: None,
                conversation_id: None,
                agent_id: Some("agent-2".to_string()),
                text: None,
                limit: None,
            }))
            .await
            .expect("query");
        assert!(!query.is_error.unwrap_or(false));

        let skeleton = server
            .session_peek_skeleton(Parameters(PeekSkeletonInput {
                session_id: "s2".to_string(),
                preview_chars: None,
            }))
            .await
            .expect("skeleton");
        assert!(!skeleton.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn move_preflight_and_apply_roundtrip() {
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
        seed(&config, session_id, "agent-3");

        let server = SessionServer::new(
            source_store_root.clone(),
            "default".to_string(),
        );
        let preflight = server
            .session_move_preflight(Parameters(SessionMoveInput {
                id: session_id.to_string(),
                to_workspace_root: target_workspace_root
                    .to_string_lossy()
                    .to_string(),
            }))
            .await
            .expect("move preflight");
        let preflight_json = extract_json(preflight);
        assert_eq!(preflight_json["status"], "ok");
        assert_eq!(preflight_json["mode"], "preflight");
        assert!(preflight_json["plan"]["supported"].as_bool().unwrap());

        let apply = server
            .session_move_apply(Parameters(SessionMoveInput {
                id: session_id.to_string(),
                to_workspace_root: target_workspace_root
                    .to_string_lossy()
                    .to_string(),
            }))
            .await
            .expect("move apply");
        let apply_json = extract_json(apply);
        assert_eq!(apply_json["status"], "ok");
        assert_eq!(apply_json["mode"], "apply");
        assert!(apply_json["outcome"]["journal"]["id"].is_string());

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
}
