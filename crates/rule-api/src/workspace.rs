use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use memory_api::model::filesystem::ScanRoot;

pub fn workspace_root_for_index_root(index_root: &Path) -> Option<PathBuf> {
    if index_root.file_name() == Some(OsStr::new(".rule")) {
        index_root.parent().map(Path::to_path_buf)
    } else {
        None
    }
}

pub fn discover_workspace_scan_roots(workspace_root: &Path) -> Vec<ScanRoot> {
    let workspace_root = stable_path(workspace_root);
    let mut scan_roots = Vec::new();
    let mut seen = BTreeSet::new();

    push_workspace_rules_root(
        &workspace_root,
        &workspace_root,
        &mut seen,
        &mut scan_roots,
    );
    discover_descendant_workspace_roots(
        &workspace_root,
        &workspace_root,
        &mut seen,
        &mut scan_roots,
    );

    scan_roots
}

fn discover_descendant_workspace_roots(
    workspace_root: &Path,
    dir: &Path,
    seen: &mut BTreeSet<PathBuf>,
    scan_roots: &mut Vec<ScanRoot>,
) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let path = entry.path();
        let name = entry.file_name();
        if should_skip_dir(&name) {
            continue;
        }

        push_workspace_rules_root(workspace_root, &path, seen, scan_roots);
        discover_descendant_workspace_roots(
            workspace_root,
            &path,
            seen,
            scan_roots,
        );
    }
}

fn push_workspace_rules_root(
    workspace_root: &Path,
    owning_workspace: &Path,
    seen: &mut BTreeSet<PathBuf>,
    scan_roots: &mut Vec<ScanRoot>,
) {
    let rules_root = owning_workspace.join(".rule").join("rules");
    if !rules_root.is_dir() {
        return;
    }

    let rules_root = stable_path(&rules_root);
    if !seen.insert(rules_root.clone()) {
        return;
    }

    let label = owning_workspace
        .strip_prefix(workspace_root)
        .ok()
        .filter(|path| !path.as_os_str().is_empty())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| ".".to_string());

    scan_roots.push(ScanRoot {
        path: rules_root,
        label,
    });
}

fn should_skip_dir(name: &OsStr) -> bool {
    matches!(
        name.to_str(),
        Some(".git" | ".hg" | ".svn" | ".rule" | "target" | "node_modules")
    )
}

fn stable_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}
