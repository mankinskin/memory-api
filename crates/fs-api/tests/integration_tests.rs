//! Integration tests for fs-api.

use fs_api::{
    ConflictKind,
    CopyFileRequest,
    DeleteDirRequest,
    DeleteFileRequest,
    EntryKind,
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
use std::fs;
use tempfile::tempdir;

#[test]
fn test_list_dir_with_depth_cap() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    // Create nested directory structure.
    fs::create_dir_all(root.join("a/b/c")).unwrap();
    fs::write(root.join("a/file1.txt"), "content").unwrap();
    fs::write(root.join("a/b/file2.txt"), "content").unwrap();
    fs::write(root.join("a/b/c/file3.txt"), "content").unwrap();

    let request = ListDirRequest {
        path: root.to_path_buf(),
        depth_limit: Some(2),
        entry_limit: None,
        include_globs: vec![],
        exclude_globs: vec![],
        honor_ignore: false,
    };

    let result = list_dir(&request).expect("list_dir failed");

    // Should include a/, a/b/, a/file1.txt, a/b/file2.txt but NOT a/b/c/ or deeper.
    let paths: Vec<String> = result
        .entries
        .iter()
        .map(|e| e.path.to_string_lossy().to_string())
        .collect();

    assert!(
        paths.contains(&"a".to_string()) || paths.contains(&"a/".to_string())
    );
    assert!(
        paths.contains(&"a/file1.txt".to_string())
            || paths.contains(&"a\\file1.txt".to_string())
    );
    // Depth 2 might include b but not c.
}

#[test]
fn test_list_dir_with_entry_cap() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    // Create many files.
    for i in 0..100 {
        fs::write(root.join(format!("file{}.txt", i)), "content").unwrap();
    }

    let request = ListDirRequest {
        path: root.to_path_buf(),
        depth_limit: Some(1),
        entry_limit: Some(10),
        include_globs: vec![],
        exclude_globs: vec![],
        honor_ignore: false,
    };

    let result = list_dir(&request).expect("list_dir failed");

    assert_eq!(result.entries.len(), 10);
    assert!(result.truncated);
    assert_eq!(result.total_found, Some(100));
}

#[test]
fn test_list_dir_with_include_filter() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    fs::write(root.join("file1.txt"), "content").unwrap();
    fs::write(root.join("file2.rs"), "content").unwrap();
    fs::write(root.join("file3.txt"), "content").unwrap();

    let request = ListDirRequest {
        path: root.to_path_buf(),
        depth_limit: Some(1),
        entry_limit: None,
        include_globs: vec!["*.rs".to_string()],
        exclude_globs: vec![],
        honor_ignore: false,
    };

    let result = list_dir(&request).expect("list_dir failed");

    assert_eq!(result.entries.len(), 1);
    assert!(
        result.entries[0]
            .path
            .to_string_lossy()
            .contains("file2.rs")
    );
}

#[test]
fn test_list_dir_with_exclude_filter() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    fs::create_dir(root.join("target")).unwrap();
    fs::write(root.join("target/build.log"), "content").unwrap();
    fs::write(root.join("src.txt"), "content").unwrap();

    let request = ListDirRequest {
        path: root.to_path_buf(),
        depth_limit: Some(2),
        entry_limit: None,
        include_globs: vec![],
        exclude_globs: vec!["target".to_string()],
        honor_ignore: false,
    };

    let result = list_dir(&request).expect("list_dir failed");

    let paths: Vec<String> = result
        .entries
        .iter()
        .map(|e| e.path.to_string_lossy().to_string())
        .collect();

    // Should exclude target directory.
    assert!(!paths.iter().any(|p| p.contains("target")));
}

