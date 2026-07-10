use super::*;
use std::{
    io::{
        Read,
        Write,
    },
    path::{
        Path,
        PathBuf,
    },
    process::{
        Command,
        Stdio,
    },
};

pub(super) const STDIO_TAIL_BYTES: usize = 2048;
const ALLOWED_ENV_SELECTOR_KEYS: &[&str] = &["TICKET_INDEX_ROOT"];

fn filter_env_selectors(
    env_selectors: &serde_json::Map<String, serde_json::Value>
) -> serde_json::Map<String, serde_json::Value> {
    let mut filtered = serde_json::Map::new();
    for key in ALLOWED_ENV_SELECTOR_KEYS {
        if let Some(value) = env_selectors.get(*key) {
            filtered.insert((*key).to_string(), value.clone());
        }
    }
    filtered
}

pub(super) fn classify_stdio_read_error(
    status_code: Option<i32>,
    err_kind: std::io::ErrorKind,
) -> &'static str {
    match status_code {
        Some(code) if code != 0 => "non_zero_exit",
        _ if err_kind == std::io::ErrorKind::UnexpectedEof => "unexpected_eof",
        _ => "io_read_failure",
    }
}

pub(super) fn validate_sentinel_ticket_id(
    created_id: &str,
    get_json: &serde_json::Value,
) -> Result<(), String> {
    let returned_id = get_json["ticket"]["id"].as_str().ok_or_else(|| {
        "mcp stdio sentinel get_ticket missing ticket.id".to_string()
    })?;
    if returned_id != created_id {
        return Err(format!(
            "mcp stdio sentinel get_ticket returned mismatched id: expected {created_id}, got {returned_id}"
        ));
    }
    Ok(())
}

pub(super) fn dispatch_mcp_subprocess_failure_probe(
    domain: &str,
    operation: &str,
    ctx: &MatrixCtx,
    metadata: Option<&DispatchMetadata>,
) -> CellResult {
    if domain != "ticket" {
        return blocked(format!(
            "mcp subprocess failure probe supports only ticket domain, got {domain}.{operation}"
        ));
    }

    let mut client = match operation {
        "get" =>
            StdioMcpClient::spawn_ticket_mcp_nonzero_exit_probe(ctx, metadata)?,
        "spawn_fail" =>
            StdioMcpClient::spawn_ticket_mcp_spawn_failure_probe(ctx, metadata)?,
        _ => {
            return blocked(format!(
                "mcp subprocess failure probe supports only ticket.get and ticket.spawn_fail, got {domain}.{operation}"
            ));
        },
    };

    match client.initialize() {
        Ok(()) => Err(
            "mcp subprocess failure probe unexpectedly initialized successfully"
                .to_string(),
        ),
        Err(bundle) => Err(bundle),
    }
}

struct StdioMcpClient {
    child: std::process::Child,
    next_id: u64,
    invocation_executable: String,
    invocation_args: Vec<String>,
    invocation_cwd: String,
    invocation_env_selectors: serde_json::Map<String, serde_json::Value>,
    metadata: Option<DispatchMetadata>,
}

impl StdioMcpClient {
    fn spawn_ticket_mcp(
        ctx: &MatrixCtx,
        metadata: Option<&DispatchMetadata>,
    ) -> Result<Self, String> {
        let mcp_workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let store_root = ctx.store_root(".ticket");

        let executable = "cargo".to_string();
        let args = vec![
            "run".to_string(),
            "-p".to_string(),
            "ticket-mcp".to_string(),
            "--quiet".to_string(),
        ];

        let mut env_selectors = serde_json::Map::new();
        env_selectors.insert(
            "TICKET_INDEX_ROOT".to_string(),
            serde_json::Value::String(store_root.to_string_lossy().to_string()),
        );

        Self::spawn_with_config(
            executable,
            args,
            mcp_workspace_root,
            env_selectors,
            metadata,
        )
    }

