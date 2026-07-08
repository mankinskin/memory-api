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
use crate::workspace_policy::{
    WorkspacePolicy,
    load_workspace_policy,
};

pub const TICKET_INDEX_DIR: &str = ".ticket";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidWorkspaceSelector {
    value: String,
}

impl InvalidWorkspaceSelector {
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl std::fmt::Display for InvalidWorkspaceSelector {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        write!(
            f,
            "invalid workspace selector '{}': entity creation requires an explicit workspace path; do not use omitted, empty, 'default', '.', or '..'",
            self.value
        )
    }
}

impl std::error::Error for InvalidWorkspaceSelector {}

pub fn validate_explicit_workspace_selector(
    workspace: Option<&str>
) -> Result<&str, InvalidWorkspaceSelector> {
    let Some(workspace) = workspace else {
        return Err(InvalidWorkspaceSelector {
            value: "<omitted>".to_string(),
        });
    };
    let trimmed = workspace.trim();
    if matches!(trimmed, "" | "default" | "." | "..") {
        return Err(InvalidWorkspaceSelector {
            value: trimmed.to_string(),
        });
    }
    Ok(trimmed)
}

#[derive(Debug)]
pub enum WorkspacePathError {
    CanonicalizeFailed {
        input: String,
        source: std::io::Error,
    },
    InvalidWindowsPrefix {
        input: String,
        detail: String,
    },
    UnrepresentablePath {
        input: String,
        detail: String,
    },
}

impl std::fmt::Display for WorkspacePathError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::CanonicalizeFailed {
                input,
                source,
            } => write!(
                f,
                "failed to canonicalize workspace root '{input}': {source}"
            ),
            Self::InvalidWindowsPrefix {
                input,
                detail,
            } => write!(
                f,
                "invalid Windows path prefix for '{input}': {detail}"
            ),
            Self::UnrepresentablePath {
                input,
                detail,
            } => write!(
                f,
                "unrepresentable path '{input}': {detail}"
            ),
        }
    }
}

impl std::error::Error for WorkspacePathError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::CanonicalizeFailed {
                source,
                ..
            } => Some(source),
            _ => None,
        }
    }
}

pub fn working_dir() -> Option<PathBuf> {
    resolve_working_dir(
        std::env::current_dir().ok().as_deref(),
        std::env::var_os("PWD").as_deref().map(Path::new),
    )
}

pub fn normalize_path_for_display(path: &Path) -> String {
    normalize_path_for_display_impl(path).unwrap_or_else(|_| {
        let fallback = normalize_path_for_workspace_string(&path.to_string_lossy());
        normalize_drive_letter_for_display(&fallback)
    })
}

pub fn normalize_path_for_display_strict(
    path: &Path,
) -> Result<String, WorkspacePathError> {
    normalize_path_for_display_impl(path)
}

/// Canonicalize a path for use as a workspace/store root, stripping the Windows
/// `\\?\` verbatim prefix that [`std::fs::canonicalize`] emits.
///
/// The verbatim prefix must never reach stored move journals, rewritten path
/// references, or post-move validation comparisons: it breaks string matching
/// against clean paths and corrupts tracked files. Falls back to the input path
/// when canonicalization fails (for example when the directory does not exist
/// yet).
pub fn canonicalize_workspace_root(path: &Path) -> PathBuf {
    canonicalize_workspace_root_lossy(path)
}

pub fn canonicalize_workspace_root_strict(
    path: &Path,
) -> Result<PathBuf, WorkspacePathError> {
    let canonical = std::fs::canonicalize(path).map_err(|source| {
        WorkspacePathError::CanonicalizeFailed {
            input: path.to_string_lossy().to_string(),
            source,
        }
    })?;
    Ok(strip_verbatim_prefix(&canonical))
}

pub fn canonicalize_workspace_root_lossy(path: &Path) -> PathBuf {
    canonicalize_workspace_root_strict(path)
        .unwrap_or_else(|_| strip_verbatim_prefix(path))
}

/// Remove the Windows `\\?\` (and slash-normalized `//?/`) verbatim prefix from
/// a path and normalize separators, without touching the filesystem.
pub fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let raw = path.to_string_lossy();
    let normalized = normalize_path_for_workspace_string(&raw);
    PathBuf::from(normalized)
}

