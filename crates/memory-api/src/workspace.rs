//! Workspace management: named index roots with global registry and per-project
//! local overrides.
//!
//! ## Resolution order (highest priority first)
//!
//! 1. `--index-root` CLI flag / env var  (handled by caller)
//! 2. `.ticket-workspace` file found by walking up from the current directory
//! 3. Active workspace in `~/.ticket-workspaces.toml`
//! 4. Built-in default `~/.ticket-index/`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Filename for the global workspace registry.
pub const WORKSPACE_CONFIG_FILE: &str = ".ticket-workspaces.toml";

/// Filename searched upward from cwd for a project-local workspace override.
pub const LOCAL_WORKSPACE_FILE: &str = ".ticket-workspace";

// ── Config file ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct WorkspaceConfig {
    /// Name of the currently active workspace.
    pub active: Option<String>,
    /// Named workspaces: name → absolute path string.
    #[serde(default)]
    pub workspaces: BTreeMap<String, String>,
}

impl WorkspaceConfig {
    /// Path to the global workspace registry file.
    pub fn config_path() -> PathBuf {
        dirs_home().join(WORKSPACE_CONFIG_FILE)
    }

    /// Load the global registry. Missing file = empty config (not an error).
    pub fn load() -> Self {
        let path = Self::config_path();
        let Ok(content) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        toml::from_str(&content).unwrap_or_default()
    }

    /// Persist the registry back to disk.
    pub fn save(&self) -> std::io::Result<()> {
        let path = Self::config_path();
        let content = toml::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        std::fs::write(path, content)
    }

    /// Register a new workspace. Fails if the name is already taken.
    pub fn add(&mut self, name: &str, path: PathBuf) -> Result<(), String> {
        if self.workspaces.contains_key(name) {
            return Err(format!("workspace '{}' already exists", name));
        }
        self.workspaces
            .insert(name.to_string(), path.to_string_lossy().to_string());
        Ok(())
    }

    /// Set the active workspace by name. Fails if the name is not registered.
    pub fn set_active(&mut self, name: &str) -> Result<(), String> {
        if !self.workspaces.contains_key(name) {
            return Err(format!(
                "workspace '{}' is not registered; run `ticket workspace new {}` first",
                name, name
            ));
        }
        self.active = Some(name.to_string());
        Ok(())
    }

    /// Remove a workspace from the registry. Does not delete data on disk.
    pub fn remove(&mut self, name: &str) -> Result<(), String> {
        if self.workspaces.remove(name).is_none() {
            return Err(format!("workspace '{}' is not registered", name));
        }
        if self.active.as_deref() == Some(name) {
            self.active = None;
        }
        Ok(())
    }

    /// Resolve the active workspace to a path, if one is set and valid.
    pub fn active_path(&self) -> Option<PathBuf> {
        let name = self.active.as_deref()?;
        self.workspaces.get(name).map(PathBuf::from)
    }

    /// Resolve a name (or absolute path string) from a local `.ticket-workspace` file.
    pub fn resolve_value(&self, value: &str) -> Option<PathBuf> {
        let p = PathBuf::from(value.trim());
        if p.is_absolute() {
            return Some(p);
        }
        self.workspaces.get(value.trim()).map(PathBuf::from)
    }
}

// ── Local .ticket-workspace file ─────────────────────────────────────────────

/// Walk upward from `cwd` looking for a `.ticket-workspace` file.
pub fn find_local_workspace_file() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    find_local_workspace_file_from(&cwd)
}

/// Walk upward from `start` looking for a `.ticket-workspace` file.
pub fn find_local_workspace_file_from(start: &Path) -> Option<PathBuf> {
    let mut dir = start;
    loop {
        let candidate = dir.join(LOCAL_WORKSPACE_FILE);
        if candidate.is_file() {
            return Some(candidate);
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => return None,
        }
    }
}

// ── Workspace resolution ──────────────────────────────────────────────────────

/// The layer that produced the resolved index root — useful for diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceSource {
    /// `--index-root` flag or env var.
    Explicit,
    /// `.ticket-workspace` file found by walking up from cwd.
    LocalFile(PathBuf),
    /// Active workspace from `~/.ticket-workspaces.toml`.
    ActiveWorkspace(String),
    /// Built-in default `~/.ticket-index/`.
    Default,
}

impl WorkspaceSource {
    pub fn description(&self) -> String {
        match self {
            Self::Explicit => "–– (--index-root / env var)".to_string(),
            Self::LocalFile(p) => format!("local .ticket-workspace ({})", p.display()),
            Self::ActiveWorkspace(name) => format!("active workspace '{}'", name),
            Self::Default => "built-in default (~/.ticket-index)".to_string(),
        }
    }
}

/// Resolve the active index root using the full resolution chain.
///
/// Returns `(resolved_path, source)`.
pub fn resolve_workspace() -> (PathBuf, WorkspaceSource) {
    let config = WorkspaceConfig::load();

    // Layer 2: project-local .ticket-workspace file
    if let Some(local_file) = find_local_workspace_file() {
        if let Ok(content) = std::fs::read_to_string(&local_file) {
            let value = content.trim();
            if !value.is_empty() {
                let p = PathBuf::from(value);
                let resolved = if p.is_absolute() {
                    p
                } else {
                    local_file
                        .parent()
                        .unwrap_or_else(|| Path::new("."))
                        .join(&p)
                };
                return (resolved, WorkspaceSource::LocalFile(local_file));
            }
        }
    }

    // Layer 3: active workspace in ~/.ticket-workspaces.toml
    if let Some(name) = config.active.as_deref() {
        if let Some(path) = config.active_path() {
            return (path, WorkspaceSource::ActiveWorkspace(name.to_string()));
        }
    }

    // Layer 4: built-in default
    (dirs_home().join(".ticket-index"), WorkspaceSource::Default)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

/// Compute a relative path from `base_dir` to `target`.
pub fn make_relative_path(base_dir: &Path, target: &Path) -> PathBuf {
    use std::path::Component;

    let base_abs = base_dir
        .canonicalize()
        .unwrap_or_else(|_| base_dir.to_path_buf());

    let target_abs = if target.is_absolute() {
        target.to_path_buf()
    } else {
        base_abs.join(target)
    };

    let base_parts: Vec<Component> = base_abs.components().collect();
    let target_parts: Vec<Component> = target_abs.components().collect();

    let common = base_parts
        .iter()
        .zip(target_parts.iter())
        .take_while(|(a, b)| a == b)
        .count();

    if common == 0 {
        return target_abs;
    }

    let up_count = base_parts.len() - common;
    let mut rel = PathBuf::new();
    for _ in 0..up_count {
        rel.push("..");
    }
    for part in &target_parts[common..] {
        rel.push(part);
    }

    if rel.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        rel
    }
}

fn dirs_home() -> PathBuf {
    #[cfg(windows)]
    return PathBuf::from(
        std::env::var("USERPROFILE")
            .or_else(|_| {
                std::env::var("HOMEDRIVE")
                    .and_then(|d| std::env::var("HOMEPATH").map(|p| d + &p))
            })
            .unwrap_or_else(|_| ".".to_string()),
    );
    #[cfg(not(windows))]
    return PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| ".".to_string()));
}
