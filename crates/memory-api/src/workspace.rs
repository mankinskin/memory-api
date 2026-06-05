//! Local index-root discovery helpers shared across the CLI tools.
//!
//! Tool callers handle explicit overrides (`--index-root`, env vars) first.
//! This module resolves repo-local roots by walking upward from the current
//! directory and falling back to a sibling hidden folder in the current repo.

use std::path::{
    Path,
    PathBuf,
};

use crate::model::filesystem::ScanRoot;

pub const TICKET_INDEX_DIR: &str = ".ticket";

pub fn working_dir() -> Option<PathBuf> {
    resolve_working_dir(
        std::env::current_dir().ok().as_deref(),
        std::env::var_os("PWD").as_deref().map(Path::new),
    )
}

pub fn find_local_root(dir_name: &str) -> Option<PathBuf> {
    let cwd = working_dir()?;
    find_local_root_from(&cwd, dir_name)
}

pub fn find_local_root_from(
    start: &Path,
    dir_name: &str,
) -> Option<PathBuf> {
    let mut dir = start_dir(start);
    loop {
        let candidate = dir.join(dir_name);
        if candidate.is_dir() {
            return Some(candidate);
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return None,
        }
    }
}

pub fn resolve_local_root(dir_name: &str) -> PathBuf {
    working_dir()
        .map(|cwd| resolve_local_root_from(&cwd, dir_name))
        .unwrap_or_else(|| PathBuf::from(dir_name))
}

pub fn resolve_local_root_from(
    start: &Path,
    dir_name: &str,
) -> PathBuf {
    find_local_root_from(start, dir_name)
        .unwrap_or_else(|| start_dir(start).join(dir_name))
}

pub fn resolve_store_root_from(
    start: &Path,
    dir_name: &str,
) -> PathBuf {
    let normalized = normalize_working_dir_path(start);
    if normalized.file_name().and_then(|name| name.to_str()) == Some(dir_name) {
        return normalized;
    }

    let dir = start_dir(&normalized);

    if dir.file_name().and_then(|name| name.to_str()) == Some(dir_name) {
        return dir.to_path_buf();
    }

    find_local_root_from(dir, dir_name).unwrap_or_else(|| dir.to_path_buf())
}

pub fn resolve_requested_store_root(
    explicit_store_root: Option<&Path>,
    explicit_workspace_root: Option<&Path>,
    env_store_root: Option<&Path>,
    dir_name: &str,
) -> PathBuf {
    let cwd = working_dir();
    resolve_requested_store_root_from(
        explicit_store_root,
        explicit_workspace_root,
        env_store_root,
        cwd.as_deref(),
        dir_name,
    )
}

pub fn resolve_requested_store_root_from(
    explicit_store_root: Option<&Path>,
    explicit_workspace_root: Option<&Path>,
    env_store_root: Option<&Path>,
    cwd: Option<&Path>,
    dir_name: &str,
) -> PathBuf {
    if let Some(path) = explicit_store_root {
        return resolve_store_root_from(path, dir_name);
    }

    if let Some(path) = explicit_workspace_root {
        return resolve_store_root_from(path, dir_name);
    }

    if let Some(path) = env_store_root {
        return resolve_store_root_from(path, dir_name);
    }

    if let Some(cwd) = cwd {
        return resolve_local_root_from(cwd, dir_name);
    }

    PathBuf::from(dir_name)
}

pub fn resolve_workspace_root_from_store_root(
    store_root: &Path,
    dir_name: &str,
) -> PathBuf {
    let normalized = normalize_working_dir_path(store_root);
    if normalized.file_name().and_then(|name| name.to_str()) == Some(dir_name) {
        return normalized
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or(normalized);
    }

    normalized
}

pub fn find_descendant_store_roots_from(
    start: &Path,
    dir_name: &str,
) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    collect_descendant_store_roots(
        &normalize_working_dir_path(start_dir(start)),
        dir_name,
        &mut roots,
    );
    roots.sort();
    roots.dedup();
    roots
}

