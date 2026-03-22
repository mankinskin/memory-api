use serde::{Deserialize, Serialize};

pub const COMMAND_SCHEMA_VERSION: &str = "0";

/// Canonical `TaskCommand` names — used for both the human CLI adapter and the
/// machine-readable agent protocol.
///
/// Agent protocol uses the `task_` prefixed forms (e.g. `task_create`).
/// Human CLI and short exec use the bare forms (e.g. `create`).
/// Both are accepted by `ticket exec`; the `task_` prefix is stripped internally.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum TicketCommand {
    // ── human-CLI bare names ──────────────────────────────────────────────────
    Create,
    Get,
    Update,
    List,
    Delete,
    Scan,
    Claim,
    Unclaim,
    Search,
    Query,
    History,
    Diff,
    Revert,
    FinalizeMerge,
    ReadyOverview,
    Batch,
    // ── agent-protocol task_ names ────────────────────────────────────────────
    TaskCreate,
    TaskGet,
    TaskUpdate,
    TaskList,
    TaskDelete,
    TaskSearch,
    TaskClaim,
    TaskUnclaim,
    // ── validation & release protocol commands ────────────────────────────────
    TaskAssignmentStart,
    TaskValidateStart,
    TaskValidateResult,
    TaskReleaseCandidateCreate,
    TaskReleaseGateCheck,
    TaskReleasePromote,
    // ── workspace management ──────────────────────────────────────────────────
    WorkspaceList,
    WorkspaceNew,
    WorkspaceUse,
    WorkspaceCurrent,
    WorkspaceRemove,
    // ── fs watcher ──────────────────────────────────────────
    Watch,
}

impl TicketCommand {
    pub const fn names() -> &'static [&'static str] {
        &[
            // bare names (human CLI / short exec)
            "create",
            "get",
            "update",
            "list",
            "delete",
            "scan",
            "claim",
            "unclaim",
            "search",
            "query",
            "history",
            "diff",
            "revert",
            "finalize_merge",
            "ready_overview",
            "batch",
            // task_ names (agent protocol canonical forms)
            "task_create",
            "task_get",
            "task_update",
            "task_list",
            "task_delete",
            "task_search",
            "task_claim",
            "task_unclaim",
            // validation & release
            "task_validate_start",
            "task_validate_result",
            "task_release_candidate_create",
            "task_release_gate_check",
            "task_release_promote",
            // edge management
            "link",
            "links",
            // workspace management
            "workspace_list",
            "workspace_new",
            "workspace_use",
            "workspace_current",
            "workspace_remove",
            // fs watcher
            "watch",
            // coordinator dashboard
            "status",
            // assignment orchestration
            "task_assignment_start",
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandSchemaExport {
    pub version: String,
    pub command_namespace: String,
    pub commands: Vec<String>,
}

pub fn export_command_schema() -> CommandSchemaExport {
    CommandSchemaExport {
        version: COMMAND_SCHEMA_VERSION.to_string(),
        command_namespace: "ticket".to_string(),
        commands: TicketCommand::names()
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    }
}

pub fn export_command_schema_json() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&export_command_schema())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CommandEnvelope {
    pub request_id: String,
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorEnvelope {
    pub code: String,
    pub message: String,
}
