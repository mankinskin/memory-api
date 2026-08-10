use std::{
    collections::BTreeMap,
    path::{
        Path,
        PathBuf,
    },
};

use serde::Deserialize;
use tempfile::TempDir;

#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("manifest parse error in {path}: {source}")]
    ManifestParse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("fixture root not found: {0}")]
    MissingFixtureRoot(PathBuf),
    #[error("no writable storeless workspace base found from {start}")]
    NoStorelessWorkspaceBase { start: PathBuf },
    #[error("git command {args:?} failed in {dir}: {detail}")]
    Git {
        dir: PathBuf,
        args: Vec<String>,
        detail: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct FixtureManifest {
    pub fixture_name: String,
    pub stores: Vec<StoreDef>,
    pub worktrees: Vec<WorktreeDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StoreDef {
    pub domain: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorktreeDef {
    pub name: String,
    pub relative_path: String,
    pub kind: String,
}

#[derive(Debug)]
pub struct LoadedFixture {
    pub tempdir: TempDir,
    pub workspace_root: PathBuf,
    pub manifest: FixtureManifest,
    pub store_roots: BTreeMap<String, PathBuf>,
}

#[derive(Debug)]
pub struct EmptyWorkspace {
    pub(crate) tempdir: TempDir,
}

impl EmptyWorkspace {
    pub fn path(&self) -> &Path {
        self.tempdir.path()
    }

    pub fn snapshot(&self) -> Result<WorkspaceSnapshot, FixtureError> {
        crate::snapshot_workspace(self.path())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceSnapshot {
    pub(crate) entries: BTreeMap<PathBuf, WorkspaceEntry>,
}

impl WorkspaceSnapshot {
    pub fn diff(
        &self,
        after: &Self,
    ) -> WorkspaceDelta {
        let mut delta = WorkspaceDelta::default();

        for (path, entry) in &after.entries {
            match self.entries.get(path) {
                None => delta.added.push(path.clone()),
                Some(before_entry) if before_entry != entry => {
                    delta.changed.push(path.clone());
                },
                Some(_) => {},
            }
        }

        for path in self.entries.keys() {
            if !after.entries.contains_key(path) {
                delta.removed.push(path.clone());
            }
        }

        delta
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkspaceEntry {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceDelta {
    pub added: Vec<PathBuf>,
    pub removed: Vec<PathBuf>,
    pub changed: Vec<PathBuf>,
}

impl WorkspaceDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.removed.is_empty()
            && self.changed.is_empty()
    }
}

impl LoadedFixture {
    pub fn store_root(
        &self,
        domain: &str,
    ) -> Option<&Path> {
        self.store_roots.get(domain).map(PathBuf::as_path)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct TicketPerfFixtureOptions {
    pub root_generated_ticket_count: usize,
    pub submodule_generated_ticket_count: usize,
    pub tracked_reference_file_count: usize,
    pub references_per_file: usize,
}

impl Default for TicketPerfFixtureOptions {
    fn default() -> Self {
        Self {
            root_generated_ticket_count: 180,
            submodule_generated_ticket_count: 96,
            tracked_reference_file_count: 16,
            references_per_file: 20,
        }
    }
}

impl TicketPerfFixtureOptions {
    pub fn heavy() -> Self {
        Self {
            root_generated_ticket_count: 240,
            submodule_generated_ticket_count: 64,
            tracked_reference_file_count: 18,
            references_per_file: 28,
        }
    }

    pub fn stress() -> Self {
        Self {
            root_generated_ticket_count: 480,
            submodule_generated_ticket_count: 160,
            tracked_reference_file_count: 36,
            references_per_file: 48,
        }
    }
}

#[derive(Debug)]
pub struct TicketPerfFixture {
    pub fixture: LoadedFixture,
    pub root_ticket_ids: Vec<String>,
    pub submodule_ticket_ids: Vec<String>,
    pub tracked_reference_files: Vec<PathBuf>,
}
