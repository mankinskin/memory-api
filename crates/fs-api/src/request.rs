use std::path::PathBuf;

use serde::{
    Deserialize,
    Serialize,
};

/// Request to list directory contents with bounded output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListDirRequest {
    /// Root directory to list.
    pub path: PathBuf,

    /// Maximum depth to recurse. None means list only the directory itself.
    pub depth_limit: Option<usize>,

    /// Maximum number of entries to return before truncating.
    pub entry_limit: Option<usize>,

    /// Glob patterns to include (e.g., "*.rs"). If empty, include all.
    pub include_globs: Vec<String>,

    /// Glob patterns to exclude (e.g., "target/**", ".git/**").
    pub exclude_globs: Vec<String>,

    /// Whether to honor .gitignore and other standard filters.
    pub honor_ignore: bool,
}

/// Request to get file/directory metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatRequest {
    /// Path to the file or directory.
    pub path: PathBuf,
}

/// Request to move a file or directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MoveFileRequest {
    /// Source path.
    pub from: PathBuf,

    /// Destination path.
    pub to: PathBuf,

    /// Whether to overwrite existing destination.
    pub overwrite: bool,

    /// Root path for security validation. Both source and destination must
    /// resolve (after following symlinks) to paths within this root. Every
    /// mutation is validated against this root with no opt-out. Transport
    /// layers (CLI, MCP) default this to the current working directory.
    pub root: PathBuf,
}

/// Request to rename a file or directory (in-place).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameFileRequest {
    /// Current path.
    pub from: PathBuf,

    /// New name (relative to parent directory).
    pub to: PathBuf,

    /// Root path for security validation.
    pub root: PathBuf,
}

/// Request to copy a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CopyFileRequest {
    /// Source path.
    pub from: PathBuf,

    /// Destination path.
    pub to: PathBuf,

    /// Whether to overwrite existing destination.
    pub overwrite: bool,

    /// Root path for security validation.
    pub root: PathBuf,
}

/// Request to delete a file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteFileRequest {
    /// Path to the file to delete.
    pub path: PathBuf,

    /// Root path for security validation.
    pub root: PathBuf,
}

/// Request to delete a directory (recursively).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteDirRequest {
    /// Path to the directory to delete.
    pub path: PathBuf,

    /// Whether to allow deleting non-empty directories.
    pub recursive: bool,

    /// Root path for security validation.
    pub root: PathBuf,
}