pub fn discover_workspace_scan_roots(
    workspace_root: &Path,
    store_dir: &str,
    entity_dir: &str,
) -> Vec<ScanRoot> {
    let workspace_root = normalize_working_dir_path(start_dir(workspace_root));
    let mut store_roots = find_descendant_store_roots_from(&workspace_root, store_dir);

    for ancestor in workspace_root.ancestors().skip(1) {
        let candidate = ancestor.join(store_dir);
        if candidate.is_dir() {
            store_roots.push(candidate);
        }
    }

    store_roots.sort();
    store_roots.dedup();

    store_roots
        .into_iter()
        .map(|store_root| {
            let owning_workspace =
                resolve_workspace_root_from_store_root(&store_root, store_dir);
            let label = owning_workspace
                .strip_prefix(&workspace_root)
                .ok()
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .unwrap_or_else(|| {
                    owning_workspace
                        .file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| format!("ancestor:{name}"))
                        .unwrap_or_else(|| "ancestor".to_string())
                });

            ScanRoot {
                path: store_root.join(entity_dir),
                label,
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSource {
    Discovered(PathBuf),
    Default(PathBuf),
}

impl WorkspaceSource {
    pub fn description(&self) -> String {
        match self {
            Self::Discovered(path) =>
                format!("discovered local .ticket ({})", path.display()),
            Self::Default(path) =>
                format!("default local .ticket ({})", path.display()),
        }
    }
}

pub fn resolve_workspace() -> (PathBuf, WorkspaceSource) {
    working_dir()
        .map(|cwd| resolve_workspace_from(&cwd))
        .unwrap_or_else(|| {
            let path = PathBuf::from(TICKET_INDEX_DIR);
            (path.clone(), WorkspaceSource::Default(path))
        })
}

pub fn resolve_workspace_from(start: &Path) -> (PathBuf, WorkspaceSource) {
    if let Some(path) = find_local_root_from(start, TICKET_INDEX_DIR) {
        return (path.clone(), WorkspaceSource::Discovered(path));
    }

    let path = start_dir(start).join(TICKET_INDEX_DIR);
    (path.clone(), WorkspaceSource::Default(path))
}

fn start_dir(start: &Path) -> &Path {
    if start.is_dir() {
        start
    } else {
        start.parent().unwrap_or(start)
    }
}

fn resolve_working_dir(
    cwd: Option<&Path>,
    pwd: Option<&Path>,
) -> Option<PathBuf> {
    cwd.or(pwd).map(normalize_working_dir_path)
}

fn normalize_working_dir_path(path: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        let raw = path.to_string_lossy();
        if let Some(normalized) = normalize_git_bash_pwd(&raw) {
            return PathBuf::from(normalized);
        }
        return PathBuf::from(raw.replace('\\', "/"));
    }

    #[cfg(not(windows))]
    {
        path.to_path_buf()
    }
}

fn collect_descendant_store_roots(
    dir: &Path,
    dir_name: &str,
    roots: &mut Vec<PathBuf>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };

        if name == dir_name {
            roots.push(normalize_working_dir_path(&path));
            continue;
        }

        if should_skip_descendant_dir(name) {
            continue;
        }

        collect_descendant_store_roots(&path, dir_name, roots);
    }
}

fn should_skip_descendant_dir(name: &str) -> bool {
    matches!(name, ".git" | ".hg" | ".svn" | "target" | "node_modules" | "release" | "tmp")
}

