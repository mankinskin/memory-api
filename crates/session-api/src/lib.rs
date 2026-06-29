pub mod error;
pub mod hook;
pub mod model;
pub mod move_domain;
pub mod peek;
pub mod store;

pub use error::SessionError;
pub use hook::{
    copilot_payload_from_transcript_path,
    copilot_payload_from_transcript_reader,
    CopilotHookMessage,
    CopilotHookPayload,
    SessionCaptureRequest,
};
pub use model::{
    SessionLinks,
    SessionMetadata,
    SessionRecord,
    SessionRole,
    SessionTurn,
    SessionWorktreeAllocationMode,
    SessionWorktreeAssignment,
    SessionWorktreeStatus,
};
pub use peek::{
    peek_skeleton,
    peek_turn_range,
    SessionSkeleton,
    SessionSkeletonEntry,
    SessionTurnRange,
    DEFAULT_SKELETON_PREVIEW_CHARS,
};
pub use store::{
    PersistedSessionManifest,
    PersistedSessionTranscript,
    SessionQuery,
    SessionStoreConfig,
    SessionStorePaths,
    SessionStorePlan,
    SessionWorktreeCheckInReceipt,
    SessionWorktreeCheckInRequest,
};
