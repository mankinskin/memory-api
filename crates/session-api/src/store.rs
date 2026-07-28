use std::{
    collections::{
        BTreeMap,
        BTreeSet,
    },
    fs,
    io::ErrorKind,
    path::{
        Path,
        PathBuf,
    },
};
use uuid::Uuid;

use serde::{
    Deserialize,
    Serialize,
    de::DeserializeOwned,
};

use crate::{
    CopilotHookPayload,
    SESSION_SCHEMA_VERSION,
    SessionAuditReport,
    SessionAuditSelector,
    SessionCaptureRequest,
    SessionError,
    SessionFinishRecord,
    SessionFinishResult,
    SessionHandoffPackage,
    SessionHandoffRecord,
    SessionHandoffResult,
    SessionLinks,
    SessionMetadata,
    SessionPinFeedbackSink,
    SessionPinnedEntity,
    SessionPinnedEntityHeader,
    SessionPinnedEntityKind,
    SessionRecord,
    SessionRunLineage,
    SessionRuntimeContext,
    SessionRuntimeInitRequest,
    SessionRuntimeInitResult,
    SessionRuntimeView,
    SessionTicketStateResolver,
    SessionTurn,
    SessionValidationGate,
    SessionWorkflowDiagnostic,
    SessionWorkflowEdge,
    SessionWorkflowEdgeKind,
    SessionWorkflowNode,
    SessionWorkflowNodeDraft,
    SessionWorkflowNodeResolution,
    SessionWorkflowNodeStatus,
    SessionWorkflowSnapshot,
    SessionWorktreeAllocationMode,
    SessionWorktreeAssignment,
    SessionWorktreeStatus,
    audit::build_session_audit_report,
    hook::{
        CopilotHookEvent,
        copilot_payload_from_transcript_path,
    },
    peek::{
        PromptPackOptions,
        SessionPromptPack,
        SessionSkeleton,
        SessionTurnRange,
        peek_prompt_pack,
        peek_skeleton,
        peek_turn_range,
    },
};
use rule_api::RuleStore;
use spec_api::SpecStore;
use test_api::{
    ExecutionQuery,
    TestStoreConfig,
    ValidationOutcome,
};
use ticket_api::storage::TicketStore;

