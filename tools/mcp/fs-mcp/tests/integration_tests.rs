//! Transport-layer integration tests for fs-mcp.
//!
//! Domain behavior is covered by fs-api. These tests verify:
//! - JSON schema marshaling and argument deserialization
//! - Error translation to McpError
//! - Tool registration and capability advertisement

use fs_mcp::server::{
    CopyFileInput,
    DeleteDirInput,
    DeleteFileInput,
    FsServer,
    ListDirInput,
    MoveFileInput,
    RenameFileInput,
    StatInput,
};
use rmcp::{
    ServerHandler,
    handler::server::wrapper::Parameters,
    model::{
        CallToolResult,
        RawContent,
    },
};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
};
use tempfile::TempDir;

/// Extract JSON content from an MCP CallToolResult.
fn extract_json(result: CallToolResult) -> Value {
    let text = result
        .content
        .iter()
        .find_map(|content| {
            if let RawContent::Text(text) = &content.raw {
                Some(text.text.clone())
            } else {
                None
            }
        })
        .expect("text content");
    serde_json::from_str(&text).expect("parse json")
}

#[tokio::test]
async fn test_list_dir_basic() {
    let tmp = TempDir::new().expect("tempdir");
    fs::write(tmp.path().join("file1.txt"), "content1").unwrap();
    fs::write(tmp.path().join("file2.txt"), "content2").unwrap();

    let server = FsServer::new();
    let input = ListDirInput {
        path: tmp.path().to_path_buf(),
        depth_limit: None,
        entry_limit: None,
        include_globs: vec![],
        exclude_globs: vec![],
        honor_ignore: false,
    };

    let result = server.fs_list_dir(Parameters(input)).await.unwrap();
    let json = extract_json(result);

    assert!(!json["truncated"].as_bool().unwrap());
    assert!(json["entries"].as_array().unwrap().len() >= 2);
}

#[tokio::test]
async fn test_list_dir_with_limit() {
    let tmp = TempDir::new().expect("tempdir");
    for i in 1..=100 {
        fs::write(tmp.path().join(format!("file{}.txt", i)), "content")
            .unwrap();
    }

    let server = FsServer::new();
    let input = ListDirInput {
        path: tmp.path().to_path_buf(),
        depth_limit: None,
        entry_limit: Some(10),
        include_globs: vec![],
        exclude_globs: vec![],
        honor_ignore: false,
    };

    let result = server.fs_list_dir(Parameters(input)).await.unwrap();
    let json = extract_json(result);

    assert!(json["truncated"].as_bool().unwrap());
    assert_eq!(json["entries"].as_array().unwrap().len(), 10);
}

#[tokio::test]
async fn test_stat_existing_file() {
    let tmp = TempDir::new().expect("tempdir");
    let file_path = tmp.path().join("test.txt");
    fs::write(&file_path, "content").unwrap();

    let server = FsServer::new();
    let input = StatInput {
        path: file_path.clone(),
    };

    let result = server.fs_stat(Parameters(input)).await.unwrap();
    let json = extract_json(result);

    assert_eq!(json["exists"], true);
    assert_eq!(json["kind"], "file");
}

#[tokio::test]
async fn test_stat_missing_file() {
    let tmp = TempDir::new().expect("tempdir");
    let file_path = tmp.path().join("missing.txt");

    let server = FsServer::new();
    let input = StatInput {
        path: file_path.clone(),
    };

    let result = server.fs_stat(Parameters(input)).await.unwrap();
    let json = extract_json(result);

    assert!(!json["exists"].as_bool().unwrap());
}

#[tokio::test]
async fn test_move_file_conflict() {
    let tmp = TempDir::new().expect("tempdir");
    let from = tmp.path().join("source.txt");
    let to = tmp.path().join("dest.txt");
    fs::write(&from, "source content").unwrap();
    fs::write(&to, "dest content").unwrap();

    let server = FsServer::new();
    let input = MoveFileInput {
        from: from.clone(),
        to: to.clone(),
        overwrite: false,
        root: Some(tmp.path().to_path_buf()),
    };

    let result = server.fs_move_file(Parameters(input)).await.unwrap();
    let json = extract_json(result);

    let conflicts = json["conflicts"].as_array().unwrap();
    assert!(!conflicts.is_empty());
}

#[tokio::test]
async fn test_move_file_with_overwrite() {
    let tmp = TempDir::new().expect("tempdir");
    let from = tmp.path().join("source.txt");
    let to = tmp.path().join("dest.txt");
    fs::write(&from, "source content").unwrap();
    fs::write(&to, "dest content").unwrap();

    let server = FsServer::new();
    let input = MoveFileInput {
        from: from.clone(),
        to: to.clone(),
        overwrite: true,
        root: Some(tmp.path().to_path_buf()),
    };

    let result = server.fs_move_file(Parameters(input)).await.unwrap();
    let json = extract_json(result);

    let conflicts = json["conflicts"].as_array().unwrap();
    assert!(conflicts.is_empty());
    assert!(!from.exists());
    assert!(to.exists());
}

#[tokio::test]
async fn test_copy_file() {
    let tmp = TempDir::new().expect("tempdir");
    let from = tmp.path().join("source.txt");
    let to = tmp.path().join("dest.txt");
    fs::write(&from, "source content").unwrap();

    let server = FsServer::new();
    let input = CopyFileInput {
        from: from.clone(),
        to: to.clone(),
        overwrite: false,
        root: Some(tmp.path().to_path_buf()),
    };

    let result = server.fs_copy_file(Parameters(input)).await.unwrap();
    let json = extract_json(result);

    let conflicts = json["conflicts"].as_array().unwrap();
    assert!(conflicts.is_empty());
    assert!(from.exists());
    assert!(to.exists());
}