#[cfg(windows)]
fn normalize_git_bash_pwd(raw: &str) -> Option<String> {
    let bytes = raw.as_bytes();
    if bytes.len() < 3 || bytes[0] != b'/' || bytes[2] != b'/' {
        return None;
    }

    let drive = bytes[1] as char;
    if !drive.is_ascii_alphabetic() {
        return None;
    }

    let mut normalized = String::with_capacity(raw.len());
    normalized.push(drive.to_ascii_uppercase());
    normalized.push(':');
    normalized.push('/');
    normalized.push_str(&raw[3..]);
    Some(normalized)
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn find_local_root_from_discovers_parent_workspace() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let nested = repo.join("a").join("b");
        std::fs::create_dir_all(repo.join(".ticket")).unwrap();
        std::fs::create_dir_all(&nested).unwrap();

        let found = find_local_root_from(&nested, ".ticket").unwrap();

        assert_eq!(found, repo.join(".ticket"));
    }

    #[test]
    fn resolve_local_root_from_defaults_to_start_directory() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let nested = repo.join("src");
        std::fs::create_dir_all(&nested).unwrap();

        let resolved = resolve_local_root_from(&nested, ".spec");

        assert_eq!(resolved, nested.join(".spec"));
    }

    #[test]
    fn resolve_store_root_from_uses_existing_hidden_store() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let nested = repo.join("src");
        std::fs::create_dir_all(repo.join(".ticket")).unwrap();
        std::fs::create_dir_all(&nested).unwrap();

        let resolved = resolve_store_root_from(&nested, ".ticket");

        assert_eq!(resolved, repo.join(".ticket"));
    }

    #[test]
    fn resolve_store_root_from_preserves_direct_store_root() {
        let dir = tempdir().unwrap();
        let store = dir.path().join(".ticket");
        std::fs::create_dir_all(&store).unwrap();

        let resolved = resolve_store_root_from(&store, ".ticket");

        assert_eq!(resolved, store);
    }

    #[test]
    fn resolve_store_root_from_preserves_non_workspace_directory() {
        let dir = tempdir().unwrap();
        let scratch = dir.path().join("scratch");
        std::fs::create_dir_all(&scratch).unwrap();

        let resolved = resolve_store_root_from(&scratch, ".ticket");

        assert_eq!(resolved, scratch);
    }

    #[test]
    fn resolve_requested_store_root_from_normalizes_explicit_workspace_root() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("child");
        std::fs::create_dir_all(repo.join(".spec")).unwrap();
        std::fs::create_dir_all(child.join(".spec")).unwrap();

        let resolved = resolve_requested_store_root_from(
            None,
            Some(&child),
            None,
            Some(&repo),
            ".spec",
        );

        assert_eq!(resolved, child.join(".spec"));
    }

    #[test]
    fn resolve_requested_store_root_from_prefers_explicit_store_root() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("child");
        std::fs::create_dir_all(repo.join(".ticket")).unwrap();
        std::fs::create_dir_all(child.join(".ticket")).unwrap();

        let resolved = resolve_requested_store_root_from(
            Some(&repo.join(".ticket")),
            Some(&child),
            Some(&child.join(".ticket")),
            Some(&child),
            ".ticket",
        );

        assert_eq!(resolved, repo.join(".ticket"));
    }

    #[test]
    fn resolve_requested_store_root_from_falls_back_to_local_discovery() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let nested = repo.join("tools").join("cli");
        std::fs::create_dir_all(repo.join(".ticket")).unwrap();
        std::fs::create_dir_all(&nested).unwrap();

        let resolved = resolve_requested_store_root_from(
            None,
            None,
            None,
            Some(&nested),
            ".ticket",
        );

        assert_eq!(resolved, repo.join(".ticket"));
    }

    #[test]
    fn resolve_workspace_root_from_store_root_uses_parent_of_hidden_store() {
        let dir = tempdir().unwrap();
        let store = dir.path().join("repo").join(".spec");
        std::fs::create_dir_all(&store).unwrap();

        let resolved =
            resolve_workspace_root_from_store_root(&store, ".spec");

        assert_eq!(resolved, store.parent().unwrap());
    }

    #[test]
    fn resolve_workspace_root_from_store_root_preserves_direct_non_store_path() {
        let dir = tempdir().unwrap();
        let scratch = dir.path().join("scratch-store");
        std::fs::create_dir_all(&scratch).unwrap();

        let resolved =
            resolve_workspace_root_from_store_root(&scratch, ".spec");

        assert_eq!(resolved, scratch);
    }

    #[test]
    fn find_descendant_store_roots_from_discovers_nested_hidden_stores() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("memory-api");
        let nested = child.join("tools").join("cli");
        std::fs::create_dir_all(repo.join(".spec")).unwrap();
        std::fs::create_dir_all(child.join(".spec")).unwrap();
        std::fs::create_dir_all(&nested).unwrap();

        let roots = find_descendant_store_roots_from(&repo, ".spec");

        assert_eq!(roots, vec![repo.join(".spec"), child.join(".spec")]);
    }

    #[test]
    fn find_descendant_store_roots_from_skips_known_non_workspace_dirs() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("memory-api");
        std::fs::create_dir_all(repo.join(".spec")).unwrap();
        std::fs::create_dir_all(child.join(".spec")).unwrap();
        std::fs::create_dir_all(repo.join("target").join("build").join(".spec"))
            .unwrap();
        std::fs::create_dir_all(
            repo.join("node_modules").join("pkg").join(".spec"),
        )
        .unwrap();
        std::fs::create_dir_all(repo.join("release").join("notes").join(".spec"))
            .unwrap();
        std::fs::create_dir_all(repo.join("tmp").join("scratch").join(".spec"))
            .unwrap();
        std::fs::create_dir_all(repo.join(".git").join("worktree").join(".spec"))
            .unwrap();

        let roots = find_descendant_store_roots_from(&repo, ".spec");

        assert_eq!(roots, vec![repo.join(".spec"), child.join(".spec")]);
    }

    #[test]
    fn discover_workspace_scan_roots_maps_store_roots_to_entity_roots() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("memory-api");
        std::fs::create_dir_all(repo.join(".rule")).unwrap();
        std::fs::create_dir_all(child.join(".rule")).unwrap();

        let roots = discover_workspace_scan_roots(&repo, ".rule", "rules");

        assert_eq!(
            roots,
            vec![
                ScanRoot {
                    path: repo.join(".rule").join("rules"),
                    label: ".".to_string(),
                },
                ScanRoot {
                    path: child.join(".rule").join("rules"),
                    label: "memory-api".to_string(),
                },
            ]
        );
    }

    #[test]
    fn discover_workspace_scan_roots_includes_ancestor_store_roots() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        let child = repo.join("memory-viewers").join("memory-api");
        std::fs::create_dir_all(repo.join(".rule")).unwrap();
        std::fs::create_dir_all(child.join(".rule")).unwrap();

        let roots = discover_workspace_scan_roots(&child, ".rule", "rules");

        assert_eq!(
            roots,
            vec![
                ScanRoot {
                    path: repo.join(".rule").join("rules"),
                    label: "ancestor:repo".to_string(),
                },
                ScanRoot {
                    path: child.join(".rule").join("rules"),
                    label: ".".to_string(),
                },
            ]
        );
    }

    #[test]
    fn resolve_workspace_from_reports_default_local_ticket() {
        let dir = tempdir().unwrap();
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();

        let (path, source) = resolve_workspace_from(&repo);

        assert_eq!(path, repo.join(".ticket"));
        assert_eq!(source, WorkspaceSource::Default(repo.join(".ticket")));
    }

    #[test]
    fn resolve_working_dir_prefers_cwd() {
        let cwd = Path::new("repo/current");
        let pwd = Path::new("repo/pwd");

        let resolved = resolve_working_dir(Some(cwd), Some(pwd));

        assert_eq!(resolved, Some(normalize_working_dir_path(cwd)));
    }

    #[test]
    fn resolve_working_dir_falls_back_to_pwd() {
        let pwd = Path::new("repo/pwd");

        let resolved = resolve_working_dir(None, Some(pwd));

        assert_eq!(resolved, Some(normalize_working_dir_path(pwd)));
    }

    #[cfg(windows)]
    #[test]
    fn normalize_working_dir_path_converts_backslashes() {
        let normalized =
            normalize_working_dir_path(Path::new(r"C:\repo\memory-api"));

        assert_eq!(normalized, PathBuf::from("C:/repo/memory-api"));
    }

    #[cfg(windows)]
    #[test]
    fn normalize_working_dir_path_converts_git_bash_pwd() {
        let normalized =
            normalize_working_dir_path(Path::new("/c/repo/memory-api"));

        assert_eq!(normalized, PathBuf::from("C:/repo/memory-api"));
    }
}
