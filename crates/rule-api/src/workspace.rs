use std::{
    ffi::OsStr,
    path::{
        Path,
        PathBuf,
    },
};

use memory_api::model::filesystem::ScanRoot;

pub fn workspace_root_for_index_root(index_root: &Path) -> Option<PathBuf> {
    if index_root.file_name() == Some(OsStr::new(".rule")) {
        Some(
            memory_api::workspace::resolve_workspace_root_from_store_root(
                index_root, ".rule",
            ),
        )
    } else {
        None
    }
}

pub fn discover_workspace_scan_roots(workspace_root: &Path) -> Vec<ScanRoot> {
    memory_api::workspace::discover_workspace_scan_roots(
        workspace_root,
        ".rule",
        "rules",
    )
}
