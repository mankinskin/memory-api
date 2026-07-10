pub use memory_api::workspace::*;

pub const DEFAULT_WORKSPACE_NAME: &str = "default";
pub const TICKET_ENTITY_DIR: &str = "tickets";

pub fn workspace_recovery_hint(active_index_root: &std::path::Path) -> String {
    memory_api::workspace::workspace_recovery_hint_for_store(
        active_index_root,
        TICKET_INDEX_DIR,
        TICKET_ENTITY_DIR,
        "ticket",
    )
}
