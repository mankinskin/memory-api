//! Local index-root discovery helpers shared across the CLI tools.
//!
//! Tool callers handle explicit overrides (`--index-root`, env vars) first.
//! This module resolves repo-local roots by walking upward from the current
//! directory and falling back to a sibling hidden folder in the current repo.

use std::path::{
    Path,
    PathBuf,
};

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
