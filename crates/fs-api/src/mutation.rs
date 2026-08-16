use std::{
    fs,
    io::ErrorKind,
};

use crate::{
    error::FsApiError,
    request::{
        CopyFileRequest,
        DeleteDirRequest,
        DeleteFileRequest,
        MoveFileRequest,
        RenameFileRequest,
    },
    response::{
        Conflict,
        ConflictKind,
        MutationResult,
    },
    security::validate_path_within_root,
};
use ignore::WalkBuilder;

/// Move a file or directory.
pub fn move_file(
    request: &MoveFileRequest
) -> Result<MutationResult, FsApiError> {
    let mut conflicts = Vec::new();
    let mut affected_paths = Vec::new();

    // Validate paths against root and capture canonical paths.
    let canonical_from =
        validate_path_within_root(&request.from, &request.root, "source")?;
    let canonical_to =
        validate_path_within_root(&request.to, &request.root, "destination")?;

    if !canonical_from.exists() {
        conflicts.push(Conflict {
            kind: ConflictKind::SourceMissing,
            path: canonical_from.to_path_buf(),
            message: format!(
                "source does not exist: {}",
                canonical_from.display()
            ),
        });
        return Ok(MutationResult {
            affected_paths,
            conflicts,
        });
    }

    if canonical_to.exists() && !request.overwrite {
        conflicts.push(Conflict {
            kind: ConflictKind::DestinationExists,
            path: canonical_to.to_path_buf(),
            message: format!(
                "destination already exists: {}",
                canonical_to.display()
            ),
        });
        return Ok(MutationResult {
            affected_paths,
            conflicts,
        });
    }

    // Ensure parent directory exists.
    if let Some(parent) = canonical_to.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                FsApiError::CannotMoveFile {
                    from: canonical_from.to_path_buf(),
                    to: canonical_to.to_path_buf(),
                    source: e,
                }
            })?;
        }
    }

    fs::rename(&canonical_from, &canonical_to).map_err(|e| {
        if e.kind() == ErrorKind::PermissionDenied {
            return FsApiError::InvalidRequest(format!(
                "permission denied moving from {} to {}",
                canonical_from.display(),
                canonical_to.display()
            ));
        }
        FsApiError::CannotMoveFile {
            from: canonical_from.to_path_buf(),
            to: canonical_to.to_path_buf(),
            source: e,
        }
    })?;

    affected_paths.push(canonical_from.to_path_buf());
    affected_paths.push(canonical_to.to_path_buf());

    Ok(MutationResult {
        affected_paths,
        conflicts,
    })
}

/// Rename a file or directory (in-place within parent directory).
pub fn rename_file(
    request: &RenameFileRequest
) -> Result<MutationResult, FsApiError> {
    let mut conflicts = Vec::new();
    let mut affected_paths = Vec::new();

    // Validate paths against root and capture canonical paths.
    let canonical_from =
        validate_path_within_root(&request.from, &request.root, "source")?;
    let canonical_to =
        validate_path_within_root(&request.to, &request.root, "destination")?;

    if !canonical_from.exists() {
        conflicts.push(Conflict {
            kind: ConflictKind::SourceMissing,
            path: canonical_from.to_path_buf(),
            message: format!(
                "source does not exist: {}",
                canonical_from.display()
            ),
        });
        return Ok(MutationResult {
            affected_paths,
            conflicts,
        });
    }

    if canonical_to.exists() {
        conflicts.push(Conflict {
            kind: ConflictKind::DestinationExists,
            path: canonical_to.to_path_buf(),
            message: format!(
                "destination already exists: {}",
                canonical_to.display()
            ),
        });
        return Ok(MutationResult {
            affected_paths,
            conflicts,
        });
    }

    fs::rename(&canonical_from, &canonical_to).map_err(|e| {
        FsApiError::CannotMoveFile {
            from: canonical_from.to_path_buf(),
            to: canonical_to.to_path_buf(),
            source: e,
        }
    })?;

    affected_paths.push(canonical_from.to_path_buf());
    affected_paths.push(canonical_to.to_path_buf());

    Ok(MutationResult {
        affected_paths,
        conflicts,
    })
}

