pub mod board;
pub mod entity_fs;
pub mod entity_store;
pub mod index;
pub mod indexed;
pub mod local_root;
pub mod move_kernel;
pub mod schema;
pub mod search;
pub mod watcher;

pub use board::{
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
pub use entity_store::EntityStore;
pub use local_root::{
    ensure_gitignore_entries,
    ensure_sqlite_index_root,
};
pub use move_kernel::{
    GitWorktreeTopology,
    MoveBlocker,
    MoveBoardState,
    MoveDomain,
    MoveError,
    MoveExecutionPhase,
    MoveJournal,
    MoveLeaseBlock,
    MoveManualFollowup,
    MoveOutcome,
    MovePathRewrite,
    MovePlan,
    MoveReferenceDirection,
    MoveReferenceVisibility,
    MoveReferences,
    MoveResult,
};
pub use watcher::{
    WatchHandle,
    run_watch_loop,
};
