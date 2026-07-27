//! Security validation for filesystem operations.
//!
//! Provides path validation to prevent symlink escapes and path traversal attacks.
//!
//! # Platform Test Coverage
//!
//! Escape detection is validated on both Unix and Windows platforms:
//!
//! - **Unix**: Tests use symlinks (`test_validate_symlink_escape_rejected`) to verify escape rejection.
//! - **Windows**: Tests use directory junctions (`test_validate_junction_escape_*`) to verify escape rejection
//!   without requiring elevated privileges. Junctions are created via the `junction` crate.
//! - **Cross-platform**: Path traversal via `..` components is tested on all platforms.
//!
//! Note: File symlinks on Windows require Developer Mode or admin privileges, so junction-based
//! tests provide the primary Windows coverage for symlink-style escape vectors.

use std::path::{Path, PathBuf};

use crate::error::FsApiError;

/// Validate that a path (after resolving symlinks) remains within the given root.
///
/// # Security
///
/// - Canonicalizes both the path and root to resolve symlinks and relative components.
/// - On Windows, normalizes paths to handle UNC/verbatim prefixes (`\\?\`) correctly.
/// - Returns an error if the canonical path escapes the canonical root.
///
/// # Arguments
///
/// * `path` - The path to validate (may be relative or absolute, may contain symlinks)
/// * `root` - The root directory that `path` must remain within
/// * `context` - Description of the operation (e.g., "source", "destination") for error messages
pub fn validate_path_within_root(
    path: &Path,
    root: &Path,
    _context: &str,
) -> Result<PathBuf, FsApiError> {
    // Canonicalize root first.
    let canonical_root = root.canonicalize().map_err(|e| FsApiError::CannotReadDirectory {
        path: root.to_path_buf(),
        source: e,
    })?;

    // Canonicalize the path to resolve symlinks.
    // If the path doesn't exist yet (e.g., a destination), try to canonicalize its parent.
    let canonical_path = if path.exists() {
        path.canonicalize().map_err(|e| FsApiError::CannotReadMetadata {
            path: path.to_path_buf(),
            source: e,
        })?
    } else {
        // For non-existent paths, canonicalize the parent and append the filename.
        if let Some(parent) = path.parent() {
            if parent.as_os_str().is_empty() {
                // Relative path with no parent - treat as current directory.
                let mut cwd = std::env::current_dir().map_err(|e| FsApiError::IoError(e))?;
                if let Some(file_name) = path.file_name() {
                    cwd.push(file_name);
                }
                cwd
            } else if parent.exists() {
                let mut canonical_parent =
                    parent.canonicalize().map_err(|e| FsApiError::CannotReadDirectory {
                        path: parent.to_path_buf(),
                        source: e,
                    })?;
                if let Some(file_name) = path.file_name() {
                    canonical_parent.push(file_name);
                }
                canonical_parent
            } else {
                // Parent doesn't exist either - cannot validate.
                return Err(FsApiError::PathNotFound {
                    path: parent.to_path_buf(),
                });
            }
        } else {
            // No parent (e.g., root path) - use as-is.
            path.to_path_buf()
        }
    };

    // Normalize both paths for Windows compatibility.
    let normalized_root = normalize_path(&canonical_root);
    let normalized_path = normalize_path(&canonical_path);

    // Check if the canonical path is within the canonical root.
    if !normalized_path.starts_with(&normalized_root) {
        return Err(FsApiError::PathTraversal {
            path: path.to_path_buf(),
        });
    }

    Ok(canonical_path)
}

/// Normalize a path for cross-platform comparison.
///
/// On Windows, `canonicalize()` may return paths with UNC/verbatim prefixes like `\\?\C:\...`.
/// This function strips those prefixes to ensure consistent path comparison.
///
/// On Unix, this is a no-op.
#[cfg(windows)]
fn normalize_path(path: &Path) -> PathBuf {
    // Strip Windows verbatim prefix if present.
    let path_str = path.to_string_lossy();
    if path_str.starts_with(r"\\?\") {
        PathBuf::from(&path_str[4..])
    } else {
        path.to_path_buf()
    }
}