#[tokio::test]
async fn test_delete_file() {
    let tmp = TempDir::new().expect("tempdir");
    let file_path = tmp.path().join("test.txt");
    fs::write(&file_path, "content").unwrap();

    let server = FsServer::new();
    let input = DeleteFileInput {
        path: file_path.clone(),
        root: Some(tmp.path().to_path_buf()),
    };

    let result = server.fs_delete_file(Parameters(input)).await.unwrap();
    let json = extract_json(result);

    let conflicts = json["conflicts"].as_array().unwrap();
    assert!(conflicts.is_empty());
    assert!(!file_path.exists());
}

#[tokio::test]
async fn test_delete_dir_recursive() {
    let tmp = TempDir::new().expect("tempdir");
    let dir_path = tmp.path().join("subdir");
    fs::create_dir(&dir_path).unwrap();
    fs::write(dir_path.join("file.txt"), "content").unwrap();

    let server = FsServer::new();
    let input = DeleteDirInput {
        path: dir_path.clone(),
        recursive: true,
        root: Some(tmp.path().to_path_buf()),
    };

    let result = server.fs_delete_dir(Parameters(input)).await.unwrap();
    let json = extract_json(result);

    let conflicts = json["conflicts"].as_array().unwrap();
    assert!(conflicts.is_empty());
    assert!(!dir_path.exists());
}

// ── Security Validation Tests ───────────────────────────────────────────────

#[tokio::test]
#[cfg_attr(windows, ignore = "Requires elevated privileges on Windows")]
async fn test_security_symlink_escape_via_move() {
    let root_dir = TempDir::new().expect("tempdir");
    let outside_dir = TempDir::new().expect("tempdir");

    let safe_file = root_dir.path().join("safe.txt");
    fs::write(&safe_file, "safe content").unwrap();

    // Create symlink pointing outside root
    let symlink_path = root_dir.path().join("escape_link");
    #[cfg(unix)]
    std::os::unix::fs::symlink(outside_dir.path(), &symlink_path).unwrap();
    #[cfg(windows)]
    std::os::windows::fs::symlink_dir(outside_dir.path(), &symlink_path)
        .unwrap();

    let escape_dest = symlink_path.join("escaped.txt");

    let server = FsServer::new();
    let input = MoveFileInput {
        from: safe_file.clone(),
        to: escape_dest.clone(),
        overwrite: false,
        root: Some(root_dir.path().to_path_buf()),
    };

    // Should fail - symlink escape detected
    let result = server.fs_move_file(Parameters(input)).await;
    assert!(result.is_err(), "Expected symlink escape to be rejected");
}

#[tokio::test]
async fn test_security_parent_directory_escape_via_delete() {
    let root_dir = TempDir::new().expect("tempdir");
    let nested = root_dir.path().join("nested");
    fs::create_dir(&nested).unwrap();

    // Try to delete using ../ to escape root
    let escape_path = nested.join("..").join("..").join("etc").join("passwd");

    let server = FsServer::new();
    let input = DeleteFileInput {
        path: escape_path.clone(),
        root: Some(root_dir.path().to_path_buf()),
    };

    // Should fail - parent directory escape detected
    let result = server.fs_delete_file(Parameters(input)).await;
    assert!(
        result.is_err(),
        "Expected parent directory escape to be rejected"
    );
}

#[tokio::test]
async fn test_security_copy_with_valid_root() {
    let root_dir = TempDir::new().expect("tempdir");
    let src = root_dir.path().join("src.txt");
    let dst = root_dir.path().join("subdir").join("dst.txt");

    fs::write(&src, "content").unwrap();
    fs::create_dir(root_dir.path().join("subdir")).unwrap();

    let server = FsServer::new();
    let input = CopyFileInput {
        from: src.clone(),
        to: dst.clone(),
        overwrite: false,
        root: Some(root_dir.path().to_path_buf()),
    };

    let result = server.fs_copy_file(Parameters(input)).await.unwrap();
    let json = extract_json(result);

    let conflicts = json["conflicts"].as_array().unwrap();
    assert!(conflicts.is_empty());
    assert!(src.exists());
    assert!(dst.exists());
}

#[tokio::test]
async fn test_security_root_defaults_to_cwd() {
    // This test verifies that root=None defaults to CWD, triggering validation.
    // We create a temp directory outside CWD and expect it to be rejected.
    let root_dir = TempDir::new().expect("tempdir");
    let src = root_dir.path().join("src.txt");
    let dst = root_dir.path().join("dst.txt");

    fs::write(&src, "content").unwrap();

    let server = FsServer::new();
    // root: None should default to CWD in the server handler
    let input = MoveFileInput {
        from: src.clone(),
        to: dst.clone(),
        overwrite: false,
        root: None, // Should default to CWD
    };

    // Since temp dir is likely outside CWD, this should fail validation
    let result = server.fs_move_file(Parameters(input)).await;

    // Expect failure due to path traversal (temp dir outside CWD)
    assert!(
        result.is_err(),
        "Expected path traversal error when temp dir is outside CWD"
    );
}