/// Copy a file.
pub fn copy_file(
    request: &CopyFileRequest
) -> Result<MutationResult, FsApiError> {
    let mut conflicts = Vec::new();
    let mut affected_paths = Vec::new();

    // Validate paths against root and capture canonical paths.
    let canonical_from =
        validate_path_within_root(&request.from, &request.root, "source")?;
    let canonical_to =
        validate_path_within_root(&request.to, &request.root, "destination")?;

    if !canonical_from.exists() {
        conflicts.push(Conflict {
            kind: ConflictKind::SourceMissing,
            path: canonical_from.to_path_buf(),
            message: format!(
                "source does not exist: {}",
                canonical_from.display()
            ),
        });
        return Ok(MutationResult {
            affected_paths,
            conflicts,
        });
    }

    if !canonical_from.is_file() {
        return Err(FsApiError::InvalidRequest(format!(
            "source is not a file: {}",
            canonical_from.display()
        )));
    }

    if canonical_to.exists() && !request.overwrite {
        conflicts.push(Conflict {
            kind: ConflictKind::DestinationExists,
            path: canonical_to.to_path_buf(),
            message: format!(
                "destination already exists: {}",
                canonical_to.display()
            ),
        });
        return Ok(MutationResult {
            affected_paths,
            conflicts,
        });
    }

    // Ensure parent directory exists.
    if let Some(parent) = canonical_to.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(|e| {
                FsApiError::CannotCopyFile {
                    from: canonical_from.to_path_buf(),
                    to: canonical_to.to_path_buf(),
                    source: e,
                }
            })?;
        }
    }

    fs::copy(&canonical_from, &canonical_to).map_err(|e| {
        FsApiError::CannotCopyFile {
            from: canonical_from.to_path_buf(),
            to: canonical_to.to_path_buf(),
            source: e,
        }
    })?;

    affected_paths.push(canonical_to.to_path_buf());

    Ok(MutationResult {
        affected_paths,
        conflicts,
    })
}

/// Delete a file.
pub fn delete_file(
    request: &DeleteFileRequest
) -> Result<MutationResult, FsApiError> {
    let mut conflicts = Vec::new();
    let mut affected_paths = Vec::new();

    // Validate path against root and capture canonical path.
    let canonical_path =
        validate_path_within_root(&request.path, &request.root, "file")?;

    if !canonical_path.exists() {
        conflicts.push(Conflict {
            kind: ConflictKind::SourceMissing,
            path: canonical_path.to_path_buf(),
            message: format!(
                "file does not exist: {}",
                canonical_path.display()
            ),
        });
        return Ok(MutationResult {
            affected_paths,
            conflicts,
        });
    }

    if !canonical_path.is_file() {
        return Err(FsApiError::InvalidRequest(format!(
            "path is not a file: {}",
            canonical_path.display()
        )));
    }

    fs::remove_file(&canonical_path).map_err(|e| {
        FsApiError::CannotDeleteFile {
            path: canonical_path.to_path_buf(),
            source: e,
        }
    })?;

    affected_paths.push(canonical_path.to_path_buf());

    Ok(MutationResult {
        affected_paths,
        conflicts,
    })
}

/// Delete a directory (optionally recursive).
///
/// # Entry Limit
///
/// When `recursive` is true, enforces a 10,000 entry limit to prevent unbounded
/// deletion of large trees. The pre-count short-circuits on exceeding the limit.
pub fn delete_dir(
    request: &DeleteDirRequest
) -> Result<MutationResult, FsApiError> {
    let mut conflicts = Vec::new();
    let mut affected_paths = Vec::new();

    // Validate path against root and capture canonical path.
    let canonical_path =
        validate_path_within_root(&request.path, &request.root, "directory")?;

    if !canonical_path.exists() {
        conflicts.push(Conflict {
            kind: ConflictKind::SourceMissing,
            path: canonical_path.to_path_buf(),
            message: format!(
                "directory does not exist: {}",
                canonical_path.display()
            ),
        });
        return Ok(MutationResult {
            affected_paths,
            conflicts,
        });
    }

    if !canonical_path.is_dir() {
        return Err(FsApiError::InvalidRequest(format!(
            "path is not a directory: {}",
            canonical_path.display()
        )));
    }

    if request.recursive {
        // Apply entry limit to prevent unbounded deletion (consistent with list_dir).
        // Short-circuit on exceeding the limit to avoid walking a hostile tree.
        // Count ALL entries, including errors, to prevent bypass via unreadable trees.
        const MAX_ENTRIES: usize = 10_000;

        let walker = WalkBuilder::new(&canonical_path)
            .follow_links(false)
            .build();

        let mut count = 0usize;
        for _entry in walker {
            count += 1;
            if count > MAX_ENTRIES {
                return Err(FsApiError::InvalidRequest(format!(
                    "recursive delete exceeds entry limit of {}: {}",
                    MAX_ENTRIES,
                    canonical_path.display()
                )));
            }
        }

        fs::remove_dir_all(&canonical_path).map_err(|e| {
            FsApiError::CannotDeleteDirectory {
                path: canonical_path.to_path_buf(),
                source: e,
            }
        })?;
    } else {
        fs::remove_dir(&canonical_path).map_err(|e| {
            FsApiError::CannotDeleteDirectory {
                path: canonical_path.to_path_buf(),
                source: e,
            }
        })?;
    }

    affected_paths.push(canonical_path.to_path_buf());

    Ok(MutationResult {
        affected_paths,
        conflicts,
    })
}
