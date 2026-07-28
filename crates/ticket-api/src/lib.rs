pub mod contracts;
pub mod error;
pub mod execution;
pub mod health;
pub mod model;
pub mod missing_rule;
pub mod output;
pub mod query_helpers;
pub mod storage;
pub mod store_index;
pub mod watcher;
pub mod workflow;
pub mod workspace;

/// Re-export of the workspace-policy model and its load/save helpers.
pub mod workspace_policy {
    pub use memory_api::workspace_policy::*;
}

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
    DESCRIPTION_HISTORY_KEY,
    DescriptionUpdateMode,
    ReconcileAction,
};

pub use store_index::{
    TICKET_INDEX_AGENT_HOOK_PATH,
    TicketCatalogArtifacts,
    TicketCatalogSource,
    generate_ticket_catalog,
};

pub use missing_rule::{
    SituationQuery,
    handle_missing_rule_match,
};
