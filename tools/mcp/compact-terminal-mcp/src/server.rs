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
    io::Write,
    path::PathBuf,
    process::{
        Command,
        Stdio,
    },
    time::Duration,
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
use uuid::Uuid;

/// Default maximum bytes to return inline. Outputs longer than this are spilled to file.
const DEFAULT_INLINE_LIMIT: usize = 4096;

/// Default command timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 60;

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

// ── Output types ─────────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum RunResult {
    /// Short output returned inline.
    Inline {
        exit_code: i32,
        stdout: String,
        stderr: String,
        elapsed_ms: u128,
    },
    /// Long output spilled to a transient file.
    Spilled {
        exit_code: i32,
        /// First `inline_limit` bytes of stdout for quick scanning.
        stdout_preview: String,
        /// First `inline_limit` bytes of stderr.
        stderr_preview: String,
        /// Total bytes of combined output stored in the spill file.
        total_bytes: usize,
        /// Total lines in the spill file.
        total_lines: usize,
        /// Path to the transient file containing the full output.
        spill_file: PathBuf,
        elapsed_ms: u128,
        /// Suggested follow-up inspection commands.
        next_steps: Vec<String>,
    },
    /// Command timed out.
    TimedOut {
        timeout_secs: u64,
        /// Partial stdout captured before timeout.
        stdout_partial: String,
        spill_file: Option<PathBuf>,
    },
    /// Command could not be launched.
    LaunchError { message: String },
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

    fn write_spill(
        &self,
        content: &str,
    ) -> Result<PathBuf, std::io::Error> {
        std::fs::create_dir_all(&self.spill_dir)?;
        let path = self.spill_dir.join(format!("{}.txt", Uuid::new_v4()));
        let mut f = std::fs::File::create(&path)?;
        f.write_all(content.as_bytes())?;
        Ok(path)
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
        let inline_limit = input.inline_limit.unwrap_or(DEFAULT_INLINE_LIMIT);
        let timeout_secs = input.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS);

        let start = std::time::Instant::now();

        // Build the command.
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(&input.command);
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        if let Some(ref cwd) = input.cwd {
            cmd.current_dir(cwd);
        }

        let child = match cmd.spawn() {
            Ok(c) => c,
            Err(e) => {
                let result = RunResult::LaunchError {
                    message: format!(
                        "failed to spawn '{}': {e}",
                        input.command
                    ),
                };
                return Self::json_result(&result);
            },
        };

        // Wait with timeout using tokio::task::spawn_blocking.
        let timeout = Duration::from_secs(timeout_secs);
        let output = tokio::task::spawn_blocking(move || {
            // Simple timeout via thread: try wait_with_output in a blocking thread.
            // We can't easily kill from here, but the timeout gives the agent feedback.
            child.wait_with_output()
        });

        let output = match tokio::time::timeout(timeout, output).await {
            Ok(Ok(Ok(out))) => out,
            Ok(Ok(Err(e))) => {
                let result = RunResult::LaunchError {
                    message: format!("command failed: {e}"),
                };
                return Self::json_result(&result);
            },
            Ok(Err(e)) => {
                let result = RunResult::LaunchError {
                    message: format!("task panic: {e}"),
                };
                return Self::json_result(&result);
            },
            Err(_timeout) => {
                let result = RunResult::TimedOut {
                    timeout_secs,
                    stdout_partial: String::new(),
                    spill_file: None,
                };
                return Self::json_result(&result);
            },
        };

        let elapsed_ms = start.elapsed().as_millis();
        let exit_code = output.status.code().unwrap_or(-1);
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();

        let combined_len = stdout.len() + stderr.len();

        if combined_len <= inline_limit {
            // Short output — return inline.
            let result = RunResult::Inline {
                exit_code,
                stdout,
                stderr,
                elapsed_ms,
            };
            return Self::json_result(&result);
        }

        // Long output — spill to file.
        let spill_content = format!(
            "=== stdout ===\n{stdout}\n=== stderr ===\n{stderr}\n=== exit_code: {exit_code} ===\n"
        );
        let total_bytes = spill_content.len();
        let total_lines = spill_content.lines().count();

        let spill_file = match self.write_spill(&spill_content) {
            Ok(p) => p,
            Err(e) => {
                // Cannot spill — return truncated inline.
                let result = RunResult::LaunchError {
                    message: format!(
                        "output too large ({combined_len} bytes) and spill failed: {e}"
                    ),
                };
                return Self::json_result(&result);
            },
        };

        let stdout_preview =
            stdout.chars().take(inline_limit / 2).collect::<String>();
        let stderr_preview =
            stderr.chars().take(inline_limit / 2).collect::<String>();

        let spill_str = spill_file.display().to_string();
        let next_steps = vec![
            format!("peek \"{spill_str}\" --count"),
            format!("peek \"{spill_str}\" --grep \"error\" --window 10"),
            format!("peek \"{spill_str}\" --head 30"),
            format!(
                "Use read_spill with start/end or grep to inspect targeted sections"
            ),
        ];

        let result = RunResult::Spilled {
            exit_code,
            stdout_preview,
            stderr_preview,
            total_bytes,
            total_lines,
            spill_file,
            elapsed_ms,
            next_steps,
        };
        Self::json_result(&result)
    }

    async fn read_spill_tool(
        &self,
        input: ReadSpillInput,
    ) -> Result<CallToolResult, McpError> {
        let content = match std::fs::read_to_string(&input.spill_file) {
            Ok(c) => c,
            Err(e) => {
                return Err(McpError::invalid_params(
                    format!(
                        "cannot read spill file '{}': {e}",
                        input.spill_file.display()
                    ),
                    None,
                ));
            },
        };

        let lines: Vec<&str> = content.lines().collect();
        let total = lines.len();

        // grep mode: return matching line numbers.
        if let Some(ref pattern) = input.grep {
            let matches: Vec<usize> = lines
                .iter()
                .enumerate()
                .filter(|(_, l)| l.contains(pattern.as_str()))
                .map(|(i, _)| i + 1)
                .collect();

            let text = if matches.is_empty() {
                format!("no match for {:?} in {} lines", pattern, total)
            } else {
                format!(
                    "matches (line numbers): {}\ntotal: {} of {} lines matched",
                    matches
                        .iter()
                        .map(|n| n.to_string())
                        .collect::<Vec<_>>()
                        .join(", "),
                    matches.len(),
                    total
                )
            };
            return Ok(CallToolResult::success(vec![Content::text(text)]));
        }

        // Bounded window mode.
        let start = input.start.unwrap_or(1).max(1);
        let end = input
            .end
            .unwrap_or_else(|| (start + 80).min(total))
            .min(total);

        if start > total {
            return Err(McpError::invalid_params(
                format!(
                    "start={start} exceeds spill file length ({total} lines)"
                ),
                None,
            ));
        }

        let window: String = lines[start - 1..=end.min(total) - 1]
            .iter()
            .enumerate()
            .map(|(i, l)| format!("{:>6} {l}", start + i))
            .collect::<Vec<_>>()
            .join("\n");

        let header = format!(
            "# spill: {}, lines {start}–{end} of {total}\n",
            input.spill_file.display()
        );
        Ok(CallToolResult::success(vec![Content::text(format!(
            "{header}{window}"
        ))]))
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
    async fn run(
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
    async fn read_spill(
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
            instructions: Some(
                "Compact terminal MCP. Use run() for all shell commands. \
                 Long outputs are truncated inline and stored in a transient file. \
                 Use read_spill() for targeted follow-up inspection."
                    .into(),
            ),
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
