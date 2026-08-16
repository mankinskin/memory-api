//! Transport-layer integration tests for compact-terminal-mcp.
//!
//! Domain behavior is covered by compact-terminal-api. These tests verify:
//! - JSON schema marshaling and argument deserialization
//! - Error translation to McpError
//! - Tool registration and capability advertisement

use compact_terminal_mcp::server::{
    CompactTerminalServer,
    ReadSpillInput,
    RunInput,
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
use std::path::PathBuf;
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

/// Extract plain text content from an MCP CallToolResult.
fn extract_text(result: CallToolResult) -> String {
    result
        .content
        .iter()
        .find_map(|content| {
            if let RawContent::Text(text) = &content.raw {
                Some(text.text.clone())
            } else {
                None
            }
        })
        .expect("text content")
}

#[tokio::test]
async fn run_tool_inline_output_below_limit() {
    let tmp = TempDir::new().expect("tempdir");
    let server = CompactTerminalServer::new(Some(tmp.path().to_path_buf()));

    let input = RunInput {
        command: "echo hello".to_string(),
        cwd: None,
        inline_limit: Some(4096),
        timeout_secs: Some(5),
    };

    let result = server.run(Parameters(input)).await.expect("run failed");
    let json = extract_json(result);

    assert_eq!(json["exit_code"], 0);
    assert!(json["stdout"].as_str().unwrap().contains("hello"));
    assert!(
        json.get("spill_file").is_none(),
        "should be inline, not spilled"
    );
}

#[tokio::test]
async fn run_tool_spilled_output_above_limit() {
    let tmp = TempDir::new().expect("tempdir");
    let server = CompactTerminalServer::new(Some(tmp.path().to_path_buf()));

    let input = RunInput {
        command: "seq 1 1000".to_string(),
        cwd: None,
        inline_limit: Some(100),
        timeout_secs: Some(5),
    };

    let result = server.run(Parameters(input)).await.expect("run failed");
    let json = extract_json(result);

    assert_eq!(json["exit_code"], 0);
    assert!(json.get("total_bytes").is_some(), "should be spilled");
    assert!(json.get("total_lines").is_some(), "should be spilled");
    assert!(json.get("spill_file").is_some(), "should have spill_file");
    assert!(json.get("next_steps").is_some(), "should have next_steps");

    let spill_path = json["spill_file"].as_str().unwrap();
    let spill_file = PathBuf::from(spill_path);
    assert!(spill_file.exists(), "spill file should exist on disk");
}

#[tokio::test]
async fn read_spill_by_line_range() {
    let tmp = TempDir::new().expect("tempdir");
    let server = CompactTerminalServer::new(Some(tmp.path().to_path_buf()));

    // First, create a spill file.
    let run_input = RunInput {
        command: "seq 1 100".to_string(),
        cwd: None,
        inline_limit: Some(50),
        timeout_secs: Some(5),
    };
    let run_result =
        server.run(Parameters(run_input)).await.expect("run failed");
    let run_json = extract_json(run_result);
    let spill_path = run_json["spill_file"].as_str().expect("spill_file");

    // Now read lines 10-15 from the spill file.
    let read_input = ReadSpillInput {
        spill_file: PathBuf::from(spill_path),
        start: Some(10),
        end: Some(15),
        grep: None,
    };
    let read_result = server
        .read_spill(Parameters(read_input))
        .await
        .expect("read_spill failed");
    let content = extract_text(read_result);

    assert!(content.contains("10"), "should contain line 10");
    assert!(content.contains("15"), "should contain line 15");
}

#[tokio::test]
async fn read_spill_grep_pattern() {
    let tmp = TempDir::new().expect("tempdir");
    let server = CompactTerminalServer::new(Some(tmp.path().to_path_buf()));

    // Create a spill file with known content.
    let run_input = RunInput {
        command: r#"echo "line 1"; echo "error at line 2"; echo "line 3"; echo "another error at line 4""#.to_string(),
        cwd: None,
        inline_limit: Some(50),
        timeout_secs: Some(5),
    };
    let run_result =
        server.run(Parameters(run_input)).await.expect("run failed");
    let run_json = extract_json(run_result);
    let spill_path = run_json["spill_file"].as_str().expect("spill_file");

    // Grep for "error".
    let read_input = ReadSpillInput {
        spill_file: PathBuf::from(spill_path),
        start: None,
        end: None,
        grep: Some("error".to_string()),
    };
    let read_result = server
        .read_spill(Parameters(read_input))
        .await
        .expect("read_spill failed");
    let content = extract_text(read_result);

    assert!(content.contains("matches"), "should report matches");
}

#[tokio::test]
async fn read_spill_missing_file_returns_mcp_error() {
    let tmp = TempDir::new().expect("tempdir");
    let server = CompactTerminalServer::new(Some(tmp.path().to_path_buf()));

    let read_input = ReadSpillInput {
        spill_file: PathBuf::from("/nonexistent/spill/file.txt"),
        start: None,
        end: None,
        grep: None,
    };

    let result = server.read_spill(Parameters(read_input)).await;
    assert!(result.is_err(), "should return an error for missing file");

    let error = result.err().unwrap();
    // The server translates read_spill errors to McpError::invalid_params.
    // Verify it's a proper McpError (has code -32602 for invalid_params).
    let error_str = format!("{:?}", error);
    assert!(
        error_str.contains("InvalidParams") || error_str.contains("-32602"),
        "missing file should translate to InvalidParams MCP error, got: {error_str}"
    );
}

#[tokio::test]
async fn server_advertises_tools_capability() {
    let tmp = TempDir::new().expect("tempdir");
    let server = CompactTerminalServer::new(Some(tmp.path().to_path_buf()));

    // Verify capability advertisement.
    let info = server.get_info();
    assert!(
        info.capabilities.tools.is_some(),
        "server must advertise tools capability (guards regression of missing .enable_tools())"
    );
}
