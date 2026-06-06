use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SessionError {
    #[error("invalid hook input: {0}")]
    InvalidHookInput(String),

    #[error("session capture is missing a session id")]
    MissingSessionId,

    #[error("session capture did not include any turns")]
    EmptyTurns,

    #[error("session store root cannot be empty")]
    EmptyStoreRoot,

    #[error("session id contains invalid path characters: {0}")]
    InvalidSessionId(String),

    #[error("workspace slug contains invalid path characters: {0}")]
    InvalidWorkspaceSlug(String),

    #[error("session owner id cannot be empty")]
    MissingOwnerId,

    #[error("session ticket id cannot be empty")]
    MissingTicketId,

    #[error("worktree path cannot be empty")]
    EmptyWorktreePath,

    #[error("worktree branch cannot be empty")]
    EmptyWorktreeBranch,

    #[error("store path has no parent directory: {0}")]
    InvalidStorePath(PathBuf),

    #[error("session {session_id} does not have a persisted worktree assignment")]
    MissingWorktreeAssignment {
        session_id: String,
    },

    #[error("session {session_id} ownership mismatch for worktree check-in")]
    SessionOwnershipMismatch {
        session_id: String,
    },

    #[error("worktree path {path} is already owned by active session {session_id}")]
    WorktreeConflict {
        path: PathBuf,
        session_id: String,
    },

    #[error("cross-session worktree reuse requires an explicit adopt flow: predecessor {session_id} already owns {path}")]
    CrossSessionReuseRequiresAdopt {
        session_id: String,
        path: PathBuf,
    },

    #[error("failed to serialize session data for {path}: {source}")]
    Serialize {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("failed to deserialize session data from {path}: {source}")]
    Deserialize {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    #[error("session data was not found at {path}")]
    NotFound {
        path: PathBuf,
    },

    #[error(
        "incoming transcript would rewrite existing turns for session {session_id} ({existing_turns} existing, {incoming_turns} incoming)"
    )]
    TranscriptConflict {
        session_id: String,
        existing_turns: usize,
        incoming_turns: usize,
    },

    #[error("filesystem operation failed for {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}