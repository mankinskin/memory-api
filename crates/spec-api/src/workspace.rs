pub const SPEC_INDEX_DIR: &str = ".spec";
pub const SPEC_ENTITY_DIR: &str = "specs";

pub fn workspace_recovery_hint(active_index_root: &std::path::Path) -> String {
    memory_kernel::workspace::workspace_recovery_hint_for_store(
        active_index_root,
        SPEC_INDEX_DIR,
        SPEC_ENTITY_DIR,
        "spec",
    )
}
