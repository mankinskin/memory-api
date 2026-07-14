pub mod audit;
pub mod error;
pub mod follow_up;
pub mod hook;
pub mod model;
pub mod move_domain;
pub mod peek;
pub mod store;
pub mod transcript_feedback;

pub use audit::{
    SessionAuditFinding,
    SessionAuditMetrics,
    SessionAuditReport,
    SessionAuditSelector,
    SessionAuditSeverity,
    SessionAuditToolCount,
};
pub use error::SessionError;
pub use follow_up::{
    FollowUpSynthesisOutcome,
    FollowUpTicketDraft,
    build_follow_up_ticket_draft,
    follow_up_ticket_id,
    synthesize_follow_up_ticket,
};
pub use hook::{
    CopilotHookEvent,
    CopilotHookMessage,
    CopilotHookPayload,
    CopilotRuntimeMetadata,
    SessionCaptureRequest,
    copilot_payload_from_transcript_path,
    copilot_payload_from_transcript_reader,
};
pub use model::{
    RUNTIME_CONTEXT_SCHEMA_VERSION,
    SESSION_SCHEMA_VERSION,
    SessionFinishRecord,
    SessionFinishResult,
    SessionHandoffRecord,
    SessionHandoffResult,
    SessionLinks,
    SessionMetadata,
    SessionPinFeedbackSink,
    SessionPinnedEntity,
    SessionPinnedEntityHeader,
    SessionPinnedEntityKind,
    SessionRecord,
    SessionRole,
    SessionRunLineage,
    SessionRuntimeContext,
    SessionRuntimeInitRequest,
    SessionRuntimeInitResult,
    SessionRuntimeView,
    SessionTicketStateResolver,
    SessionTurn,
    SessionTurnEventMeta,
    SessionValidationGate,
    SessionWorkflowDiagnostic,
    SessionWorkflowEdge,
    SessionWorkflowEdgeKind,
    SessionWorkflowGraph,
    SessionWorkflowNode,
    SessionWorkflowNodeDraft,
    SessionWorkflowNodeKind,
    SessionWorkflowNodeRequirement,
    SessionWorkflowNodeResolution,
    SessionWorkflowNodeStatus,
    SessionWorkflowSnapshot,
    SessionWorktreeAllocationMode,
    SessionWorktreeAssignment,
    SessionWorktreeStatus,
    default_runtime_context_schema_version,
    default_session_schema_version,
};
pub use peek::{
    DEFAULT_PROMPT_SUMMARIZE_THRESHOLD_CHARS,
    DEFAULT_SKELETON_PREVIEW_CHARS,
    PromptInclusion,
    PromptPackOptions,
    SessionPromptPack,
    SessionPromptPackEntry,
    SessionSkeleton,
    SessionSkeletonEntry,
    SessionTurnRange,
    peek_prompt_pack,
    peek_skeleton,
    peek_turn_range,
};
pub use store::{
    PersistedActiveWorkspaceSession,
    PersistedRuntimeContext,
    PersistedSessionEvents,
    PersistedSessionManifest,
    PersistedSessionTranscript,
    SessionQuery,
    SessionRuntimePaths,
    SessionStoreConfig,
    SessionStorePaths,
    SessionStorePlan,
    SessionWorktreeCheckInReceipt,
    SessionWorktreeCheckInRequest,
};
pub use transcript_feedback::{
    EntityDiscoveryQueue,
    ExplicitIngestionArgs,
    FailedToolCallMapping,
    FeedbackSignalKind,
    StructuredFeedbackSignal,
    UnmappedReason,
    discover_entities_from_signals,
    map_failed_tool_call_to_entity,
    mine_explicit_ingestion_signals,
    mine_failed_tool_call_signals,
    mine_structured_feedback_signals,
    recover_feedback_entry_from_signal,
};