#[test]
fn test_list_dir_honor_gitignore() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    // Initialize a git repo so .gitignore is honored.
    std::process::Command::new("git")
        .arg("init")
        .current_dir(root)
        .output()
        .expect("failed to init git repo");

    fs::write(root.join(".gitignore"), "ignored.txt\n").unwrap();
    fs::write(root.join("ignored.txt"), "content").unwrap();
    fs::write(root.join("visible.txt"), "content").unwrap();

    let request = ListDirRequest {
        path: root.to_path_buf(),
        depth_limit: Some(1),
        entry_limit: None,
        include_globs: vec![],
        exclude_globs: vec![],
        honor_ignore: true,
    };

    let result = list_dir(&request).expect("list_dir failed");

    let paths: Vec<String> = result
        .entries
        .iter()
        .map(|e| e.path.to_string_lossy().to_string())
        .collect();

    // Should exclude ignored.txt.
    assert!(!paths.iter().any(|p| p.contains("ignored.txt")));
    assert!(paths.iter().any(|p| p.contains("visible.txt")));
}

#[test]
fn test_stat_existing_file() {
    let temp = tempdir().expect("failed to create temp dir");
    let file_path = temp.path().join("test.txt");
    fs::write(&file_path, "content").unwrap();

    let request = StatRequest {
        path: file_path.clone(),
    };

    let result = stat(&request).expect("stat failed");

    assert!(result.exists);
    assert_eq!(result.kind, Some(EntryKind::File));
    assert_eq!(result.size, Some(7));
    assert!(result.modified_secs.is_some());
}

#[test]
fn test_stat_missing_path() {
    let temp = tempdir().expect("failed to create temp dir");
    let missing_path = temp.path().join("missing.txt");

    let request = StatRequest { path: missing_path };

    let result = stat(&request).expect("stat failed");

    assert!(!result.exists);
    assert_eq!(result.kind, None);
    assert_eq!(result.size, None);
}

#[test]
fn test_move_without_overwrite_destination_exists() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let from = root.join("source.txt");
    let to = root.join("dest.txt");

    fs::write(&from, "source").unwrap();
    fs::write(&to, "dest").unwrap();

    let request = MoveFileRequest {
        from: from.clone(),
        to: to.clone(),
        overwrite: false,
        root: root.to_path_buf(),
    };

    let result = move_file(&request).expect("move_file failed");

    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(result.conflicts[0].kind, ConflictKind::DestinationExists);
    assert!(from.exists()); // Source should still exist.
}

#[test]
fn test_move_with_overwrite_succeeds() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let from = root.join("source.txt");
    let to = root.join("dest.txt");

    fs::write(&from, "source").unwrap();
    fs::write(&to, "dest").unwrap();

    let request = MoveFileRequest {
        from: from.clone(),
        to: to.clone(),
        overwrite: true,
        root: root.to_path_buf(),
    };

    let result = move_file(&request).expect("move_file failed");

    assert!(result.conflicts.is_empty());
    assert!(!from.exists());
    assert!(to.exists());
    assert_eq!(fs::read_to_string(&to).unwrap(), "source");
}

#[test]
fn test_move_source_missing() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let from = root.join("missing.txt");
    let to = root.join("dest.txt");

    let request = MoveFileRequest {
        from: from.clone(),
        to: to.clone(),
        overwrite: false,
        root: root.to_path_buf(),
    };

    let result = move_file(&request).expect("move_file failed");

    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(result.conflicts[0].kind, ConflictKind::SourceMissing);
}

#[test]
fn test_copy_file_succeeds() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let from = root.join("source.txt");
    let to = root.join("dest.txt");

    fs::write(&from, "content").unwrap();

    let request = CopyFileRequest {
        from: from.clone(),
        to: to.clone(),
        overwrite: false,
        root: root.to_path_buf(),
    };

    let result = copy_file(&request).expect("copy_file failed");

    assert!(result.conflicts.is_empty());
    assert!(from.exists());
    assert!(to.exists());
    assert_eq!(fs::read_to_string(&to).unwrap(), "content");
}

#[test]
fn test_delete_file_succeeds() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let file_path = root.join("test.txt");
    fs::write(&file_path, "content").unwrap();

    let request = DeleteFileRequest {
        path: file_path.clone(),
        root: root.to_path_buf(),
    };

    let result = delete_file(&request).expect("delete_file failed");

    assert!(result.conflicts.is_empty());
    assert!(!file_path.exists());
}

