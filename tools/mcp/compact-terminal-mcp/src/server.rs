//! Compact terminal MCP server.
//!
//! Exposes a single `run` tool that executes a shell command and returns:
//!
//! - **Short output** (≤ `inline_limit` bytes): returned directly in the MCP response.
//! - **Long output** (> `inline_limit` bytes): truncated inline summary + a transient
//!   file path where the full output is stored. Follow-up inspection should use
//!   bounded reads (`peek --grep`, `peek --start --end`) on the transient file
//!   rather than re-running the full command.
//!
//! # Transient file lifecycle
//!
//! Transient files are written to `<spill_dir>/<uuid>.txt` (default: system temp dir).
//! They are not automatically deleted — callers should clean up when no longer needed,
//! or rely on OS temp cleanup.

use std::{
    env,
    path::PathBuf,
};

use compact_terminal_api::{
    ReadSpillRequest,
    RunRequest,
    execute,
    read_spill,
};
use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    ServiceExt,
    handler::server::{
        tool::ToolRouter,
        wrapper::Parameters,
    },
    model::{
        CallToolResult,
        Content,
    },
    schemars::{
        self,
        JsonSchema,
    },
    tool,
    tool_handler,
    tool_router,
    transport::stdio,
};
use serde::{
    Deserialize,
    Serialize,
};

// ── Input types ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RunInput {
    /// The shell command to execute (passed to `sh -c`).
    pub command: String,

    /// Working directory for the command. Defaults to the server's working dir.
    #[serde(default)]
    pub cwd: Option<PathBuf>,

    /// Maximum bytes to return inline. Outputs exceeding this are spilled to a
    /// transient file and summarised. Default: 4096.
    #[serde(default)]
    pub inline_limit: Option<usize>,

    /// Command timeout in seconds. Default: 60.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReadSpillInput {
    /// Path to the transient spill file returned by a previous `run` call.
    pub spill_file: PathBuf,

    /// First line to read (1-based, inclusive). Defaults to 1.
    #[serde(default)]
    pub start: Option<usize>,

    /// Last line to read (1-based, inclusive). Defaults to start + 80.
    #[serde(default)]
    pub end: Option<usize>,

    /// Search pattern: returns matching line numbers (1-based) instead of content.
    #[serde(default)]
    pub grep: Option<String>,
}

// ── Server ────────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct CompactTerminalServer {
    spill_dir: PathBuf,
    tool_router: ToolRouter<Self>,
}

impl CompactTerminalServer {
    pub fn new(spill_dir: Option<PathBuf>) -> Self {
        let spill_dir = spill_dir
            .unwrap_or_else(|| env::temp_dir().join("compact-terminal-mcp"));
        Self {
            spill_dir,
            tool_router: Self::tool_router(),
        }
    }

    fn json_result<T: Serialize>(
        value: &T
    ) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string(value).map_err(|e| {
            McpError::internal_error(format!("serialization: {e}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    async fn run_tool(
        &self,
        input: RunInput,
    ) -> Result<CallToolResult, McpError> {
        let request = RunRequest {
            command: input.command,
            cwd: input.cwd,
            inline_limit: input.inline_limit,
            timeout_secs: input.timeout_secs,
            spill_dir: Some(self.spill_dir.clone()),
        };

        let result = tokio::task::spawn_blocking(move || execute(&request))
            .await
            .map_err(|e| {
                McpError::internal_error(format!("task error: {e}"), None)
            })?
            .map_err(|e| {
                McpError::internal_error(format!("execution error: {e}"), None)
            })?;

        Self::json_result(&result)
    }

    async fn read_spill_tool(
        &self,
        input: ReadSpillInput,
    ) -> Result<CallToolResult, McpError> {
        let request = ReadSpillRequest {
            spill_file: input.spill_file,
            start: input.start,
            end: input.end,
            grep: input.grep,
        };

        let result = tokio::task::spawn_blocking(move || read_spill(&request))
            .await
            .map_err(|e| {
                McpError::internal_error(format!("task error: {e}"), None)
            })?
            .map_err(|e| McpError::invalid_params(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(result.content)]))
    }
}

// ── MCP tool surface (delegates to impl methods above) ────────────────────────

#[tool_router]
impl CompactTerminalServer {
    #[tool(description = "
Run a shell command. Short outputs (≤ inline_limit bytes) are returned directly.
Long outputs are summarised inline and stored in a transient file for targeted
follow-up inspection using read_spill or peek.

Use run() for all terminal commands instead of raw shell execution. This keeps
token consumption bounded by preventing large outputs from flooding the context.

Follow-up pattern for spilled output:
  1. Check stdout_preview / stderr_preview for quick diagnosis.
  2. Use read_spill with start/end or grep to inspect targeted sections.
  3. Only re-run the full command if the spill file is insufficient.
")]
    pub async fn run(
        &self,
        Parameters(input): Parameters<RunInput>,
    ) -> Result<CallToolResult, McpError> {
        self.run_tool(input).await
    }

    #[tool(description = "
Read a bounded window from a transient spill file returned by run().

Use this instead of re-running the full command when you need to inspect
specific sections of long output. Prefer grep for pattern search and
start/end for targeted slices.

Patterns:
  - grep: 'error'       → returns matching line numbers
  - start: 1, end: 30  → first 30 lines
  - start: 100, end: 130 → specific slice
")]
    pub async fn read_spill(
        &self,
        Parameters(input): Parameters<ReadSpillInput>,
    ) -> Result<CallToolResult, McpError> {
        self.read_spill_tool(input).await
    }
}

#[tool_handler]
impl ServerHandler for CompactTerminalServer {
    fn get_info(&self) -> rmcp::model::ServerInfo {
        rmcp::model::ServerInfo {
            server_info: rmcp::model::Implementation {
                name: env!("CARGO_PKG_NAME").to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            instructions: Some(
                "Compact terminal MCP. Use run() for all shell commands. \
                 Long outputs are truncated inline and stored in a transient file. \
                 Use read_spill() for targeted follow-up inspection."
                    .into(),
            ),
            capabilities: rmcp::model::ServerCapabilities::builder()
                .enable_tools()
                .build(),
            ..Default::default()
        }
    }
}

pub async fn run_mcp_server(spill_dir: Option<PathBuf>) -> anyhow::Result<()> {
    let server = CompactTerminalServer::new(spill_dir);
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
