pub mod error;
pub mod hook;
pub mod model;
pub mod store;

pub use error::SessionError;
pub use hook::{
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
};
pub use store::{
    PersistedSessionManifest,
    PersistedSessionTranscript,
    SessionQuery,
    SessionStoreConfig,
    SessionStorePaths,
    SessionStorePlan,
};