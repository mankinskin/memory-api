pub mod error;
pub mod hook;
pub mod model;
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
pub use store::{
    PersistedSessionManifest,
    PersistedSessionTranscript,
    SessionQuery,
    SessionWorktreeCheckInReceipt,
    SessionWorktreeCheckInRequest,
    SessionStoreConfig,
    SessionStorePaths,
    SessionStorePlan,
};