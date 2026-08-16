use std::path::PathBuf;

use serde::{
    Deserialize,
    Serialize,
};

/// Conflict kind for mutation operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    DestinationExists,
    SourceMissing,
    PermissionDenied,
    Other(String),
}

/// Conflict encountered during a mutation operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Conflict {
    pub kind: ConflictKind,
    pub path: PathBuf,
    pub message: String,
}

/// Entry kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntryKind {
    File,
    Directory,
    Symlink,
}

/// A directory entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirEntry {
    /// Relative path from the requested root.
    pub path: PathBuf,
    pub kind: EntryKind,
    pub size: Option<u64>,
}

/// Result of a list_dir operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListDirResult {
    pub entries: Vec<DirEntry>,
    /// Whether results were truncated due to entry limit.
    pub truncated: bool,
    /// Total entries found (if available), even beyond the limit.
    pub total_found: Option<usize>,
}

/// Metadata result for a single path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatResult {
    pub exists: bool,
    pub kind: Option<EntryKind>,
    pub size: Option<u64>,
    /// Modified time as Unix epoch seconds.
    pub modified_secs: Option<u64>,
}

/// Result of a mutation operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MutationResult {
    /// Paths affected by the operation.
    pub affected_paths: Vec<PathBuf>,
    /// Conflicts encountered.
    pub conflicts: Vec<Conflict>,
}