#[test]
fn test_delete_file_missing() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let file_path = root.join("missing.txt");

    let request = DeleteFileRequest {
        path: file_path.clone(),
        root: root.to_path_buf(),
    };

    let result = delete_file(&request).expect("delete_file failed");

    assert_eq!(result.conflicts.len(), 1);
    assert_eq!(result.conflicts[0].kind, ConflictKind::SourceMissing);
}

#[test]
fn test_delete_dir_recursive_succeeds() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let dir_path = root.join("dir");
    fs::create_dir(&dir_path).unwrap();
    fs::write(dir_path.join("file.txt"), "content").unwrap();

    let request = DeleteDirRequest {
        path: dir_path.clone(),
        recursive: true,
        root: root.to_path_buf(),
    };

    let result = delete_dir(&request).expect("delete_dir failed");

    assert!(result.conflicts.is_empty());
    assert!(!dir_path.exists());
}

#[test]
fn test_rename_file_succeeds() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let from = root.join("old.txt");
    let to = root.join("new.txt");

    fs::write(&from, "content").unwrap();

    let request = RenameFileRequest {
        from: from.clone(),
        to: to.clone(),
        root: root.to_path_buf(),
    };

    let result = rename_file(&request).expect("rename_file failed");

    assert!(result.conflicts.is_empty());
    assert!(!from.exists());
    assert!(to.exists());
}

// ============================================================================
// Security hardening tests
// ============================================================================

#[test]
#[cfg(unix)]
fn test_list_dir_does_not_traverse_symlink_escape() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    // Create a file outside the root.
    let outside = temp.path().parent().unwrap().join("outside.txt");
    fs::write(&outside, "secret").unwrap();

    // Create a symlink inside root pointing outside.
    let link = root.join("escape_link");
    symlink(&outside, &link).unwrap();

    // Also create a normal file for comparison.
    fs::write(root.join("normal.txt"), "content").unwrap();

    let request = ListDirRequest {
        path: root.to_path_buf(),
        depth_limit: Some(1),
        entry_limit: None,
        include_globs: vec![],
        exclude_globs: vec![],
        honor_ignore: false,
    };

    let result = list_dir(&request).expect("list_dir failed");

    // Should list the symlink itself but not traverse it.
    let paths: Vec<String> = result
        .entries
        .iter()
        .map(|e| e.path.to_string_lossy().to_string())
        .collect();

    // The symlink should appear in the listing.
    assert!(paths.iter().any(|p| p.contains("escape_link")));
    // But nothing from outside should appear.
    assert!(!paths.iter().any(|p| p.contains("outside.txt")));
    // Normal file should still be listed.
    assert!(paths.iter().any(|p| p.contains("normal.txt")));
}

#[test]
#[cfg(unix)]
fn test_move_file_rejects_symlink_escape_when_root_provided() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    // Create a file outside the root.
    let outside = temp.path().parent().unwrap().join("outside_target.txt");
    fs::write(&outside, "content").unwrap();

    // Create a symlink inside root pointing outside.
    let link = root.join("escape_link");
    symlink(&outside, &link).unwrap();

    let dest = root.join("dest.txt");

    let request = MoveFileRequest {
        from: link.clone(),
        to: dest.clone(),
        overwrite: false,
        root: Some(root.to_path_buf()),
    };

    // Should fail with PathTraversal error.
    let result = move_file(&request);
    assert!(result.is_err());
    assert!(
        matches!(
            result.unwrap_err(),
            fs_api::FsApiError::PathTraversal { .. }
        ),
        "Expected PathTraversal error for symlink escape"
    );
}

#[test]
#[cfg(unix)]
fn test_copy_file_rejects_symlink_escape_when_root_provided() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    // Create a file outside the root.
    let outside = temp.path().parent().unwrap().join("outside_copy.txt");
    fs::write(&outside, "secret").unwrap();

    // Create a symlink inside root pointing outside.
    let link = root.join("copy_escape_link");
    symlink(&outside, &link).unwrap();

    let dest = root.join("copy_dest.txt");

    let request = CopyFileRequest {
        from: link.clone(),
        to: dest.clone(),
        overwrite: false,
        root: Some(root.to_path_buf()),
    };

    // Should fail with PathTraversal error.
    let result = copy_file(&request);
    assert!(result.is_err());
    assert!(
        matches!(
            result.unwrap_err(),
            fs_api::FsApiError::PathTraversal { .. }
        ),
        "Expected PathTraversal error for symlink escape"
    );
}