#[path = "store_persistence_types.rs"]
mod store_persistence_types;
pub use store_persistence_types::*;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionQuery {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id_prefix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionStoreConfig {
    pub root: PathBuf,
    pub workspace_slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorktreeCheckInRequest {
    pub session_id: String,
    pub owner_id: String,
    pub ticket_id: String,
    pub worktree_path: PathBuf,
    pub branch: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_session_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionWorktreeCheckInReceipt {
    pub session_id: String,
    pub owner_id: String,
    pub ticket_id: String,
    pub worktree_path: PathBuf,
    pub branch: String,
    pub allocation_mode: SessionWorktreeAllocationMode,
    pub status: SessionWorktreeStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor_path: Option<PathBuf>,
}

mod config {
    use super::*;

    include!("store/config/capture_query.rs");
    include!("store/config/worktree_runtime.rs");
    include!("store/config/runtime_workflow.rs");
    include!("store/config/workflow.rs");
    include!("store/config/handoff_finish.rs");
    include!("store/config/persistence.rs");
    include!("store/config/worktree_conflicts.rs");
    include!("store/config/tool_metrics.rs");
    include!("store/config/subagent_rollup_query.rs");
}

#[path = "store_routing_types.rs"]
mod store_routing_types;
pub use store_routing_types::{
    SessionRuntimePaths,
    SessionStorePaths,
};
use store_routing_types::{
    parse_entity_urn,
    parse_entity_urn_kind,
    validate_runtime_workspace_id,
};

fn sibling_store_root(
    session_store_root: &Path,
    sibling_store_dir: &str,
) -> PathBuf {
    if session_store_root
        .file_name()
        .and_then(|name| name.to_str())
        == Some(".session")
    {
        if let Some(parent) = session_store_root.parent() {
            return parent.join(sibling_store_dir);
        }
    }
    session_store_root.join(sibling_store_dir)
}

/// RAII guard that releases the runtime mutation lock on drop.
struct RuntimeMutationLock {
    file: fs::File,
}

impl Drop for RuntimeMutationLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

struct DefaultTicketStateResolver {
    store: TicketStore,
    spec_store_root: PathBuf,
    workspace_slug: String,
}

impl SessionTicketStateResolver for DefaultTicketStateResolver {
    fn resolve_ticket_state(
        &self,
        ticket_urn: &str,
    ) -> Result<Option<String>, String> {
        let parsed =
            parse_entity_urn(ticket_urn).map_err(|error| error.to_string())?;
        if parsed.kind != SessionPinnedEntityKind::Ticket {
            return Err(format!("not a ticket URN: {ticket_urn}"));
        }
        // The default resolver only queries the sibling `.ticket` store for the
        // session's own workspace. Cross-workspace ticket URNs are rejected
        // explicitly rather than silently resolved against the wrong store.
        if parsed.workspace_slug != self.workspace_slug {
            return Err(format!(
                "unsupported cross-workspace ticket routing: URN workspace `{}` \
                 does not match session workspace `{}` ({ticket_urn})",
                parsed.workspace_slug, self.workspace_slug
            ));
        }
        let ticket_id =
            Uuid::parse_str(&parsed.entity_id).map_err(|error| {
                format!("invalid ticket id in URN {ticket_urn}: {error}")
            })?;
        match self
            .store
            .get_indexed(&ticket_id)
            .map_err(|error| error.to_string())?
        {
            // A resolved ticket may legitimately have no recorded state; keep that
            // distinct from an absent ticket, which is an unavailable-state error.
            Some(indexed) => Ok(indexed.state),
            None => Err(format!("required ticket not found: {ticket_urn}")),
        }
    }

    fn resolve_spec_state(
        &self,
        spec_urn: &str,
    ) -> Result<Option<String>, String> {
        let parsed =
            parse_entity_urn(spec_urn).map_err(|error| error.to_string())?;
        if parsed.kind != SessionPinnedEntityKind::Spec {
            return Err(format!("not a spec URN: {spec_urn}"));
        }
        // Symmetric to ticket routing: the default resolver only reads the
        // sibling `.spec` store for the session's own workspace.
        if parsed.workspace_slug != self.workspace_slug {
            return Err(format!(
                "unsupported cross-workspace spec routing: URN workspace `{}` \
                 does not match session workspace `{}` ({spec_urn})",
                parsed.workspace_slug, self.workspace_slug
            ));
        }
        // Open the spec store lazily so sessions with no spec nodes never
        // require an initialized `.spec` store.
        let store = SpecStore::open(&self.spec_store_root)
            .map_err(|error| error.to_string())?;
        let manifest = store
            .get(&parsed.entity_id)
            .map_err(|error| format!("required spec not found: {error}"))?;
        Ok(manifest.state().map(str::to_string))
    }
}

fn validation_outcome_label(outcome: ValidationOutcome) -> String {
    match outcome {
        ValidationOutcome::Passed => "passed".to_string(),
        ValidationOutcome::Failed => "failed".to_string(),
        ValidationOutcome::Blocked => "blocked".to_string(),
    }
}

fn node_is_effectively_done(
    node: &SessionWorkflowNode,
    live_state: Option<&Option<String>>,
) -> bool {
    if node.kind == crate::SessionWorkflowNodeKind::Ticket {
        // Ticket-backed nodes derive completion exclusively from authoritative
        // live terminal state. Local `Done` status is display/cache only and can
        // never certify completion. When live state is missing or unavailable
        // (no resolution recorded), the node fails closed and is not done.
        return matches!(
            live_state.and_then(|value| value.as_deref()),
            Some("done") | Some("cancelled")
        );
    }

    if node.kind == crate::SessionWorkflowNodeKind::Spec {
        // Spec-backed nodes are symmetric to tickets: completion is certified
        // only by the authoritative live spec terminal state. `verified` is the
        // spec success terminal; `deprecated` and `cancelled` are terminal exit
        // paths. Any other or unavailable state fails closed.
        return matches!(
            live_state.and_then(|value| value.as_deref()),
            Some("verified") | Some("deprecated") | Some("cancelled")
        );
    }

    // Session-only nodes (task/validation) use local status.
    node.status == SessionWorkflowNodeStatus::Done
}

fn render_handoff_record_terminal(record: &SessionHandoffRecord) -> String {
    let mut lines = Vec::new();
    lines.push(format!("handoff {}", record.handoff_id));
    lines.push(format!(
        "workspace_session_id: {}",
        record.workspace_session_id
    ));
    lines.push(format!("outgoing_run_id: {}", record.outgoing_run_id));
    lines.push(format!("resume: {}", record.resume_command));
    if !record.objective.is_empty() {
        lines.push(format!("objective: {}", record.objective));
    }
    if !record.target_tickets.is_empty() {
        lines.push(format!(
            "target_tickets: {}",
            record.target_tickets.join(", ")
        ));
    }
    if !record.target_files.is_empty() {
        lines.push(format!(
            "target_files: {}",
            record.target_files.join(", ")
        ));
    }
    if !record.open_escalations.is_empty() {
        lines.push(format!(
            "open_escalations: {}",
            record.open_escalations.join(", ")
        ));
        lines.push("implementation_ready: false".to_string());
    } else if !record.objective.is_empty() {
        lines.push("implementation_ready: true".to_string());
    }
    lines.push("workflow:".to_string());
    lines.push(format!("  nodes: {}", record.workflow.workflow.nodes.len()));
    lines.push(format!("  edges: {}", record.workflow.workflow.edges.len()));
    let blocked = record
        .workflow
        .workflow
        .nodes
        .iter()
        .filter(|node| node.status != SessionWorkflowNodeStatus::Done)
        .count();
    lines.push(format!("  not_done_nodes: {}", blocked));
    for pin in &record.pinned_entities {
        lines.push(format!(
            "pin {} {}",
            pin.urn,
            format!("{:?}", pin.kind).to_lowercase()
        ));
    }
    for gate in &record.validation {
        lines.push(format!(
            "validation {} required={} outcome={}",
            gate.validation_spec_id,
            gate.required,
            gate.outcome.as_deref().unwrap_or("-")
        ));
    }
    for diag in &record.workflow.diagnostics {
        lines.push(format!(
            "diag {} {} {}",
            diag.node_id, diag.code, diag.message
        ));
    }
    lines.join("\n")
}

fn render_handoff_record_markdown(record: &SessionHandoffRecord) -> String {
    let mut sections = Vec::new();

    // Header
    sections.push(format!("# Handoff: {}", record.handoff_id));
    sections.push(String::new());

    // Summary section
    sections.push("## Summary".to_string());
    sections.push(format!("- **Workspace Session**: `{}`", record.workspace_session_id));
    sections.push(format!("- **Outgoing Run**: `{}`", record.outgoing_run_id));
    sections.push(format!("- **Created**: {}", record.created_at.to_rfc3339()));
    if !record.objective.is_empty() {
        sections.push(format!("- **Objective**: {}", record.objective));
    }
    let implementation_ready = !record.objective.is_empty() && record.open_escalations.is_empty();
    sections.push(format!("- **Implementation Ready**: {}", implementation_ready));
    sections.push(String::new());

    // Resume command
    sections.push("## Resume Command".to_string());
    sections.push("```bash".to_string());
    sections.push(record.resume_command.clone());
    sections.push("```".to_string());
    sections.push(String::new());

    // Target Tickets
    if !record.target_tickets.is_empty() {
        sections.push("## Target Tickets".to_string());
        for ticket in &record.target_tickets {
            sections.push(format!("- `{}`", ticket));
        }
        sections.push(String::new());
    }

    // Target Files
    if !record.target_files.is_empty() {
        sections.push("## Target Files".to_string());
        for file in &record.target_files {
            sections.push(format!("- `{}`", file));
        }
        sections.push(String::new());
    }

    // Decisions
    if !record.decisions.is_empty() {
        sections.push("## Decisions".to_string());
        for decision in &record.decisions {
            sections.push(format!("- {}", decision));
        }
        sections.push(String::new());
    }

    // Non-Goals
    if !record.non_goals.is_empty() {
        sections.push("## Non-Goals".to_string());
        for non_goal in &record.non_goals {
            sections.push(format!("- {}", non_goal));
        }
        sections.push(String::new());
    }

    // Context Anchors
    if !record.context_anchors.is_empty() {
        sections.push("## Context Anchors".to_string());
        for anchor in &record.context_anchors {
            sections.push(format!("- {}", anchor));
        }
        sections.push(String::new());
    }

    // Open Escalations
    if !record.open_escalations.is_empty() {
        sections.push("## ⚠️ Open Escalations".to_string());
        for escalation in &record.open_escalations {
            sections.push(format!("- {}", escalation));
        }
        sections.push(String::new());
    }

    // Risk Notes
    if let Some(ref risk_notes) = record.risk_notes {
        sections.push("## Risk Notes".to_string());
        sections.push(risk_notes.clone());
        sections.push(String::new());
    }

    // Workflow
    sections.push("## Workflow".to_string());
    sections.push(format!("- **Nodes**: {}", record.workflow.workflow.nodes.len()));
    sections.push(format!("- **Edges**: {}", record.workflow.workflow.edges.len()));
    let not_done = record
        .workflow
        .workflow
        .nodes
        .iter()
        .filter(|node| node.status != SessionWorkflowNodeStatus::Done)
        .count();
    sections.push(format!("- **Not Done**: {}", not_done));
    sections.push(String::new());

    // Pinned Entities
    if !record.pinned_entities.is_empty() {
        sections.push("## Pinned Entities".to_string());
        for pin in &record.pinned_entities {
            sections.push(format!("- `{}` ({})", pin.urn, format!("{:?}", pin.kind).to_lowercase()));
        }
        sections.push(String::new());
    }

    // Validation
    if !record.validation.is_empty() {
        sections.push("## Validation".to_string());
        for gate in &record.validation {
            let outcome = gate.outcome.as_deref().unwrap_or("-");
            let required = if gate.required { "required" } else { "optional" };
            sections.push(format!("- `{}`: {} ({})", gate.validation_spec_id, outcome, required));
        }
        sections.push(String::new());
    }

    // Diagnostics
    if !record.workflow.diagnostics.is_empty() {
        sections.push("## Diagnostics".to_string());
        for diag in &record.workflow.diagnostics {
            sections.push(format!("- **{}** [{}]: {}", diag.node_id, diag.code, diag.message));
        }
        sections.push(String::new());
    }

    sections.join("\n")
}

fn sort_workflow_graph(graph: &mut crate::SessionWorkflowGraph) {
    graph
        .nodes
        .sort_by(|left, right| left.node_id.cmp(&right.node_id));
    graph.edges.sort_by(|left, right| {
        left.from
            .cmp(&right.from)
            .then_with(|| left.to.cmp(&right.to))
            .then_with(|| {
                format!("{:?}", left.kind).cmp(&format!("{:?}", right.kind))
            })
    });
}

fn workflow_status_label(status: SessionWorkflowNodeStatus) -> &'static str {
    match status {
        SessionWorkflowNodeStatus::Pending => "pending",
        SessionWorkflowNodeStatus::InProgress => "in-progress",
        SessionWorkflowNodeStatus::Blocked => "blocked",
        SessionWorkflowNodeStatus::Done => "done",
        SessionWorkflowNodeStatus::Deferred => "deferred",
    }
}

fn mermaid_node_id(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len() + 2);
    out.push('n');
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else {
            out.push('_');
        }
    }
    out
}

