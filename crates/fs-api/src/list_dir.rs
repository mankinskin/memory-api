use globset::{Glob, GlobSetBuilder};
use ignore::WalkBuilder;

use crate::{
    error::FsApiError,
    request::ListDirRequest,
    response::{DirEntry, EntryKind, ListDirResult},
};

/// List directory contents with bounded output.
///
/// # Security
///
/// - Rejects paths that escape the provided root.
/// - Enforces depth and entry limits to prevent memory exhaustion.
/// - Does not follow symlinks outside the root by default.
pub fn list_dir(request: &ListDirRequest) -> Result<ListDirResult, FsApiError> {
    let root = &request.path;

    if !root.exists() {
        return Err(FsApiError::PathNotFound {
            path: root.to_path_buf(),
        });
    }

    if !root.is_dir() {
        return Err(FsApiError::InvalidRequest(format!(
            "path is not a directory: {}",
            root.display()
        )));
    }

    // Canonicalize to detect path traversal attempts.
    let canonical_root = root.canonicalize().map_err(|e| FsApiError::CannotReadDirectory {
        path: root.to_path_buf(),
        source: e,
    })?;

    // Build exclude globset.
    let exclude_matcher = if !request.exclude_globs.is_empty() {
        let mut builder = GlobSetBuilder::new();
        for pattern in &request.exclude_globs {
            let glob = Glob::new(pattern).map_err(|e| {
                FsApiError::InvalidRequest(format!("invalid exclude glob '{}': {}", pattern, e))
            })?;
            builder.add(glob);
        }
        Some(builder.build().map_err(|e| {
            FsApiError::InvalidRequest(format!("failed to build exclude globset: {}", e))
        })?)
    } else {
        None
    };

    // Build include globset.
    let include_matcher = if !request.include_globs.is_empty() {
        let mut builder = GlobSetBuilder::new();
        for pattern in &request.include_globs {
            let glob = Glob::new(pattern).map_err(|e| {
                FsApiError::InvalidRequest(format!("invalid include glob '{}': {}", pattern, e))
            })?;
            builder.add(glob);
        }
        Some(builder.build().map_err(|e| {
            FsApiError::InvalidRequest(format!("failed to build include globset: {}", e))
        })?)
    } else {
        None
    };

    let mut entries = Vec::new();
    let entry_limit = request.entry_limit.unwrap_or(10_000);
    let mut total_found = 0usize;
    let mut truncated = false;

    let mut walker = WalkBuilder::new(&canonical_root);
    walker.standard_filters(request.honor_ignore);
    walker.hidden(false);
    // Explicitly disable symlink following for security.
    walker.follow_links(false);

    if let Some(depth) = request.depth_limit {
        walker.max_depth(Some(depth));
    }

    // Apply exclude globs via filter_entry.
    if let Some(ref matcher) = exclude_matcher {
        let canonical_root_clone = canonical_root.clone();
        let matcher_clone = matcher.clone();
        walker.filter_entry(move |entry| {
            let Ok(relative_path) = entry.path().strip_prefix(&canonical_root_clone) else {
                return true;
            };

            if relative_path.as_os_str().is_empty() {
                return true; // Keep the root itself.
            }

            !matcher_clone.is_match(relative_path)
        });
    }

    for entry_result in walker.build() {
        let entry = match entry_result {
            Ok(entry) => entry,
            Err(_) => continue, // Skip inaccessible entries.
        };

        let path = entry.path();

        // Skip the root itself.
        if path == canonical_root {
            continue;
        }

        let relative_path = match path.strip_prefix(&canonical_root) {
            Ok(rel) => rel.to_path_buf(),
            Err(_) => continue,
        };

        // Apply include globs if specified.
        if let Some(ref matcher) = include_matcher {
            if !matcher.is_match(&relative_path) {
                continue;
            }
        }

        total_found += 1;

        if entries.len() >= entry_limit {
            truncated = true;
            continue; // Count but don't add.
        }

        let metadata = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        let kind = if metadata.is_dir() {
            EntryKind::Directory
        } else if metadata.is_symlink() {
            EntryKind::Symlink
        } else {
            EntryKind::File
        };

        let size = if kind == EntryKind::File {
            Some(metadata.len())
        } else {
            None
        };

        entries.push(DirEntry {
            path: relative_path,
            kind,
            size,
        });
    }

    Ok(ListDirResult {
        entries,
        truncated,
        total_found: Some(total_found),
    })
}