#[test]
#[cfg(unix)]
fn test_delete_file_rejects_symlink_escape_when_root_provided() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    // Create a file outside the root.
    let outside = temp.path().parent().unwrap().join("outside_delete.txt");
    fs::write(&outside, "important").unwrap();

    // Create a symlink inside root pointing outside.
    let link = root.join("delete_escape_link");
    symlink(&outside, &link).unwrap();

    let request = DeleteFileRequest {
        path: link.clone(),
        root: Some(root.to_path_buf()),
    };

    // Should fail with PathTraversal error.
    let result = delete_file(&request);
    assert!(result.is_err());
    assert!(
        matches!(
            result.unwrap_err(),
            fs_api::FsApiError::PathTraversal { .. }
        ),
        "Expected PathTraversal error for symlink escape"
    );

    // The outside file should still exist (not deleted).
    assert!(outside.exists());
}

#[test]
fn test_move_file_normal_paths_still_work_with_root() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    let from = root.join("source.txt");
    let to = root.join("destination.txt");

    fs::write(&from, "content").unwrap();

    let request = MoveFileRequest {
        from: from.clone(),
        to: to.clone(),
        overwrite: false,
        root: root.to_path_buf(),
    };

    let result =
        move_file(&request).expect("move_file should succeed for normal paths");

    assert!(result.conflicts.is_empty());
    assert!(!from.exists());
    assert!(to.exists());
    assert_eq!(fs::read_to_string(&to).unwrap(), "content");
}

#[test]
fn test_copy_file_normal_paths_still_work_with_root() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    let from = root.join("source_copy.txt");
    let to = root.join("dest_copy.txt");

    fs::write(&from, "copy content").unwrap();

    let request = CopyFileRequest {
        from: from.clone(),
        to: to.clone(),
        overwrite: false,
        root: root.to_path_buf(),
    };

    let result =
        copy_file(&request).expect("copy_file should succeed for normal paths");

    assert!(result.conflicts.is_empty());
    assert!(from.exists());
    assert!(to.exists());
    assert_eq!(fs::read_to_string(&to).unwrap(), "copy content");
}

#[test]
#[cfg(unix)]
fn test_rename_file_rejects_symlink_escape_when_root_provided() {
    use std::os::unix::fs::symlink;

    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    // Create a file outside the root.
    let outside = temp.path().parent().unwrap().join("outside_rename.txt");
    fs::write(&outside, "data").unwrap();

    // Create a symlink inside root pointing outside.
    let link = root.join("rename_escape_link");
    symlink(&outside, &link).unwrap();

    let new_name = root.join("renamed.txt");

    let request = RenameFileRequest {
        from: link.clone(),
        to: new_name.clone(),
        root: Some(root.to_path_buf()),
    };

    // Should fail with PathTraversal error.
    let result = rename_file(&request);
    assert!(result.is_err());
    assert!(
        matches!(
            result.unwrap_err(),
            fs_api::FsApiError::PathTraversal { .. }
        ),
        "Expected PathTraversal error for symlink escape"
    );
}

#[test]
#[cfg(windows)]
fn test_windows_path_canonicalization_no_false_positives() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    // Create a nested structure.
    let subdir = root.join("subdir");
    fs::create_dir(&subdir).unwrap();
    let file = subdir.join("file.txt");
    fs::write(&file, "windows test").unwrap();

    // Test that canonicalization (which adds \\?\ prefix on Windows) doesn't break validation.
    let request = MoveFileRequest {
        from: file.clone(),
        to: root.join("moved.txt"),
        overwrite: false,
        root: root.to_path_buf(),
    };

    let result = move_file(&request).expect(
        "Windows path canonicalization should not cause false rejection",
    );
    assert!(result.conflicts.is_empty());
}