fn escape_mermaid_label(label: &str) -> String {
    label
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', " ")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionStorePlan {
    pub record: SessionRecord,
    pub paths: SessionStorePaths,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub events: Option<PersistedSessionEvents>,
}

impl SessionStorePlan {
    pub fn manifest(&self) -> PersistedSessionManifest {
        PersistedSessionManifest::from(&self.record)
    }

    pub fn transcript(&self) -> PersistedSessionTranscript {
        PersistedSessionTranscript::from(&self.record)
    }

    pub fn persist(&self) -> Result<SessionStorePaths, SessionError> {
        fs::create_dir_all(&self.paths.session_dir).map_err(|source| {
            SessionError::Io {
                path: self.paths.session_dir.clone(),
                source,
            }
        })?;

        let manifest = merge_manifest(
            read_json_if_exists(&self.paths.manifest_path)?,
            self.manifest(),
        );
        let transcript = merge_transcript(
            read_json_if_exists(&self.paths.transcript_path)?,
            self.transcript(),
        )?;

        let merged_events = merge_events(
            read_json_if_exists(&self.paths.events_path)?,
            self.events.clone(),
            self.record.session_id.clone(),
            self.record.captured_at,
        )?;

        write_json(&self.paths.manifest_path, &manifest)?;
        write_json(&self.paths.transcript_path, &transcript)?;
        if let Some(events) = merged_events {
            write_json(&self.paths.events_path, &events)?;
        }

        // Populate tool-metrics.json immediately at capture time (ticket
        // b7c61f0e AC4) instead of only lazily on first aggregate read, so
        // newly captured sessions always have a non-empty, up-to-date
        // summary reflecting the full merged transcript.
        let merged_record = SessionRecord {
            schema_version: manifest.schema_version,
            session_id: manifest.session_id.clone(),
            source: manifest.source.clone(),
            started_at: manifest.started_at,
            captured_at: manifest.captured_at,
            metadata: manifest.metadata.clone(),
            turns: transcript.turns.clone(),
            links: manifest.links.clone(),
            track_id: manifest.track_id.clone(),
            anchor_ticket_id: manifest.anchor_ticket_id.clone(),
            parent_session_id: manifest.parent_session_id.clone(),
            spawned_session_id: manifest.spawned_session_id.clone(),
        };
        let estimator = crate::tool_metrics::CharsPerTokenEstimator::default();
        let summary =
            crate::tool_metrics::compute_session_summary(&merged_record, &estimator);
        write_json(&self.paths.session_dir.join("tool-metrics.json"), &summary)?;

        Ok(self.paths.clone())
    }
}

#[path = "store_helpers.rs"]
mod store_helpers;
use store_helpers::*;

#[cfg(test)]
#[path = "store_tests.rs"]
mod tests;
