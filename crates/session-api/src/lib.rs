pub mod audit;
pub mod error;
pub mod hook;
pub mod model;
pub mod move_domain;
pub mod peek;
pub mod store;

pub use audit::{
    SessionAuditFinding,
    SessionAuditMetrics,
    SessionAuditReport,
    SessionAuditSelector,
    SessionAuditSeverity,
    SessionAuditToolCount,
};
pub use error::SessionError;
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
    SESSION_SCHEMA_VERSION,
    SessionLinks,
    SessionMetadata,
    SessionRecord,
    SessionRole,
    SessionTurn,
    SessionTurnEventMeta,
    SessionWorktreeAllocationMode,
    SessionWorktreeAssignment,
    SessionWorktreeStatus,
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
    PersistedSessionEvents,
    PersistedSessionManifest,
    PersistedSessionTranscript,
    SessionQuery,
    SessionStoreConfig,
    SessionStorePaths,
    SessionStorePlan,
    SessionWorktreeCheckInReceipt,
    SessionWorktreeCheckInRequest,
};
