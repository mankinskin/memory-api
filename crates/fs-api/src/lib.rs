//! Bounded filesystem operations API.
//!
//! Provides core logic for listing directories, reading metadata, and performing
//! conflict-aware file mutations. This API is transport-agnostic and designed to
//! be used by CLI, MCP, and HTTP adapters.
//!
//! # Security
//!
//! - Path traversal protection: all operations reject or canonicalize paths that
//!   escape the provided root.
//! - Bounded operations: depth and entry limits prevent memory exhaustion on large
//!   or malicious directory trees.
//! - No symlink following outside the root by default.
//!
//! # Entry points
//!
//! - [`list_dir()`]: List directory contents with bounded output.
//! - [`stat()`]: Get file/directory metadata without reading content.
//! - [`move_file()`], [`rename_file()`], [`copy_file()`], [`delete_file()`],
//!   [`delete_dir()`]: Conflict-aware mutation operations.

pub mod error;
pub mod list_dir;
pub mod mutation;
pub mod request;
pub mod response;
pub mod stat;
mod security;

pub use error::FsApiError;
pub use list_dir::list_dir;
pub use mutation::{copy_file, delete_dir, delete_file, move_file, rename_file};
pub use request::{
    CopyFileRequest, DeleteDirRequest, DeleteFileRequest, ListDirRequest, MoveFileRequest,
    RenameFileRequest, StatRequest,
};
pub use response::{
    Conflict, ConflictKind, DirEntry, EntryKind, ListDirResult, MutationResult, StatResult,
};
pub use stat::stat;
