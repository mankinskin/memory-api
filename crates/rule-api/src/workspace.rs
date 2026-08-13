use std::{
    ffi::OsStr,
    path::{
        Path,
        PathBuf,
    },
};

use memory_kernel::model::filesystem::ScanRoot;

pub const RULE_INDEX_DIR: &str = ".rule";
pub const RULE_ENTITY_DIR: &str = "rules";

pub fn workspace_root_for_index_root(index_root: &Path) -> Option<PathBuf> {
    if index_root.file_name() == Some(OsStr::new(RULE_INDEX_DIR)) {
        Some(
            memory_kernel::workspace::resolve_workspace_root_from_store_root(
                index_root,
                RULE_INDEX_DIR,
            ),
        )
    } else {
        None
    }
}

pub fn discover_workspace_scan_roots(workspace_root: &Path) -> Vec<ScanRoot> {
    memory_kernel::workspace::discover_workspace_scan_roots(
        workspace_root,
        RULE_INDEX_DIR,
        RULE_ENTITY_DIR,
    )
}

pub fn workspace_recovery_hint(active_index_root: &Path) -> String {
    memory_kernel::workspace::workspace_recovery_hint_for_store(
        active_index_root,
        RULE_INDEX_DIR,
        RULE_ENTITY_DIR,
        "rule",
    )
}
