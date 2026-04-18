pub mod error;
pub mod model;
pub mod storage;
pub mod workspace;

// Re-export board types at the crate root for convenient access.
pub use storage::{
    BoardCleanPreview, BoardCleanResult, BoardConfig, BoardEntry, BoardEntryStatus, BoardError,
    BoardReconcileResult, BoardSnapshot, ReconcileAction,
};