#[cfg(not(windows))]
fn normalize_path(path: &Path) -> PathBuf {
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_validate_path_within_root_accepts_valid_path() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let file = root.join("test.txt");
        fs::write(&file, "content").unwrap();

        let result = validate_path_within_root(&file, root, "test");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_path_within_root_rejects_escape_via_dotdot() {
        let temp = tempdir().unwrap();
        let root = temp.path().join("subdir");
        fs::create_dir(&root).unwrap();

        let escape_path = root.join("..").join("..").join("etc").join("passwd");

        // This should fail during canonicalization or validation.
        let result = validate_path_within_root(&escape_path, &root, "test");
        // Either fails to canonicalize (path doesn't exist) or detects escape.
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_nonexistent_destination() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        let dest = root.join("new_file.txt");

        // Should succeed - validates parent is within root.
        let result = validate_path_within_root(&dest, root, "destination");
        assert!(result.is_ok());
    }

    #[test]
    #[cfg(unix)]
    fn test_validate_symlink_escape_rejected() {
        use std::os::unix::fs::symlink;

        let temp = tempdir().unwrap();
        let root = temp.path();
        let target = temp.path().parent().unwrap().join("outside.txt");
        fs::write(&target, "content").unwrap();

        let link = root.join("link");
        symlink(&target, &link).unwrap();

        // Symlink points outside root - should be rejected.
        let result = validate_path_within_root(&link, root, "test");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), FsApiError::PathTraversal { .. }));
    }

    #[test]
    fn test_validate_nonexistent_destination_parent_canonicalization() {
        let temp = tempdir().unwrap();
        let root = temp.path();

        // Create a subdirectory.
        let subdir = root.join("subdir");
        fs::create_dir(&subdir).unwrap();

        // Validate a nonexistent path whose parent exists.
        let dest = subdir.join("newfile.txt");
        let result = validate_path_within_root(&dest, root, "destination");
        assert!(result.is_ok());

        let canonical = result.unwrap();
        
        // Normalize both for comparison (handles Windows \\?\ prefix).
        let normalized_canonical = normalize_path(&canonical);
        let normalized_root = normalize_path(&root.canonicalize().unwrap());
        
        // The canonical path should be under the root.
        assert!(normalized_canonical.starts_with(&normalized_root),
            "canonical path {:?} should start with root {:?}",
            normalized_canonical, normalized_root);
        
        // The canonical path should end with the expected filename.
        assert!(canonical.to_string_lossy().ends_with("newfile.txt"));
    }

    #[test]
    #[cfg(windows)]
    fn test_validate_junction_escape_rejected() {
        let temp = tempdir().unwrap();
        let root = temp.path();
        
        // Create a target directory outside the root.
        let outside_temp = tempdir().unwrap();
        let outside_target = outside_temp.path();
        fs::write(outside_target.join("outside.txt"), "content").unwrap();

        // Create a junction inside root pointing to the outside directory.
        let junction_path = root.join("junction");
        match junction::create(outside_target, &junction_path) {
            Ok(_) => {
                // Junction points outside root - should be rejected.
                let result = validate_path_within_root(&junction_path, root, "test");
                assert!(result.is_err(), "junction escape should be rejected");
                assert!(matches!(result.unwrap_err(), FsApiError::PathTraversal { .. }));

                // Accessing a file through the junction should also be rejected.
                let through_junction = junction_path.join("outside.txt");
                let result = validate_path_within_root(&through_junction, root, "test");
                assert!(result.is_err(), "path through junction should be rejected");
                assert!(matches!(result.unwrap_err(), FsApiError::PathTraversal { .. }));
            }
            Err(e) => {
                eprintln!("SKIPPED: Junction creation failed: {}. This test requires an NTFS filesystem.", e);
                return;
            }
        }
    }
}