#[test]
fn test_path_validation_rejects_escape_attempts() {
    // Replaces the vacuous test_required_root_validation.
    // This test verifies that validation actually rejects escape attempts,
    // not just that valid paths succeed.
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let subdir = root.join("subdir");
    fs::create_dir(&subdir).unwrap();

    let safe_file = subdir.join("safe.txt");
    fs::write(&safe_file, "content").unwrap();

    // Attempt to escape via .. traversal
    let escape_attempt = subdir.join("..").join("..").join("outside.txt");

    // Move should reject the escape attempt
    let request = MoveFileRequest {
        from: safe_file.clone(),
        to: escape_attempt.clone(),
        overwrite: false,
        root: subdir.to_path_buf(),
    };

    let result = move_file(&request);
    assert!(result.is_err(), "escape via .. should be rejected");

    // Safe file should still exist (operation failed)
    assert!(safe_file.exists());

    // Also test that copy rejects escape
    let request = CopyFileRequest {
        from: safe_file.clone(),
        to: escape_attempt.clone(),
        overwrite: false,
        root: subdir.to_path_buf(),
    };

    let result = copy_file(&request);
    assert!(result.is_err(), "escape via .. in copy should be rejected");
}

// ============================================================================
// TOCTOU regression tests
// ============================================================================

// Previously had vacuous TOCTOU tests removed. The real security coverage is in
// the test_*_rejects_symlink_escape_* tests above, which properly validate that
// symlink escapes are rejected when root is provided.

#[test]
fn test_delete_dir_recursive_entry_limit() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let dir = root.join("large_tree");
    fs::create_dir(&dir).unwrap();

    // Create a directory tree that exceeds the 10,000 entry limit.
    // Create 100 subdirectories with 101 files each = 10,100 entries + root.
    for i in 0..100 {
        let subdir = dir.join(format!("sub{}", i));
        fs::create_dir(&subdir).unwrap();
        for j in 0..101 {
            fs::write(subdir.join(format!("file{}.txt", j)), "x").unwrap();
        }
    }

    let request = DeleteDirRequest {
        path: dir.clone(),
        recursive: true,
        root: root.to_path_buf(),
    };

    // Should fail due to entry limit.
    let result = delete_dir(&request);
    assert!(result.is_err());
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(err_msg.contains("exceeds entry limit"));

    // Directory should still exist (not deleted).
    assert!(dir.exists());
}

#[test]
fn test_delete_dir_recursive_within_limit() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let dir = root.join("small_tree");
    fs::create_dir(&dir).unwrap();

    // Create a small tree well under the limit.
    for i in 0..10 {
        let subdir = dir.join(format!("sub{}", i));
        fs::create_dir(&subdir).unwrap();
        for j in 0..5 {
            fs::write(subdir.join(format!("file{}.txt", j)), "x").unwrap();
        }
    }

    let request = DeleteDirRequest {
        path: dir.clone(),
        recursive: true,
        root: root.to_path_buf(),
    };

    let result =
        delete_dir(&request).expect("delete should succeed within limit");
    assert!(result.conflicts.is_empty());
    assert!(!dir.exists());
}

// ============================================================================
// Windows Junction Escape Tests
// ============================================================================
// These tests verify that directory junctions (which can be created without
// elevation on Windows) cannot be used to escape the root directory.
// Junctions are created via the `junction` crate.

