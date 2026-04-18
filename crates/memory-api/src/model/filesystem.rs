use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use super::entity::EntityManifest;

/// Default filenames — generic, overridable via [`EntityFolderConfig`].
pub const ENTITY_ASSETS_DIR: &str = "assets";
pub const ENTITY_HISTORY_FILE: &str = "history.ndjson";
pub const ENTITY_INTERVIEW_DIR: &str = "assets/interviews";
pub const ENTITY_INTERVIEW_QUESTIONS_FILE: &str = "assets/interviews/questions.md";
pub const ENTITY_INTERVIEW_ANSWERS_FILE: &str = "assets/interviews/answers.md";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanRoot {
    pub path: PathBuf,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ParseDiagnostic {
    pub path: PathBuf,
    pub reason: String,
}

/// Per-domain folder layout configuration.
///
/// Parameterizes the filenames used inside each entity folder so that
/// `ticket-api` (with `ticket.toml` / `.ticket-lock`) and `spec-api`
/// (with `spec.toml` / `.spec-lock`) can share the same generic
/// [`EntityFs`](super::super::storage::entity_fs::EntityFs) implementation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EntityFolderConfig {
    /// Filename for the canonical manifest inside each entity folder
    /// (e.g. `"ticket.toml"` or `"spec.toml"`).
    pub manifest_file: &'static str,
    /// Filename for the advisory lock file
    /// (e.g. `".ticket-lock"` or `".spec-lock"`).
    pub lock_file: &'static str,
    /// Subdirectory for binary assets (default: `"assets"`).
    pub assets_dir: &'static str,
    /// Filename for the append-only history log (default: `"history.ndjson"`).
    pub history_file: &'static str,
}

impl EntityFolderConfig {
    pub const fn new(manifest_file: &'static str, lock_file: &'static str) -> Self {
        Self {
            manifest_file,
            lock_file,
            assets_dir: ENTITY_ASSETS_DIR,
            history_file: ENTITY_HISTORY_FILE,
        }
    }
}

pub fn parse_entity_manifest_toml(path: PathBuf, content: &str) -> Result<EntityManifest, ParseDiagnostic> {
    toml::from_str::<EntityManifest>(content).map_err(|err| ParseDiagnostic {
        path,
        reason: err.to_string(),
    })
}

pub fn has_minimum_entity_contract(entries: &[&str], manifest_file: &str) -> bool {
    entries.contains(&manifest_file)
}
