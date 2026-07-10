use serde::{
    Deserialize,
    Serialize,
};
use std::path::PathBuf;

use super::entity::EntityManifest;

/// Default filenames — generic, overridable via [`EntityFolderConfig`].
pub const ENTITY_ASSETS_DIR: &str = "assets";
pub const ENTITY_BODY_FILE: &str = "description.md";
pub const ENTITY_HISTORY_FILE: &str = "history.ndjson";
pub const ENTITY_INTERVIEW_DIR: &str = "assets/interviews";
pub const ENTITY_INTERVIEW_QUESTIONS_FILE: &str =
    "assets/interviews/questions.md";
pub const ENTITY_INTERVIEW_ANSWERS_FILE: &str = "assets/interviews/answers.md";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanRoot {
    pub path: PathBuf,
    pub label: String,
}

/// Provenance of a persisted scan root.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum ScanRootSource {
    /// Added by automatic workspace discovery.
    #[default]
    Discovered,
    /// Added explicitly by a user or tool.
    Manual,
    /// Added as a result of a workspace-policy decision.
    Policy,
}

impl ScanRootSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Discovered => "discovered",
            Self::Manual => "manual",
            Self::Policy => "policy",
        }
    }

    pub fn from_str_or_default(value: &str) -> Self {
        match value {
            "manual" => Self::Manual,
            "policy" => Self::Policy,
            _ => Self::Discovered,
        }
    }
}

/// Whether a persisted scan root is included or ignored by policy.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default,
)]
#[serde(rename_all = "lowercase")]
pub enum PolicyDecision {
    /// Root participates in scan and query.
    #[default]
    Included,
    /// Root is excluded by workspace policy.
    Ignored,
}

impl PolicyDecision {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Included => "included",
            Self::Ignored => "ignored",
        }
    }

    pub fn from_str_or_default(value: &str) -> Self {
        match value {
            "ignored" => Self::Ignored,
            _ => Self::Included,
        }
    }

    pub fn is_ignored(&self) -> bool {
        matches!(self, Self::Ignored)
    }
}

/// Auditability metadata persisted alongside a scan root.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanRootMetadata {
    #[serde(default)]
    pub source: ScanRootSource,
    #[serde(default)]
    pub policy_decision: PolicyDecision,
    #[serde(default)]
    pub workspace_root: Option<PathBuf>,
}

impl Default for ScanRootMetadata {
    fn default() -> Self {
        Self {
            source: ScanRootSource::Discovered,
            policy_decision: PolicyDecision::Included,
            workspace_root: None,
        }
    }
}

/// A scan root together with its persisted auditability metadata.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PersistedScanRoot {
    pub root: ScanRoot,
    pub metadata: ScanRootMetadata,
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
    /// Filename for the markdown body asset stored beside the manifest.
    pub body_file: &'static str,
    /// Subdirectory for binary assets (default: `"assets"`).
    pub assets_dir: &'static str,
    /// Filename for the append-only history log (default: `"history.ndjson"`).
    pub history_file: &'static str,
}

impl EntityFolderConfig {
    pub const fn new(
        manifest_file: &'static str,
        lock_file: &'static str,
    ) -> Self {
        Self {
            manifest_file,
            lock_file,
            body_file: ENTITY_BODY_FILE,
            assets_dir: ENTITY_ASSETS_DIR,
            history_file: ENTITY_HISTORY_FILE,
        }
    }

    pub const fn with_body_file(
        mut self,
        body_file: &'static str,
    ) -> Self {
        self.body_file = body_file;
        self
    }
}

pub fn parse_entity_manifest_toml(
    path: PathBuf,
    content: &str,
) -> Result<EntityManifest, ParseDiagnostic> {
    toml::from_str::<EntityManifest>(content).map_err(|err| ParseDiagnostic {
        path,
        reason: err.to_string(),
    })
}

pub fn has_minimum_entity_contract(
    entries: &[&str],
    manifest_file: &str,
) -> bool {
    entries.contains(&manifest_file)
}