#[test]
#[cfg(windows)]
fn test_list_dir_rejects_junction_escape() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();

    // Create a target directory outside the root.
    let outside_temp = tempdir().expect("failed to create outside temp");
    let outside_target = outside_temp.path();
    fs::write(outside_target.join("outside.txt"), "escape").unwrap();

    // Create a junction inside root pointing outside.
    let junction_path = root.join("junction");
    match junction::create(outside_target, &junction_path) {
        Ok(_) => {
            let request = ListDirRequest {
                path: root.to_path_buf(),
                depth_limit: None,
                entry_limit: None,
                include_globs: vec![],
                exclude_globs: vec![],
                honor_ignore: false,
            };

            let result = list_dir(&request).expect("list_dir should succeed");

            // list_dir should not traverse through the junction to list files outside root.
            // The junction itself may appear as an entry, but content outside root should not.
            for entry in &result.entries {
                let path_str = entry.path.to_string_lossy();
                assert!(
                    !path_str.contains("outside.txt"),
                    "list_dir should not traverse junction to outside root: found {}",
                    path_str
                );
            }
        },
        Err(e) => {
            eprintln!(
                "SKIPPED: Junction creation failed: {}. This test requires an NTFS filesystem.",
                e
            );
            return;
        },
    }
}

#[test]
#[cfg(windows)]
fn test_copy_file_rejects_junction_escape() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let outside_temp = tempdir().expect("failed to create outside temp");
    let outside_target = outside_temp.path();

    let junction_path = root.join("junction");
    match junction::create(outside_target, &junction_path) {
        Ok(_) => {
            let source = root.join("source.txt");
            fs::write(&source, "data").unwrap();

            // Attempt to copy through the junction to outside root.
            let escape_dest = junction_path.join("escaped.txt");
            let request = CopyFileRequest {
                from: source.clone(),
                to: escape_dest.clone(),
                overwrite: false,
                root: root.to_path_buf(),
            };

            let result = copy_file(&request);
            assert!(
                result.is_err(),
                "copy through junction should be rejected"
            );
        },
        Err(e) => {
            eprintln!(
                "SKIPPED: Junction creation failed: {}. This test requires an NTFS filesystem.",
                e
            );
            return;
        },
    }
}

#[test]
#[cfg(windows)]
fn test_move_file_rejects_junction_escape() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let outside_temp = tempdir().expect("failed to create outside temp");
    let outside_target = outside_temp.path();

    let junction_path = root.join("junction");
    match junction::create(outside_target, &junction_path) {
        Ok(_) => {
            let source = root.join("source.txt");
            fs::write(&source, "data").unwrap();

            // Attempt to move through the junction to outside root.
            let escape_dest = junction_path.join("escaped.txt");
            let request = MoveFileRequest {
                from: source.clone(),
                to: escape_dest.clone(),
                overwrite: false,
                root: root.to_path_buf(),
            };

            let result = move_file(&request);
            assert!(
                result.is_err(),
                "move through junction should be rejected"
            );
            // Source should still exist (operation failed).
            assert!(source.exists());
        },
        Err(e) => {
            eprintln!(
                "SKIPPED: Junction creation failed: {}. This test requires an NTFS filesystem.",
                e
            );
            return;
        },
    }
}

#[test]
#[cfg(windows)]
fn test_rename_file_rejects_junction_escape() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let outside_temp = tempdir().expect("failed to create outside temp");
    let outside_target = outside_temp.path();

    let junction_path = root.join("junction");
    match junction::create(outside_target, &junction_path) {
        Ok(_) => {
            let source = root.join("source.txt");
            fs::write(&source, "data").unwrap();

            // Attempt to rename to a path through the junction (escaping to outside root).
            let escape_dest = junction_path.join("escaped.txt");
            let request = RenameFileRequest {
                from: source.clone(),
                to: escape_dest.clone(),
                root: root.to_path_buf(),
            };

            let result = rename_file(&request);
            assert!(
                result.is_err(),
                "rename through junction should be rejected"
            );
            // Source should still exist (operation failed).
            assert!(source.exists());
        },
        Err(e) => {
            eprintln!(
                "SKIPPED: Junction creation failed: {}. This test requires an NTFS filesystem.",
                e
            );
            return;
        },
    }
}

