//! fs — bounded filesystem operations utility
//!
//! Provides safe directory listing and conflict-aware file mutations with bounded output.
//!
//! ## Usage
//!
//! ```text
//! # List directory with depth and entry limits
//! fs list-dir /path/to/dir --depth 2 --limit 100
//!
//! # List with glob patterns
//! fs list-dir . --include "*.rs" --exclude "target/**"
//!
//! # Get file/directory metadata
//! fs stat /path/to/file
//!
//! # Move file (conflict detection)
//! fs move /src /dst
//! fs move /src /dst --overwrite
//!
//! # Rename file (in-place)
//! fs rename /old/path /new/name
//!
//! # Copy file
//! fs copy /src /dst
//! fs copy /src /dst --overwrite
//!
//! # Delete operations
//! fs delete-file /path/to/file
//! fs delete-dir /path/to/dir --recursive
//!
//! # Output formats
//! fs list-dir . --json
//! fs list-dir . --toon
//! ```

use std::path::PathBuf;

use clap::{
    Parser,
    Subcommand,
};
use fs_api::{
    CopyFileRequest,
    DeleteDirRequest,
    DeleteFileRequest,
    FsApiError,
    ListDirRequest,
    MoveFileRequest,
    RenameFileRequest,
    StatRequest,
    copy_file,
    delete_dir,
    delete_file,
    list_dir,
    move_file,
    rename_file,
    stat,
};

/// fs — bounded filesystem operations.
///
/// Safe directory listing and conflict-aware file mutations with bounded output.
#[derive(Debug, Parser)]
#[command(name = "fs", version, about)]
struct Args {
    #[command(subcommand)]
    command: Command,

    /// Output as JSON.
    #[arg(long, global = true, conflicts_with = "toon")]
    json: bool,

    /// Output as TOON (compact machine-readable format).
    #[arg(long, global = true)]
    toon: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// List directory contents with bounded output.
    ///
    /// Supports depth limits, entry limits, glob patterns, and .gitignore honor.
    ListDir {
        /// Directory path to list.
        path: PathBuf,

        /// Maximum depth to recurse. Omit to list only the directory itself.
        #[arg(long)]
        depth: Option<usize>,

        /// Maximum number of entries to return before truncating.
        #[arg(long)]
        limit: Option<usize>,

        /// Glob patterns to include (e.g., "*.rs"). Can be specified multiple times.
        #[arg(long)]
        include: Vec<String>,

        /// Glob patterns to exclude (e.g., "target/**", ".git/**"). Can be specified multiple times.
        #[arg(long)]
        exclude: Vec<String>,

        /// Honor .gitignore and other standard filters.
        #[arg(long)]
        honor_ignore: bool,
    },

    /// Get file/directory metadata without reading content.
    Stat {
        /// Path to the file or directory.
        path: PathBuf,
    },

    /// Move a file or directory.
    ///
    /// Conflict detection: reports DestinationExists unless --overwrite is set.
    Move {
        /// Source path.
        from: PathBuf,

        /// Destination path.
        to: PathBuf,

        /// Overwrite existing destination.
        #[arg(long)]
        overwrite: bool,

        /// Root directory for security validation. Both source and destination must
        /// remain within this root after resolving symlinks. Defaults to current
        /// working directory.
        #[arg(long)]
        root: Option<PathBuf>,
    },

    /// Rename a file or directory (in-place).
    Rename {
        /// Current path.
        from: PathBuf,

        /// New name (relative to parent directory).
        to: PathBuf,

        /// Root directory for security validation.
        #[arg(long)]
        root: Option<PathBuf>,
    },

    /// Copy a file.
    Copy {
        /// Source path.
        from: PathBuf,

        /// Destination path.
        to: PathBuf,

        /// Overwrite existing destination.
        #[arg(long)]
        overwrite: bool,

        /// Root directory for security validation.
        #[arg(long)]
        root: Option<PathBuf>,
    },

    /// Delete a file.
    DeleteFile {
        /// Path to the file to delete.
        path: PathBuf,

        /// Root directory for security validation.
        #[arg(long)]
        root: Option<PathBuf>,
    },

