pub mod contracts;
pub mod error;
pub mod execution;
pub mod health;
pub mod model;
pub mod output;
pub mod storage;
pub mod watcher;
pub mod workflow;
pub mod workspace;

// Re-export board types at the crate root for convenient access.
pub use storage::{
    BoardCleanPreview,
    BoardCleanResult,
    BoardConfig,
    BoardEntry,
    BoardEntryStatus,
    BoardError,
    BoardReconcileResult,
    BoardSnapshot,
    ReconcileAction,
};