    fn spawn_ticket_mcp_nonzero_exit_probe(
        ctx: &MatrixCtx,
        metadata: Option<&DispatchMetadata>,
    ) -> Result<Self, String> {
        let mcp_workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let store_root = ctx.store_root(".ticket");

        let executable = "cargo".to_string();
        let args = vec!["definitely-not-a-valid-subcommand".to_string()];

        let mut env_selectors = serde_json::Map::new();
        env_selectors.insert(
            "TICKET_INDEX_ROOT".to_string(),
            serde_json::Value::String(store_root.to_string_lossy().to_string()),
        );

        Self::spawn_with_config(
            executable,
            args,
            mcp_workspace_root,
            env_selectors,
            metadata,
        )
    }

    fn spawn_ticket_mcp_spawn_failure_probe(
        ctx: &MatrixCtx,
        metadata: Option<&DispatchMetadata>,
    ) -> Result<Self, String> {
        let mcp_workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let store_root = ctx.store_root(".ticket");

        let executable = "definitely-missing-ticket-mcp-binary".to_string();
        let args = vec!["--version".to_string()];

        let mut env_selectors = serde_json::Map::new();
        env_selectors.insert(
            "TICKET_INDEX_ROOT".to_string(),
            serde_json::Value::String(store_root.to_string_lossy().to_string()),
        );

        Self::spawn_with_config(
            executable,
            args,
            mcp_workspace_root,
            env_selectors,
            metadata,
        )
    }

    fn spawn_with_config(
        executable: String,
        args: Vec<String>,
        cwd: PathBuf,
        env_selectors: serde_json::Map<String, serde_json::Value>,
        metadata: Option<&DispatchMetadata>,
    ) -> Result<Self, String> {
        let mut cmd = Command::new(&executable);
        cmd.args(&args)
            .current_dir(&cwd)
            .env(
                "TICKET_INDEX_ROOT",
                env_selectors
                    .get("TICKET_INDEX_ROOT")
                    .and_then(|value| value.as_str())
                    .unwrap_or_default(),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = cmd.spawn().map_err(|err| {
            build_failure_bundle(
                "spawn_failure",
                &executable,
                &args,
                &cwd,
                &env_selectors,
                "spawn_ticket_mcp",
                metadata,
                "",
                "",
                &format!(
                    "spawn ticket-mcp stdio sentinel process failed: {err}"
                ),
            )
        })?;

        Ok(Self {
            child,
            next_id: 1,
            invocation_executable: executable,
            invocation_args: args,
            invocation_cwd: cwd.to_string_lossy().to_string(),
            invocation_env_selectors: env_selectors,
            metadata: metadata.cloned(),
        })
    }

    fn failure_bundle(
        &self,
        error_class: &str,
        request_or_tool_id: &str,
        message: &str,
        stdout_tail: &str,
        stderr_tail: &str,
    ) -> String {
        build_failure_bundle(
            error_class,
            &self.invocation_executable,
            &self.invocation_args,
            &PathBuf::from(&self.invocation_cwd),
            &self.invocation_env_selectors,
            request_or_tool_id,
            self.metadata.as_ref(),
            stdout_tail,
            stderr_tail,
            message,
        )
    }

    fn initialize(&mut self) -> Result<(), String> {
        let _ = self.request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {
                    "name": "memory-matrix-sentinel",
                    "version": "0.1.0"
                }
            }),
        )?;

        self.send_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }))
    }

    fn request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        self.send_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;

        loop {
            let request_id = format!("{method}#{id}");
            let message = self.read_message(&request_id)?;
            if message["id"].as_u64() != Some(id) {
                continue;
            }

            if message.get("error").is_some() {
                return Err(format!(
                    "mcp `{method}` returned error: {}",
                    message["error"]
                ));
            }
            return Ok(message["result"].clone());
        }
    }

    fn send_message(
        &mut self,
        message: &serde_json::Value,
    ) -> Result<(), String> {
        let mut payload = serde_json::to_vec(message)
            .map_err(|err| format!("serialize mcp message: {err}"))?;
        payload.push(b'\n');
        let stdin = self
            .child
            .stdin
            .as_mut()
            .ok_or_else(|| "ticket-mcp stdin not available".to_string())?;
        stdin
            .write_all(&payload)
            .map_err(|err| format!("write mcp payload: {err}"))?;
        stdin
            .flush()
            .map_err(|err| format!("flush mcp payload: {err}"))
    }

    fn read_message(
        &mut self,
        request_id: &str,
    ) -> Result<serde_json::Value, String> {
        let stdout = self
            .child
            .stdout
            .as_mut()
            .ok_or_else(|| "ticket-mcp stdout not available".to_string())?;

        let mut payload = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            if let Err(err) = stdout.read_exact(&mut byte) {
                let mut stdout_tail = payload.clone();
                let _ = stdout.read_to_end(&mut stdout_tail);

                let mut stderr_tail = Vec::new();
                let status = self.child.wait().ok();
                if let Some(stderr) = self.child.stderr.as_mut() {
                    let _ = stderr.read_to_end(&mut stderr_tail);
                }

                let stderr_tail_text = tail_from_bytes(&stderr_tail);

                let error_class = classify_stdio_read_error(
                    status.and_then(|value| value.code()),
                    err.kind(),
                );

                let message = format!(
                    "read mcp message failed: {err}; child_status={}",
                    status
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                );

                return Err(build_failure_bundle(
                    error_class,
                    &self.invocation_executable,
                    &self.invocation_args,
                    &PathBuf::from(&self.invocation_cwd),
                    &self.invocation_env_selectors,
                    request_id,
                    self.metadata.as_ref(),
                    &tail_from_bytes(&stdout_tail),
                    &stderr_tail_text,
                    &message,
                ));
            }

            if byte[0] == b'\n' {
                break;
            }
            payload.push(byte[0]);
        }

        if payload.is_empty() {
            return self.read_message(request_id);
        }

        serde_json::from_slice(&payload).map_err(|err| {
            self.failure_bundle(
                "parse_decode_error",
                request_id,
                &format!("parse mcp json line: {err}"),
                &tail_from_bytes(&payload),
                "",
            )
        })
    }
}