    /// Delete a directory (optionally recursive).
    DeleteDir {
        /// Path to the directory to delete.
        path: PathBuf,

        /// Allow deleting non-empty directories.
        #[arg(long)]
        recursive: bool,

        /// Root directory for security validation.
        #[arg(long)]
        root: Option<PathBuf>,
    },
}

fn main() -> Result<(), FsApiError> {
    let args = Args::parse();

    let output_format = if args.json {
        OutputFormat::Json
    } else if args.toon {
        OutputFormat::Toon
    } else {
        OutputFormat::Json // default
    };

    match args.command {
        Command::ListDir {
            path,
            depth,
            limit,
            include,
            exclude,
            honor_ignore,
        } => {
            let request = ListDirRequest {
                path,
                depth_limit: depth,
                entry_limit: limit,
                include_globs: include,
                exclude_globs: exclude,
                honor_ignore,
            };

            let result = list_dir(&request)?;
            print_output(&result, output_format)?;
        },

        Command::Stat { path } => {
            let request = StatRequest { path };
            let result = stat(&request)?;
            print_output(&result, output_format)?;
        },

        Command::Move {
            from,
            to,
            overwrite,
            root,
        } => {
            let root = match root {
                Some(r) => r,
                None => std::env::current_dir().map_err(|e| {
                    FsApiError::InvalidRequest(format!(
                        "cannot determine current directory for root validation: {}",
                        e
                    ))
                })?,
            };
            let request = MoveFileRequest {
                from,
                to,
                overwrite,
                root,
            };
            let result = move_file(&request)?;
            print_output(&result, output_format)?;
        },

        Command::Rename { from, to, root } => {
            let root = match root {
                Some(r) => r,
                None => std::env::current_dir().map_err(|e| {
                    FsApiError::InvalidRequest(format!(
                        "cannot determine current directory for root validation: {}",
                        e
                    ))
                })?,
            };
            let request = RenameFileRequest { from, to, root };
            let result = rename_file(&request)?;
            print_output(&result, output_format)?;
        },

        Command::Copy {
            from,
            to,
            overwrite,
            root,
        } => {
            let root = match root {
                Some(r) => r,
                None => std::env::current_dir().map_err(|e| {
                    FsApiError::InvalidRequest(format!(
                        "cannot determine current directory for root validation: {}",
                        e
                    ))
                })?,
            };
            let request = CopyFileRequest {
                from,
                to,
                overwrite,
                root,
            };
            let result = copy_file(&request)?;
            print_output(&result, output_format)?;
        },

        Command::DeleteFile { path, root } => {
            let root = match root {
                Some(r) => r,
                None => std::env::current_dir().map_err(|e| {
                    FsApiError::InvalidRequest(format!(
                        "cannot determine current directory for root validation: {}",
                        e
                    ))
                })?,
            };
            let request = DeleteFileRequest { path, root };
            let result = delete_file(&request)?;
            print_output(&result, output_format)?;
        },

        Command::DeleteDir {
            path,
            recursive,
            root,
        } => {
            let root = match root {
                Some(r) => r,
                None => std::env::current_dir().map_err(|e| {
                    FsApiError::InvalidRequest(format!(
                        "cannot determine current directory for root validation: {}",
                        e
                    ))
                })?,
            };
            let request = DeleteDirRequest {
                path,
                recursive,
                root,
            };
            let result = delete_dir(&request)?;
            print_output(&result, output_format)?;
        },
    }

    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum OutputFormat {
    Json,
    Toon,
}

fn print_output<T: serde::Serialize>(
    value: &T,
    format: OutputFormat,
) -> Result<(), FsApiError> {
    match format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(value).map_err(|e| {
                FsApiError::InvalidRequest(format!(
                    "serialization error: {}",
                    e
                ))
            })?;
            println!("{}", json);
        },
        OutputFormat::Toon => {
            let toon = toon_format::encode_default(value).map_err(|e| {
                FsApiError::InvalidRequest(format!(
                    "TOON encoding error: {}",
                    e
                ))
            })?;
            println!("{}", toon);
        },
    }
    Ok(())
}