/// Enforce consistent path separators by converting all backslashes to forward slashes,
/// and strip any Windows verbatim (extended-path) prefixes.
pub fn normalize_slashes(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    let raw = raw.strip_prefix("//?/").unwrap_or(&raw);
    raw.strip_prefix(r"\\?\").unwrap_or(raw).to_string()
}

fn normalize_path_for_display_impl(
    path: &Path,
) -> Result<String, WorkspacePathError> {
    let raw = path
        .to_str()
        .map(str::to_string)
        .ok_or_else(|| WorkspacePathError::UnrepresentablePath {
            input: path.to_string_lossy().to_string(),
            detail: "path is not valid UTF-8".to_string(),
        })?;

    let workspace = normalize_path_for_workspace_string_strict(&raw)?;
    Ok(normalize_drive_letter_for_display(&workspace))
}

fn normalize_drive_letter_for_display(value: &str) -> String {
    if let Some((drive, remainder)) = split_windows_drive_prefix(value) {
        let mut out = String::with_capacity(value.len() + 1);
        out.push('/');
        out.push(drive.to_ascii_lowercase());
        if !remainder.is_empty() {
            out.push('/');
            out.push_str(remainder.trim_start_matches('/'));
        }
        return out;
    }

    value.to_string()
}

fn normalize_path_for_workspace_string(raw: &str) -> String {
    normalize_path_for_workspace_string_impl(raw, false)
        .unwrap_or_else(|_| collapse_slashes_preserving_root(&raw.replace('\\', "/")))
}

fn normalize_path_for_workspace_string_strict(
    raw: &str,
) -> Result<String, WorkspacePathError> {
    normalize_path_for_workspace_string_impl(raw, true)
}

fn normalize_path_for_workspace_string_impl(
    raw: &str,
    strict: bool,
) -> Result<String, WorkspacePathError> {
    let input = raw.to_string();

    if let Some(rest) = raw
        .strip_prefix(r"\\?\UNC\")
        .or_else(|| raw.strip_prefix("//?/UNC/"))
    {
        let rest_normalized = rest.replace('\\', "/");
        if strict {
            validate_unc_remainder(&rest_normalized, &input)?;
        }
        return Ok(collapse_slashes_preserving_root(&format!(
            "//{}",
            rest_normalized
        )));
    }

    let without_verbatim = raw
        .strip_prefix(r"\\?\")
        .or_else(|| raw.strip_prefix("//?/"))
        .unwrap_or(raw);
    let normalized = collapse_slashes_preserving_root(&without_verbatim.replace('\\', "/"));

    if strict && normalized.starts_with("//") {
        let remainder = normalized.trim_start_matches('/');
        validate_unc_remainder(remainder, &input)?;
    }

    Ok(normalized)
}

fn validate_unc_remainder(
    remainder: &str,
    input: &str,
) -> Result<(), WorkspacePathError> {
    let mut parts = remainder.split('/').filter(|part| !part.is_empty());
    let server = parts.next();
    let share = parts.next();
    if server.is_none() || share.is_none() {
        return Err(WorkspacePathError::InvalidWindowsPrefix {
            input: input.to_string(),
            detail: "UNC path must include both server and share segments".to_string(),
        });
    }
    Ok(())
}

fn split_windows_drive_prefix(value: &str) -> Option<(char, &str)> {
    let bytes = value.as_bytes();
    if bytes.len() < 2 {
        return None;
    }
    let drive = bytes[0] as char;
    if !drive.is_ascii_alphabetic() || bytes[1] != b':' {
        return None;
    }
    let remainder = &value[2..];
    Some((drive, remainder))
}

fn collapse_slashes_preserving_root(value: &str) -> String {
    let (prefix, remainder) = if let Some(rest) = value.strip_prefix("//") {
        ("//", rest)
    } else if let Some(rest) = value.strip_prefix('/') {
        ("/", rest)
    } else {
        ("", value)
    };

    let mut out = String::with_capacity(value.len());
    out.push_str(prefix);

    let mut prev_was_slash = false;
    for ch in remainder.chars() {
        if ch == '/' {
            if !prev_was_slash {
                out.push(ch);
            }
            prev_was_slash = true;
        } else {
            out.push(ch);
            prev_was_slash = false;
        }
    }

    out
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

/// Resolve a session-style store root relative to the tool execution directory.
///
/// Unlike [`resolve_local_root_from`], which only walks upward, this helper also
/// prefers an existing hidden store nested *below* the execution root. This is
/// required for stores that live inside a submodule (for example the
/// `memory-api` workspace's `.memory-api` directory): running the tool from the
/// repository root must reuse that nested store instead of creating a duplicate
/// hidden directory at the root.
///
/// Resolution order:
/// 1. An existing store discovered by walking upward from `cwd`.
/// 2. An existing store nested under `cwd` (shallowest first).
/// 3. A hidden store at the execution root (`cwd/<dir_name>`).
pub fn resolve_session_store_root_from(
    cwd: Option<&Path>,
    dir_name: &str,
) -> PathBuf {
    let Some(cwd) = cwd else {
        return PathBuf::from(dir_name);
    };

    if let Some(existing) = find_local_root_from(cwd, dir_name) {
        return existing;
    }

    if let Some(nested) = find_descendant_store_roots_from(cwd, dir_name)
        .into_iter()
        .next()
    {
        return nested;
    }

    resolve_local_root_from(cwd, dir_name)
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
    let normalized_root = normalize_working_dir_path(start_dir(workspace_root));
    let policy = load_workspace_policy(&normalized_root);
    discover_workspace_scan_roots_with_policy(
        workspace_root,
        store_dir,
        entity_dir,
        &policy,
    )
}

/// Discover canonical hidden store roots for a workspace using active
/// workspace-policy rules.
pub fn discover_workspace_store_roots(
    workspace_root: &Path,
    store_dir: &str,
    entity_dir: &str,
) -> Vec<PathBuf> {
    let mut roots = discover_workspace_scan_roots(
        workspace_root,
        store_dir,
        entity_dir,
    )
    .into_iter()
    .map(|root| resolve_store_root_from(&root.path, store_dir))
    .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    roots
}

/// Render a consistent workspace recovery hint for entity lookups.
///
/// The discovered stores are computed via the same policy-aware scan-root
/// resolution path used by active entity indexing.
pub fn workspace_recovery_hint_for_store(
    active_index_root: &Path,
    store_dir: &str,
    entity_dir: &str,
    store_label: &str,
) -> String {
    let active_store_root = resolve_store_root_from(active_index_root, store_dir);
    let workspace_root =
        resolve_workspace_root_from_store_root(&active_store_root, store_dir);
    let discovered = discover_workspace_store_roots(
        &workspace_root,
        store_dir,
        entity_dir,
    )
    .into_iter()
    .map(|path| normalize_path_for_display(&path))
    .collect::<Vec<_>>();

    if discovered.is_empty() {
        return format!(
            "active index root: {}. Retry with --workspace-root <workspace-path> or --index-root <path-to-{store_dir}>",
            normalize_path_for_display(&active_store_root)
        );
    }

    format!(
        "active index root: {}\n\n\tRetry with --workspace-root <workspace-path> or --index-root <path-to-{store_dir}>.\n\nDiscovered {store_label} stores:\n- {}",
        normalize_path_for_display(&active_store_root),
        discovered.join(",\n- ")
    )
}

/// Policy-aware variant of [`discover_workspace_scan_roots`].
///
/// The supplied [`WorkspacePolicy`] governs which store roots are collected:
/// descendant discovery is gated on `include_descendants`, ancestor stores are
/// gated on `include_ancestors` and suppressed entirely when
/// `deny_external_paths` is set, and every candidate is filtered through the
/// policy's ignore globs, ignore markers, and include overrides (overrides
/// win). The active workspace root store is always included when present.
pub fn discover_workspace_scan_roots_with_policy(
    workspace_root: &Path,
    store_dir: &str,
    entity_dir: &str,
    policy: &WorkspacePolicy,
) -> Vec<ScanRoot> {
    let workspace_root = normalize_working_dir_path(start_dir(workspace_root));
    let mut store_roots = Vec::new();

    // The active workspace root store is always included when present.
    let root_store = workspace_root.join(store_dir);
    if root_store.is_dir() {
        store_roots.push(normalize_working_dir_path(&root_store));
    }

    if policy.include_descendants {
        for store_root in find_descendant_store_roots_from(&workspace_root, store_dir)
        {
            let owning_workspace =
                resolve_workspace_root_from_store_root(&store_root, store_dir);
            if owning_workspace == workspace_root {
                continue;
            }
            if policy_allows(policy, &workspace_root, &owning_workspace) {
                store_roots.push(store_root);
            }
        }
    }

    // Ancestor stores live outside the workspace subtree, so they are only
    // eligible when external paths are permitted and ancestors are requested.
    if policy.include_ancestors && !policy.deny_external_paths {
        for ancestor in workspace_root.ancestors().skip(1) {
            let candidate = ancestor.join(store_dir);
            if !candidate.is_dir() {
                continue;
            }
            let owning_workspace =
                resolve_workspace_root_from_store_root(&candidate, store_dir);
            if policy_allows(policy, &workspace_root, &owning_workspace) {
                store_roots.push(normalize_working_dir_path(&candidate));
            }
        }
    }

    store_roots.sort();
    store_roots.dedup();

    store_roots
        .into_iter()
        .map(|store_root| {
            let owning_workspace =
                resolve_workspace_root_from_store_root(&store_root, store_dir);
            let label = if owning_workspace == workspace_root {
                ".".to_string()
            } else {
                owning_workspace
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
                    })
            };

            ScanRoot {
                path: normalize_working_dir_path(&store_root.join(entity_dir)),
                label,
            }
        })
        .collect()
}

/// Evaluate the policy against a candidate owning workspace.
///
/// Include overrides win over ignore globs and ignore markers. When the
/// candidate cannot be expressed relative to the workspace root (e.g. an
/// ancestor store), its normalized absolute path string is used for glob
/// matching instead.
fn policy_allows(
    policy: &WorkspacePolicy,
    workspace_root: &Path,
    owning_workspace: &Path,
) -> bool {
    let match_path = owning_workspace
        .strip_prefix(workspace_root)
        .ok()
        .filter(|path| !path.as_os_str().is_empty())
        .map(Path::to_path_buf)
        .unwrap_or_else(|| owning_workspace.to_path_buf());

    !policy.is_ignored(&match_path, owning_workspace)
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
#[path = "workspace_tests.rs"]
mod tests;