impl Drop for StdioMcpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(super) fn extract_stdio_tool_json(
    result: &serde_json::Value
) -> Result<serde_json::Value, String> {
    let text = result["content"]
        .as_array()
        .and_then(|content| {
            content.iter().find_map(|entry| {
                let is_text = entry["type"].as_str() == Some("text");
                if is_text {
                    entry["text"].as_str().map(ToString::to_string)
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| {
            "mcp tools/call result missing text content".to_string()
        })?;

    serde_json::from_str(&text)
        .map_err(|err| format!("parse mcp tools/call text payload: {err}"))
}

pub(super) fn dispatch_ticket_mcp_stdio_sentinel_get(
    ctx: &MatrixCtx,
    metadata: Option<&DispatchMetadata>,
) -> CellResult {
    let mut client = StdioMcpClient::spawn_ticket_mcp(ctx, metadata)?;
    client.initialize()?;

    let workspace_root = ctx.workspace_root.to_string_lossy().to_string();
    let title =
        format!("matrix-mcp-stdio-ticket-{}", uuid::Uuid::new_v4().simple());

    let create_result = client.request(
        "tools/call",
        serde_json::json!({
            "name": "create_ticket",
            "arguments": {
                "workspace": workspace_root,
                "type": "tracker-improvement",
                "title": title,
                "state": "new",
                "fields": []
            }
        }),
    )?;
    let create_json =
        extract_stdio_tool_json(&create_result).map_err(|err| {
            client.failure_bundle(
                "parse_decode_error",
                "tools/call#create_ticket",
                &err,
                "",
                "",
            )
        })?;
    if create_json["status"].as_str().unwrap_or_default() != "ok" {
        return Err(client.failure_bundle(
            "protocol_sentinel_mismatch",
            "tools/call#create_ticket",
            &format!(
                "mcp stdio sentinel create_ticket returned non-ok status: {}",
                create_json
            ),
            "",
            "",
        ));
    }
    let created_id = create_json["id"]
        .as_str()
        .ok_or_else(|| {
            client.failure_bundle(
                "protocol_sentinel_mismatch",
                "tools/call#create_ticket",
                "mcp stdio sentinel create_ticket missing id",
                "",
                "",
            )
        })?
        .to_string();

    let get_result = client.request(
        "tools/call",
        serde_json::json!({
            "name": "get_ticket",
            "arguments": {
                "workspace": workspace_root,
                "id": created_id
            }
        }),
    )?;
    let get_json = extract_stdio_tool_json(&get_result).map_err(|err| {
        client.failure_bundle(
            "parse_decode_error",
            "tools/call#get_ticket",
            &err,
            "",
            "",
        )
    })?;
    validate_sentinel_ticket_id(&created_id, &get_json).map_err(|err| {
        client.failure_bundle(
            "protocol_sentinel_mismatch",
            "tools/call#get_ticket",
            &err,
            "",
            "",
        )
    })?;

    Ok(Cell::Passed)
}

pub(super) fn tail_from_bytes(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }
    let start = bytes.len().saturating_sub(STDIO_TAIL_BYTES);
    String::from_utf8_lossy(&bytes[start..]).to_string()
}

pub(super) fn build_failure_bundle(
    error_class: &str,
    executable: &str,
    args: &[String],
    cwd: &Path,
    env_selectors: &serde_json::Map<String, serde_json::Value>,
    request_or_tool_id: &str,
    metadata: Option<&DispatchMetadata>,
    stdout_tail: &str,
    stderr_tail: &str,
    error_message: &str,
) -> String {
    let correlation = if let Some(meta) = metadata {
        serde_json::json!({
            "run_id": meta.run_id,
            "cell_id": meta.cell_id,
            "transport": meta.transport,
            "operation": meta.operation,
            "request_or_tool_id": request_or_tool_id,
        })
    } else {
        serde_json::json!({
            "run_id": serde_json::Value::Null,
            "cell_id": serde_json::Value::Null,
            "transport": "mcp",
            "operation": serde_json::Value::Null,
            "request_or_tool_id": request_or_tool_id,
        })
    };

    let linkage = if let Some(meta) = metadata {
        let has_log_sessions = !meta.log_session_ids.is_empty();
        serde_json::json!({
            "test_execution_id": meta.execution_id,
            "log_session_ids": meta.log_session_ids,
            "log_session_ids_reason": if has_log_sessions {
                "runtime sessions correlated by run_id + test_execution_id"
            } else {
                "runtime log sessions unavailable for this execution"
            },
            "journal_id": serde_json::Value::Null,
        })
    } else {
        serde_json::json!({
            "test_execution_id": serde_json::Value::Null,
            "log_session_ids": [],
            "log_session_ids_reason": "dispatch metadata unavailable",
            "journal_id": serde_json::Value::Null,
        })
    };

    let filtered_env_selectors = filter_env_selectors(env_selectors);

    serde_json::json!({
        "error_class": error_class,
        "message": error_message,
        "invocation": {
            "executable": executable,
            "args": args,
            "cwd": cwd.to_string_lossy().to_string(),
            "workspace_selector": "default",
            "env_selectors": filtered_env_selectors,
        },
        "output_tails": {
            "stdout_tail": stdout_tail,
            "stderr_tail": stderr_tail,
            "max_bytes": STDIO_TAIL_BYTES,
        },
        "correlation": correlation,
        "linkage": linkage,
    })
    .to_string()
}