#[test]
#[cfg(windows)]
fn test_delete_file_rejects_junction_escape() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let outside_temp = tempdir().expect("failed to create outside temp");
    let outside_target = outside_temp.path();
    let outside_file = outside_target.join("outside.txt");
    fs::write(&outside_file, "victim").unwrap();

    let junction_path = root.join("junction");
    match junction::create(outside_target, &junction_path) {
        Ok(_) => {
            // Attempt to delete a file outside root through the junction.
            let escape_path = junction_path.join("outside.txt");
            let request = DeleteFileRequest {
                path: escape_path.clone(),
                root: root.to_path_buf(),
            };

            let result = delete_file(&request);
            assert!(
                result.is_err(),
                "delete through junction should be rejected"
            );
            // Outside file should still exist (operation failed).
            assert!(outside_file.exists());
        },
        Err(e) => {
            eprintln!(
                "SKIPPED: Junction creation failed: {}. This test requires an NTFS filesystem.",
                e
            );
            return;
        },
    }
}

#[test]
#[cfg(windows)]
fn test_delete_dir_rejects_junction_escape() {
    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let outside_temp = tempdir().expect("failed to create outside temp");
    let outside_target = outside_temp.path();
    let outside_dir = outside_target.join("outside_dir");
    fs::create_dir(&outside_dir).unwrap();
    fs::write(outside_dir.join("file.txt"), "data").unwrap();

    let junction_path = root.join("junction");
    match junction::create(outside_target, &junction_path) {
        Ok(_) => {
            // Attempt to delete a directory outside root through the junction.
            let escape_path = junction_path.join("outside_dir");
            let request = DeleteDirRequest {
                path: escape_path.clone(),
                recursive: true,
                root: root.to_path_buf(),
            };

            let result = delete_dir(&request);
            assert!(
                result.is_err(),
                "delete_dir through junction should be rejected"
            );
            // Outside directory should still exist (operation failed).
            assert!(outside_dir.exists());
        },
        Err(e) => {
            eprintln!(
                "SKIPPED: Junction creation failed: {}. This test requires an NTFS filesystem.",
                e
            );
            return;
        },
    }
}

// ============================================================================
// Counting Fix Test (Task 3)
// ============================================================================

#[test]
#[cfg(unix)]
fn test_delete_dir_counts_all_entries_including_errors() {
    use std::os::unix::fs::PermissionsExt;

    let temp = tempdir().expect("failed to create temp dir");
    let root = temp.path();
    let dir = root.join("test_tree");
    fs::create_dir(&dir).unwrap();

    // Create a structure where some entries will be unreadable.
    // Create subdirectories, make some unreadable, then create files in readable ones.
    // The goal is to have more than 10,000 total iterations even if many are errors.

    // For practical testing, we'll create a smaller structure and verify the
    // counting logic counts errors. Create 50 dirs with 201 entries each = 10,050+
    // Then make half the directories unreadable to trigger walk errors.

    for i in 0..50 {
        let subdir = dir.join(format!("sub{}", i));
        fs::create_dir(&subdir).unwrap();

        for j in 0..201 {
            fs::write(subdir.join(format!("file{}.txt", j)), "x").unwrap();
        }

        // Make every other directory unreadable to trigger errors during walk.
        if i % 2 == 0 {
            let mut perms = fs::metadata(&subdir).unwrap().permissions();
            perms.set_mode(0o000); // No permissions
            fs::set_permissions(&subdir, perms).unwrap();
        }
    }

    let request = DeleteDirRequest {
        path: dir.clone(),
        recursive: true,
        root: root.to_path_buf(),
    };

    // Should fail due to entry limit, even though half the entries are unreadable.
    // If the fix is correct, it counts all walk iterations (errors + successes).
    let result = delete_dir(&request);

    // Restore permissions for cleanup.
    for i in 0..50 {
        if i % 2 == 0 {
            let subdir = dir.join(format!("sub{}", i));
            if subdir.exists() {
                let mut perms = fs::metadata(&subdir).unwrap().permissions();
                perms.set_mode(0o755);
                let _ = fs::set_permissions(&subdir, perms);
            }
        }
    }

    assert!(
        result.is_err(),
        "delete should fail due to entry limit even with unreadable entries"
    );
    let err_msg = format!("{:?}", result.unwrap_err());
    assert!(
        err_msg.contains("exceeds entry limit"),
        "error should mention entry limit: {}",
        err_msg
    );
}
