use std::{
    collections::BTreeMap,
    io::{
        Read,
        Write,
    },
    path::PathBuf,
    process::{
        Command,
        Stdio,
    },
    sync::Arc,
    time::Instant,
};

use axum::{
    body::{
        Body,
        to_bytes,
    },
    http::{
        Method,
        Request,
        StatusCode,
    },
};
use chrono::Utc;
use rmcp::{
    handler::server::wrapper::Parameters,
    model::CallToolResult,
};
use ticket_mcp::server::{
    CreateTicketInput,
    DeleteTicketInput,
    ListTicketsInput,
    TicketRefInput,
    TicketServer,
    UpdateTicketInput,
};
use spec_mcp::server::{
    CreateSpecInput,
    GetSpecInput,
    ScanInput as SpecScanInput,
    SearchSpecsInput,
    SpecRefInput,
    SpecServer,
    UpdateSpecInput,
};
use rule_mcp::server::{
    CreateRuleInput,
    RuleRefInput,
    ScanInput as RuleScanInput,
    RuleServer,
    SearchRulesInput,
    UpdateRuleInput,
};

use memory_fixtures::{
    FixtureError,
    LoadedFixture,
    materialize_fixture,
};
use test_api::{
    TestStoreConfig,
    ValidationExecution,
    ValidationOutcome,
    ValidationProvenance,
    ValidationSpec,
};
use ticket_api::{
    model::filesystem::ScanRoot,
    storage::store::TicketStore,
};
use ticket_http::{
    AppState,
    WorkspaceRegistry,
    build_router,
    serve::StreamBroker,
};
use tower::ServiceExt;

use crate::domains::{
    AuditDomain,
    DocDomain,
    LogDomain,
    RuleDomain,
    SessionDomain,
    SpecDomain,
    TestDomain,
    TicketDomain,
};

/// The ticket this matrix provides evidence for.
pub(crate) const MATRIX_TICKET_ID: &str = "751f0e71";

/// Operation columns exercised for every domain row.
pub const OPERATIONS: &[&str] = &[
    "get", "search", "create", "update", "delete", "move", "scan",
];

/// Transport axis exercised by the matrix.
pub const TRANSPORTS: &[&str] = &["in-process", "cli", "mcp", "http"];

/// Fixture profile name emitted for every matrix cell execution.
pub const FIXTURE_PROFILE_DEFAULT: &str = "memory-fixtures/default";

/// Expected status declared by the matrix registry for a given cell.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExpectedOutcome {
    Passed,
    Blocked,
}

/// Canonical transport-matrix registry entry.
#[derive(Debug, Clone)]
pub struct CellSpec {
    pub cell_id: String,
    pub domain: String,
    pub operation: String,
    pub transport: String,
    pub fixture_profile: String,
    pub expected_outcome: ExpectedOutcome,
    pub blocked_reason: Option<String>,
}

/// Outcome of a single matrix cell that ran without an internal error.
pub enum Cell {
    /// The operation ran and its correctness assertions held.
    Passed,
    /// The operation could not be exercised; carries a concrete reason.
    Blocked(String),
}

/// Result of a cell run. `Err` maps to a `Failed` execution.
pub type CellResult = Result<Cell, String>;

pub(crate) fn pass() -> CellResult {
    Ok(Cell::Passed)
}

pub(crate) fn blocked(reason: impl Into<String>) -> CellResult {
    Ok(Cell::Blocked(reason.into()))
}

pub(crate) fn unsupported(
    operation: &str,
    domain: &str,
) -> String {
    format!("{domain}-api storage surface exposes no `{operation}` operation")
}

/// Shared context handed to every cell: the materialized workspace root.
pub struct MatrixCtx {
    pub workspace_root: PathBuf,
}

impl MatrixCtx {
    /// Build a context rooted at a materialized fixture workspace.
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self {
            workspace_root: workspace_root.into(),
        }
    }

    /// Resolve a hidden store directory under the materialized workspace.
    pub(crate) fn store_root(
        &self,
        dir: &str,
    ) -> PathBuf {
        self.workspace_root.join(dir)
    }
}

/// One domain row of the matrix.
///
/// Every operation defaults to `Blocked` with an "unsupported" reason; a domain
/// overrides only the operations its storage API genuinely supports.
pub(crate) trait DomainOps {
    fn domain(&self) -> &'static str;

    fn get(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(unsupported("get", self.domain()))
    }
    fn search(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(unsupported("search", self.domain()))
    }
    fn create(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(unsupported("create", self.domain()))
    }
    fn update(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(unsupported("update", self.domain()))
    }
    fn delete(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(unsupported("delete", self.domain()))
    }
    fn move_op(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(format!(
            "{} move surface is not adapter-backed in memory-matrix yet",
            self.domain()
        ))
    }
    fn scan(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(unsupported("scan", self.domain()))
    }
}

fn dispatch(
    ops: &dyn DomainOps,
    transport: &str,
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    if transport == "cli" {
        return dispatch_cli(ops.domain(), operation, ctx);
    }

    if transport == "http" {
        return dispatch_http(ops.domain(), operation, ctx);
    }

    if transport == "mcp" {
        return dispatch_mcp(ops.domain(), operation, ctx);
    }

    if transport != "in-process" {
        return blocked(format!(
            "transport `{transport}` for domain `{}` operation `{operation}` is not wired in the matrix harness yet; \
             recorded as blocked-with-reason per real-transport rollout",
            ops.domain()
        ));
    }

    match operation {
        "get" => ops.get(ctx),
        "search" => ops.search(ctx),
        "create" => ops.create(ctx),
        "update" => ops.update(ctx),
        "delete" => ops.delete(ctx),
        "move" => ops.move_op(ctx),
        "scan" => ops.scan(ctx),
        other => Err(format!("unknown operation `{other}`")),
    }
}

fn dispatch_mcp(
    domain: &str,
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    match domain {
        "ticket" => dispatch_ticket_mcp(operation, ctx),
        "spec" => dispatch_spec_mcp(operation, ctx),
        "rule" => dispatch_rule_mcp(operation, ctx),
        _ => blocked(format!(
            "mcp transport for domain `{domain}` operation `{operation}` is not wired yet"
        )),
    }
}

fn extract_mcp_json(
    result: CallToolResult
) -> Result<serde_json::Value, String> {
    let text = result
        .content
        .iter()
        .find_map(|content| {
            if let rmcp::model::RawContent::Text(text) = &content.raw {
                Some(text.text.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| "mcp result missing text content".to_string())?;
    serde_json::from_str(&text)
        .map_err(|err| format!("parse mcp json result: {err}"))
}

fn dispatch_ticket_mcp(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    if operation == "get" {
        return dispatch_ticket_mcp_stdio_sentinel_get(ctx);
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("build tokio runtime for mcp matrix cell: {err}"))?;

    let workspace_root = ctx.workspace_root.to_string_lossy().to_string();
    let server = TicketServer::new(ctx.store_root(".ticket"));
    let title = format!("matrix-mcp-ticket-{}", uuid::Uuid::new_v4().simple());

    runtime.block_on(async move {
        match operation {
            "create" => {
                let created = server
                    .create_ticket(Parameters(CreateTicketInput {
                        workspace: workspace_root.clone(),
                        type_id: "tracker-improvement".to_string(),
                        title: Some(title),
                        state: Some("new".to_string()),
                        fields: vec![],
                        description: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp ticket create call failed: {err}"))?;
                let json = extract_mcp_json(created)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp ticket create returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "get" => {
                let created = server
                    .create_ticket(Parameters(CreateTicketInput {
                        workspace: workspace_root.clone(),
                        type_id: "tracker-improvement".to_string(),
                        title: Some(title),
                        state: Some("new".to_string()),
                        fields: vec![],
                        description: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for get failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp create result missing id".to_string())?
                    .to_string();

                let result = server
                    .get_ticket(Parameters(TicketRefInput {
                        workspace: workspace_root,
                        id: created_id.clone(),
                    }))
                    .await
                    .map_err(|err| format!("mcp ticket get call failed: {err}"))?;
                let json = extract_mcp_json(result)?;
                let returned_id = json["ticket"]["id"]
                    .as_str()
                    .ok_or_else(|| "mcp ticket get result missing ticket.id".to_string())?;
                if returned_id != created_id {
                    return Err(format!(
                        "mcp ticket get returned mismatched id: expected {created_id}, got {returned_id}"
                    ));
                }
                Ok(Cell::Passed)
            },
            "search" => {
                let created = server
                    .create_ticket(Parameters(CreateTicketInput {
                        workspace: workspace_root.clone(),
                        type_id: "tracker-improvement".to_string(),
                        title: Some(title.clone()),
                        state: Some("new".to_string()),
                        fields: vec![],
                        description: None,
                    }))
                    .await
                    .map_err(|err| {
                        format!("mcp seed create for search failed: {err}")
                    })?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp create result missing id".to_string())?
                    .to_string();

                let result = server
                    .list_tickets(Parameters(ListTicketsInput {
                        workspace: workspace_root,
                        state: None,
                        type_id: None,
                        query: Some(title),
                        limit: Some(10),
                    }))
                    .await
                    .map_err(|err| format!("mcp ticket list call failed: {err}"))?;
                let json = extract_mcp_json(result)?;
                let items = json["items"]
                    .as_array()
                    .ok_or_else(|| "mcp ticket list result missing items".to_string())?;
                let found = items.iter().any(|item| {
                    item["id"].as_str().map(|value| value == created_id).unwrap_or(false)
                });
                if !found {
                    return Err(format!(
                        "mcp ticket search did not return seeded ticket id {created_id}"
                    ));
                }
                Ok(Cell::Passed)
            },
            "update" => {
                let created = server
                    .create_ticket(Parameters(CreateTicketInput {
                        workspace: workspace_root.clone(),
                        type_id: "tracker-improvement".to_string(),
                        title: Some(title),
                        state: Some("new".to_string()),
                        fields: vec![],
                        description: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for update failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp create result missing id".to_string())?
                    .to_string();

                let updated = server
                    .update_ticket(Parameters(UpdateTicketInput {
                        workspace: workspace_root,
                        id: created_id,
                        transition_states: vec![],
                        to_state: Some("ready".to_string()),
                        fields: None,
                        field_map: None,
                        undo: false,
                        description: None,
                        author: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp ticket update call failed: {err}"))?;
                let json = extract_mcp_json(updated)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp ticket update returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "delete" => {
                let created = server
                    .create_ticket(Parameters(CreateTicketInput {
                        workspace: workspace_root.clone(),
                        type_id: "tracker-improvement".to_string(),
                        title: Some(title),
                        state: Some("new".to_string()),
                        fields: vec![],
                        description: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for delete failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp create result missing id".to_string())?
                    .to_string();

                let deleted = server
                    .delete_ticket(Parameters(DeleteTicketInput {
                        workspace: workspace_root,
                        id: created_id,
                    }))
                    .await
                    .map_err(|err| format!("mcp ticket delete call failed: {err}"))?;
                let json = extract_mcp_json(deleted)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp ticket delete returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            _ => blocked(format!(
                "mcp transport for domain `ticket` operation `{operation}` is not wired yet"
            )),
        }
    })
}

struct StdioMcpClient {
    child: std::process::Child,
    next_id: u64,
}

impl StdioMcpClient {
    fn spawn_ticket_mcp(store_root: &std::path::Path) -> Result<Self, String> {
        let mcp_workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let mut cmd = Command::new("cargo");
        cmd.args(["run", "-p", "ticket-mcp", "--quiet"])
            .current_dir(mcp_workspace_root)
            .env("TICKET_INDEX_ROOT", store_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = cmd
            .spawn()
            .map_err(|err| format!("spawn ticket-mcp stdio sentinel process: {err}"))?;

        Ok(Self { child, next_id: 1 })
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
            let message = self.read_message()?;
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

    fn send_message(&mut self, message: &serde_json::Value) -> Result<(), String> {
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

    fn read_message(&mut self) -> Result<serde_json::Value, String> {
        let stdout = self
            .child
            .stdout
            .as_mut()
            .ok_or_else(|| "ticket-mcp stdout not available".to_string())?;

        let mut payload = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            if let Err(err) = stdout.read_exact(&mut byte) {
                let mut stderr_tail = String::new();
                let status = self.child.wait().ok();
                if let Some(stderr) = self.child.stderr.as_mut() {
                    let _ = stderr.read_to_string(&mut stderr_tail);
                }
                let status_note = status
                    .map(|value| format!("; child status: {value}"))
                    .unwrap_or_default();
                let stderr_note = if stderr_tail.trim().is_empty() {
                    "".to_string()
                } else {
                    format!("; child stderr: {}", stderr_tail.trim())
                };
                return Err(format!(
                    "read mcp message: {err}{status_note}{stderr_note}"
                ));
            }

            if byte[0] == b'\n' {
                break;
            }
            payload.push(byte[0]);
        }

        if payload.is_empty() {
            return self.read_message();
        }

        serde_json::from_slice(&payload)
            .map_err(|err| format!("parse mcp json line: {err}"))
    }
}

impl Drop for StdioMcpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn extract_stdio_tool_json(
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
        .ok_or_else(|| "mcp tools/call result missing text content".to_string())?;

    serde_json::from_str(&text)
        .map_err(|err| format!("parse mcp tools/call text payload: {err}"))
}

fn dispatch_ticket_mcp_stdio_sentinel_get(ctx: &MatrixCtx) -> CellResult {
    let mut client =
        StdioMcpClient::spawn_ticket_mcp(&ctx.store_root(".ticket"))?;
    client.initialize()?;

    let workspace_root = ctx.workspace_root.to_string_lossy().to_string();
    let title = format!(
        "matrix-mcp-stdio-ticket-{}",
        uuid::Uuid::new_v4().simple()
    );

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
    let create_json = extract_stdio_tool_json(&create_result)?;
    if create_json["status"].as_str().unwrap_or_default() != "ok" {
        return Err(format!(
            "mcp stdio sentinel create_ticket returned non-ok status: {}",
            create_json
        ));
    }
    let created_id = create_json["id"]
        .as_str()
        .ok_or_else(|| "mcp stdio sentinel create_ticket missing id".to_string())?
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
    let get_json = extract_stdio_tool_json(&get_result)?;
    let returned_id = get_json["ticket"]["id"]
        .as_str()
        .ok_or_else(|| {
            "mcp stdio sentinel get_ticket missing ticket.id".to_string()
        })?;
    if returned_id != created_id {
        return Err(format!(
            "mcp stdio sentinel get_ticket returned mismatched id: expected {created_id}, got {returned_id}"
        ));
    }

    Ok(Cell::Passed)
}

fn dispatch_spec_mcp(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("build tokio runtime for mcp matrix cell: {err}"))?;

    let workspace_root = ctx.workspace_root.to_string_lossy().to_string();
    let server = SpecServer::new(ctx.store_root(".spec"));
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let slug = format!("matrix/mcp/spec-{suffix}");
    let title = format!("Matrix MCP Spec {suffix}");

    runtime.block_on(async move {
        match operation {
            "create" => {
                let created = server
                    .spec_create(Parameters(CreateSpecInput {
                        workspace: workspace_root,
                        title,
                        slug,
                        component: "matrix".to_string(),
                        parent: None,
                        scope: Some("internal".to_string()),
                        body: Some("matrix mcp body".to_string()),
                        fields: BTreeMap::new(),
                    }))
                    .await
                    .map_err(|err| format!("mcp spec create call failed: {err}"))?;
                let json = extract_mcp_json(created)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp spec create returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "get" => {
                let created = server
                    .spec_create(Parameters(CreateSpecInput {
                        workspace: workspace_root.clone(),
                        title,
                        slug,
                        component: "matrix".to_string(),
                        parent: None,
                        scope: Some("internal".to_string()),
                        body: Some("matrix mcp body".to_string()),
                        fields: BTreeMap::new(),
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for spec get failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp spec create result missing id".to_string())?
                    .to_string();

                let result = server
                    .spec_get(Parameters(GetSpecInput {
                        workspace: Some(workspace_root),
                        id: created_id.clone(),
                        full: false,
                    }))
                    .await
                    .map_err(|err| format!("mcp spec get call failed: {err}"))?;
                let json = extract_mcp_json(result)?;
                let returned_id = json["spec"]["id"]
                    .as_str()
                    .ok_or_else(|| "mcp spec get result missing spec.id".to_string())?;
                if returned_id != created_id {
                    return Err(format!(
                        "mcp spec get returned mismatched id: expected {created_id}, got {returned_id}"
                    ));
                }
                Ok(Cell::Passed)
            },
            "search" => {
                let created = server
                    .spec_create(Parameters(CreateSpecInput {
                        workspace: workspace_root.clone(),
                        title: title.clone(),
                        slug,
                        component: "matrix".to_string(),
                        parent: None,
                        scope: Some("internal".to_string()),
                        body: Some("matrix mcp body".to_string()),
                        fields: BTreeMap::new(),
                    }))
                    .await
                    .map_err(|err| {
                        format!("mcp seed create for spec search failed: {err}")
                    })?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp spec create result missing id".to_string())?
                    .to_string();

                let result = server
                    .spec_search(Parameters(SearchSpecsInput {
                        workspace: Some(workspace_root),
                        query: title,
                        limit: 10,
                    }))
                    .await
                    .map_err(|err| format!("mcp spec search call failed: {err}"))?;
                let json = extract_mcp_json(result)?;
                let items = json["items"]
                    .as_array()
                    .ok_or_else(|| "mcp spec search result missing items".to_string())?;
                let found = items.iter().any(|item| {
                    item["id"]
                        .as_str()
                        .map(|value| value == created_id)
                        .unwrap_or(false)
                });
                if !found {
                    return Err(format!(
                        "mcp spec search did not return seeded spec id {created_id}"
                    ));
                }
                Ok(Cell::Passed)
            },
            "update" => {
                let created = server
                    .spec_create(Parameters(CreateSpecInput {
                        workspace: workspace_root.clone(),
                        title,
                        slug,
                        component: "matrix".to_string(),
                        parent: None,
                        scope: Some("internal".to_string()),
                        body: Some("matrix mcp body".to_string()),
                        fields: BTreeMap::new(),
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for spec update failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp spec create result missing id".to_string())?
                    .to_string();

                let updated = server
                    .spec_update(Parameters(UpdateSpecInput {
                        workspace: Some(workspace_root),
                        id: created_id,
                        fields: Some(vec!["title=Matrix MCP Updated".to_string()]),
                        to_state: None,
                        body: None,
                        field_map: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp spec update call failed: {err}"))?;
                let json = extract_mcp_json(updated)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp spec update returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "delete" => {
                let created = server
                    .spec_create(Parameters(CreateSpecInput {
                        workspace: workspace_root.clone(),
                        title,
                        slug,
                        component: "matrix".to_string(),
                        parent: None,
                        scope: Some("internal".to_string()),
                        body: Some("matrix mcp body".to_string()),
                        fields: BTreeMap::new(),
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for spec delete failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp spec create result missing id".to_string())?
                    .to_string();

                let deleted = server
                    .spec_delete(Parameters(SpecRefInput {
                        workspace: Some(workspace_root),
                        id: created_id,
                    }))
                    .await
                    .map_err(|err| format!("mcp spec delete call failed: {err}"))?;
                let json = extract_mcp_json(deleted)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp spec delete returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "scan" => {
                let scanned = server
                    .spec_scan(Parameters(SpecScanInput {
                        workspace: Some(workspace_root),
                        force: false,
                    }))
                    .await
                    .map_err(|err| format!("mcp spec scan call failed: {err}"))?;
                let json = extract_mcp_json(scanned)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp spec scan returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            _ => blocked(format!(
                "mcp transport for domain `spec` operation `{operation}` is not wired yet"
            )),
        }
    })
}

fn dispatch_rule_mcp(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("build tokio runtime for mcp matrix cell: {err}"))?;

    let workspace_root = ctx.workspace_root.to_string_lossy().to_string();
    let server = RuleServer::new(ctx.store_root(".rule"));
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let slug = format!("matrix/mcp/rule-{suffix}");
    let title = format!("Matrix MCP Rule {suffix}");

    runtime.block_on(async move {
        match operation {
            "create" => {
                let created = server
                    .rule_create(Parameters(CreateRuleInput {
                        workspace: workspace_root,
                        title,
                        slug,
                        file_kind: "markdown".to_string(),
                        section: "matrix".to_string(),
                        body: Some("matrix mcp body".to_string()),
                        repo_scope: vec![],
                        path_scope: vec![],
                        order_key: None,
                        source_repo: None,
                        source_path: None,
                        source_start_line: None,
                        source_end_line: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp rule create call failed: {err}"))?;
                let json = extract_mcp_json(created)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp rule create returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "get" => {
                let created = server
                    .rule_create(Parameters(CreateRuleInput {
                        workspace: workspace_root,
                        title,
                        slug,
                        file_kind: "markdown".to_string(),
                        section: "matrix".to_string(),
                        body: Some("matrix mcp body".to_string()),
                        repo_scope: vec![],
                        path_scope: vec![],
                        order_key: None,
                        source_repo: None,
                        source_path: None,
                        source_start_line: None,
                        source_end_line: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for rule get failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp rule create result missing id".to_string())?
                    .to_string();

                let result = server
                    .rule_get(Parameters(RuleRefInput { id: created_id.clone() }))
                    .await
                    .map_err(|err| format!("mcp rule get call failed: {err}"))?;
                let json = extract_mcp_json(result)?;
                let returned_id = json["rule"]["id"]
                    .as_str()
                    .ok_or_else(|| "mcp rule get result missing rule.id".to_string())?;
                if returned_id != created_id {
                    return Err(format!(
                        "mcp rule get returned mismatched id: expected {created_id}, got {returned_id}"
                    ));
                }
                Ok(Cell::Passed)
            },
            "search" => {
                let created = server
                    .rule_create(Parameters(CreateRuleInput {
                        workspace: workspace_root,
                        title: title.clone(),
                        slug,
                        file_kind: "markdown".to_string(),
                        section: "matrix".to_string(),
                        body: Some("matrix mcp body".to_string()),
                        repo_scope: vec![],
                        path_scope: vec![],
                        order_key: None,
                        source_repo: None,
                        source_path: None,
                        source_start_line: None,
                        source_end_line: None,
                    }))
                    .await
                    .map_err(|err| {
                        format!("mcp seed create for rule search failed: {err}")
                    })?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp rule create result missing id".to_string())?
                    .to_string();

                let result = server
                    .rule_search(Parameters(SearchRulesInput {
                        query: title,
                        state: None,
                        file_kind: None,
                        section: None,
                        repo_scope: None,
                        path_scope: None,
                        slug: None,
                        low_rated_only: false,
                        unresolved_only: false,
                        limit: 10,
                    }))
                    .await
                    .map_err(|err| format!("mcp rule search call failed: {err}"))?;
                let json = extract_mcp_json(result)?;
                let items = json["items"]
                    .as_array()
                    .ok_or_else(|| "mcp rule search result missing items".to_string())?;
                let found = items.iter().any(|item| {
                    item["id"]
                        .as_str()
                        .map(|value| value == created_id)
                        .unwrap_or(false)
                });
                if !found {
                    return Err(format!(
                        "mcp rule search did not return seeded rule id {created_id}"
                    ));
                }
                Ok(Cell::Passed)
            },
            "update" => {
                let created = server
                    .rule_create(Parameters(CreateRuleInput {
                        workspace: workspace_root,
                        title,
                        slug,
                        file_kind: "markdown".to_string(),
                        section: "matrix".to_string(),
                        body: Some("matrix mcp body".to_string()),
                        repo_scope: vec![],
                        path_scope: vec![],
                        order_key: None,
                        source_repo: None,
                        source_path: None,
                        source_start_line: None,
                        source_end_line: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for rule update failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp rule create result missing id".to_string())?
                    .to_string();

                let updated = server
                    .rule_update(Parameters(UpdateRuleInput {
                        id: created_id,
                        fields: Some(vec!["title=Matrix MCP Updated Rule".to_string()]),
                        field_map: None,
                        to_state: None,
                        body: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp rule update call failed: {err}"))?;
                let json = extract_mcp_json(updated)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp rule update returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "scan" => {
                let scanned = server
                    .rule_scan(Parameters(RuleScanInput { force: false }))
                    .await
                    .map_err(|err| format!("mcp rule scan call failed: {err}"))?;
                let json = extract_mcp_json(scanned)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp rule scan returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            _ => blocked(format!(
                "mcp transport for domain `rule` operation `{operation}` is not wired yet"
            )),
        }
    })
}

fn domain_names() -> Vec<&'static str> {
    domains().into_iter().map(|d| d.domain()).collect()
}

fn in_process_supported(
    domain: &str,
    operation: &str,
) -> bool {
    match domain {
        "ticket" | "spec" | "rule" => OPERATIONS.contains(&operation),
        "audit" => ["search", "scan"].contains(&operation),
        "session" => ["create", "get", "search", "update"].contains(&operation),
        "test" => ["create", "get", "search", "update"].contains(&operation),
        "log" => ["create", "get", "search", "update"].contains(&operation),
        "doc" => false,
        _ => false,
    }
}

fn cli_supported(
    domain: &str,
    operation: &str,
) -> bool {
    ["ticket", "spec", "rule"].contains(&domain) && operation != "move"
}

fn http_supported(
    domain: &str,
    operation: &str,
) -> bool {
    domain == "ticket" && ["get", "search"].contains(&operation)
}

fn is_supported(
    domain: &str,
    transport: &str,
    operation: &str,
) -> bool {
    match transport {
        "in-process" => in_process_supported(domain, operation),
        "cli" => cli_supported(domain, operation),
        "http" => http_supported(domain, operation),
        "mcp" => {
            match domain {
                "ticket" => ["create", "get", "search", "update", "delete"]
                    .contains(&operation),
                "spec" => ["create", "get", "search", "update", "delete", "scan"]
                    .contains(&operation),
                "rule" => ["create", "get", "search", "update", "scan"]
                    .contains(&operation),
                _ => false,
            }
        },
        _ => false,
    }
}

fn expected_blocked_reason(
    domain: &str,
    transport: &str,
    operation: &str,
) -> String {
    match transport {
        "in-process" => {
            if operation == "move" {
                format!(
                    "{domain} move surface is not adapter-backed in memory-matrix yet"
                )
            } else {
                unsupported(operation, domain)
            }
        },
        "cli" => {
            if operation == "move" {
                format!(
                    "cli transport for domain `{domain}` operation `move` is not wired in memory-matrix yet; in-process move cells exercise the adapter-backed move kernel"
                )
            } else {
                format!(
                    "cli transport for domain `{domain}` operation `{operation}` is not wired yet"
                )
            }
        },
        "http" => {
            if domain == "ticket" {
                format!(
                    "http transport for domain `ticket` operation `{operation}` is not wired yet; currently only `ticket.get@http` and `ticket.search@http` are exercised through the ticket-http router surface"
                )
            } else {
                format!(
                    "http transport for domain `{domain}` operation `{operation}` is not wired yet"
                )
            }
        },
        _ => format!(
            "transport `{transport}` for domain `{domain}` operation `{operation}` is not wired in the matrix harness yet; recorded as blocked-with-reason per real-transport rollout"
        ),
    }
}

fn expected_outcome_for_cell(
    domain: &str,
    transport: &str,
    operation: &str,
) -> (ExpectedOutcome, Option<String>) {
    if is_supported(domain, transport, operation) {
        (ExpectedOutcome::Passed, None)
    } else {
        (
            ExpectedOutcome::Blocked,
            Some(expected_blocked_reason(domain, transport, operation)),
        )
    }
}

/// Canonical transport-cell registry for `domain x operation x transport`.
pub fn transport_cells() -> Vec<CellSpec> {
    let mut out = Vec::new();
    for domain in domain_names() {
        for &operation in OPERATIONS {
            for &transport in TRANSPORTS {
                let (expected_outcome, blocked_reason) =
                    expected_outcome_for_cell(domain, transport, operation);
                out.push(CellSpec {
                    cell_id: format!("{domain}.{operation}.{transport}"),
                    domain: domain.to_string(),
                    operation: operation.to_string(),
                    transport: transport.to_string(),
                    fixture_profile: FIXTURE_PROFILE_DEFAULT.to_string(),
                    expected_outcome,
                    blocked_reason,
                });
            }
        }
    }
    out
}

fn dispatch_http(
    domain: &str,
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    match domain {
        "ticket" => dispatch_ticket_http(operation, ctx),
        _ => blocked(format!(
            "http transport for domain `{domain}` operation `{operation}` is not wired yet"
        )),
    }
}

fn dispatch_ticket_http(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    match operation {
        "get" => run_ticket_http_get(ctx),
        "search" => run_ticket_http_search(ctx),
        _ => blocked(format!(
            "http transport for domain `ticket` operation `{operation}` is not wired yet; \
             currently only `ticket.get@http` and `ticket.search@http` are exercised through the ticket-http router surface"
        )),
    }
}

fn run_ticket_http_get(ctx: &MatrixCtx) -> CellResult {
    let (id, workspace, app) = build_ticket_http_fixture(ctx)?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("build tokio runtime for http matrix cell: {err}"))?;

    runtime
        .block_on(async move {
            let request = Request::builder()
                .method(Method::GET)
                .uri(format!("/api/tickets/{id}?workspace={workspace}"))
                .body(Body::empty())
                .map_err(|err| format!("build ticket get request: {err}"))?;

            let response = app
                .oneshot(request)
                .await
                .map_err(|err| format!("dispatch ticket get request: {err}"))?;

            if response.status() != StatusCode::OK {
                return Err(format!(
                    "ticket-http get returned unexpected status {}",
                    response.status()
                ));
            }

            let bytes = to_bytes(response.into_body(), 1024 * 1024)
                .await
                .map_err(|err| format!("read ticket get response body: {err}"))?;
            let payload: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|err| format!("parse ticket get response body: {err}"))?;

            let returned_id = payload["ticket"]["ticket_ref"]["id"]
                .as_str()
                .ok_or_else(|| {
                    "ticket get payload missing ticket.ticket_ref.id".to_string()
                })?;
            if returned_id != id.to_string() {
                return Err(format!(
                    "ticket-http get returned mismatched id: expected {id}, got {returned_id}"
                ));
            }

            Ok(Cell::Passed)
        })
}

fn run_ticket_http_search(ctx: &MatrixCtx) -> CellResult {
    let (id, workspace, app) = build_ticket_http_fixture(ctx)?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("build tokio runtime for http matrix cell: {err}"))?;

    runtime
        .block_on(async move {
            let request = Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/tickets?workspace={workspace}&query=matrix-http-ticket"
                ))
                .body(Body::empty())
                .map_err(|err| format!("build ticket search request: {err}"))?;

            let response = app
                .oneshot(request)
                .await
                .map_err(|err| format!("dispatch ticket search request: {err}"))?;

            if response.status() != StatusCode::OK {
                return Err(format!(
                    "ticket-http search returned unexpected status {}",
                    response.status()
                ));
            }

            let bytes = to_bytes(response.into_body(), 1024 * 1024)
                .await
                .map_err(|err| format!("read ticket search response body: {err}"))?;
            let payload: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|err| format!("parse ticket search response body: {err}"))?;

            let items = payload["items"]
                .as_array()
                .ok_or_else(|| "ticket search payload missing items array".to_string())?;
            let expected_id = id.to_string();
            let found = items.iter().any(|item| {
                item["ticket_ref"]["id"]
                    .as_str()
                    .map(|candidate| candidate == expected_id)
                    .unwrap_or(false)
            });
            if !found {
                return Err(format!(
                    "ticket-http search did not return seeded ticket id {expected_id}"
                ));
            }

            Ok(Cell::Passed)
        })
}

fn build_ticket_http_fixture(
    ctx: &MatrixCtx,
) -> Result<(uuid::Uuid, String, axum::Router), String> {
    let ticket_store_root = ctx.store_root(".ticket");
    let tickets_scan_root = ctx.workspace_root.join("tickets");

    std::fs::create_dir_all(&tickets_scan_root).map_err(|err| {
        format!(
            "failed to create ticket scan root `{}`: {err}",
            tickets_scan_root.display()
        )
    })?;

    let store = Arc::new(
        TicketStore::open_or_init(&ticket_store_root)
            .map_err(|err| format!("open ticket store: {err}"))?,
    );

    let has_scan_root = store
        .list_scan_roots()
        .map_err(|err| format!("list scan roots: {err}"))?
        .into_iter()
        .any(|root| root.path == tickets_scan_root);
    if !has_scan_root {
        store
            .add_scan_root(ScanRoot {
                path: tickets_scan_root,
                label: "default".into(),
            })
            .map_err(|err| format!("add ticket scan root: {err}"))?;
    }

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("matrix-http-ticket-get"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .map_err(|err| format!("seed ticket for http get: {err}"))?;

    let state = AppState::new(
        Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&store))),
        Arc::new(StreamBroker::new()),
    );
    let workspace = state.registry.primary_workspace_name().to_string();
    let app = build_router(state);

    Ok((id, workspace, app))
}

fn dispatch_cli(
    domain: &str,
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    if operation == "move" {
        return blocked(format!(
            "cli transport for domain `{domain}` operation `move` is not wired in memory-matrix yet; in-process move cells exercise the adapter-backed move kernel"
        ));
    }

    match domain {
        "ticket" => dispatch_ticket_cli(operation, ctx),
        "spec" => dispatch_spec_cli(operation, ctx),
        "rule" => dispatch_rule_cli(operation, ctx),
        _ => blocked(format!(
            "cli transport for domain `{domain}` operation `{operation}` is not wired yet"
        )),
    }
}

fn run_ticket_cli(args: Vec<String>) -> Result<(), String> {
    let cli = ticket_cli::cli::parse_cli_from(args)
        .map_err(|err| err.to_string())?;
    ticket_cli::cli::run(cli).map_err(|err| err.to_string())?;
    Ok(())
}

fn dispatch_ticket_cli(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    let root = ctx.workspace_root.to_string_lossy().to_string();
    let id = uuid::Uuid::new_v4().to_string();
    let token = format!("matrix-cli-ticket-{}", uuid::Uuid::new_v4().simple());

    match operation {
        "create" => run_ticket_cli(vec![
            "ticket".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "create".into(),
            "--id".into(),
            id,
            "--type".into(),
            "tracker-improvement".into(),
            "--title".into(),
            token,
            "--state".into(),
            "new".into(),
        ]),
        "get" => {
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--id".into(),
                id.clone(),
                "--type".into(),
                "tracker-improvement".into(),
                "--title".into(),
                token,
                "--state".into(),
                "new".into(),
            ])?;
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "get".into(),
                id,
            ])
        },
        "search" => {
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--id".into(),
                id,
                "--type".into(),
                "tracker-improvement".into(),
                "--title".into(),
                token.clone(),
                "--state".into(),
                "new".into(),
            ])?;
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "search".into(),
                token,
                "--limit".into(),
                "10".into(),
            ])
        },
        "update" => {
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--id".into(),
                id.clone(),
                "--type".into(),
                "tracker-improvement".into(),
                "--title".into(),
                token,
                "--state".into(),
                "new".into(),
            ])?;
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "update".into(),
                id,
                "--to-state".into(),
                "ready".into(),
            ])
        },
        "delete" => {
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--id".into(),
                id.clone(),
                "--type".into(),
                "tracker-improvement".into(),
                "--title".into(),
                token,
                "--state".into(),
                "new".into(),
            ])?;
            run_ticket_cli(vec![
                "ticket".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "delete".into(),
                id,
            ])
        },
        "scan" => run_ticket_cli(vec![
            "ticket".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "scan".into(),
        ]),
        other => Err(format!("unknown operation `{other}`")),
    }
    .map(|_| Cell::Passed)
}

fn run_spec_cli(args: Vec<String>) -> Result<(), String> {
    let cli = spec_cli::cli::parse_cli_from(args).map_err(|err| err.to_string())?;
    spec_cli::cli::run(cli).map_err(|err| err.to_string())?;
    Ok(())
}

fn dispatch_spec_cli(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    let root = ctx.workspace_root.to_string_lossy().to_string();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let slug = format!("matrix/cli/{suffix}");
    let token = format!("Matrix CLI Spec {suffix}");

    match operation {
        "create" => run_spec_cli(vec![
            "spec".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "create".into(),
            "--title".into(),
            token,
            "--slug".into(),
            slug,
            "--component".into(),
            "matrix".into(),
        ]),
        "get" => {
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--component".into(),
                "matrix".into(),
            ])?;
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "get".into(),
                slug,
            ])
        },
        "search" => {
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token.clone(),
                "--slug".into(),
                slug,
                "--component".into(),
                "matrix".into(),
            ])?;
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "search".into(),
                token,
                "--limit".into(),
                "10".into(),
            ])
        },
        "update" => {
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--component".into(),
                "matrix".into(),
            ])?;
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "update".into(),
                slug,
                "--field".into(),
                "scope=internal".into(),
            ])
        },
        "delete" => {
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--component".into(),
                "matrix".into(),
            ])?;
            run_spec_cli(vec![
                "spec".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "delete".into(),
                slug,
            ])
        },
        "scan" => run_spec_cli(vec![
            "spec".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "scan".into(),
        ]),
        other => Err(format!("unknown operation `{other}`")),
    }
    .map(|_| Cell::Passed)
}

fn run_rule_cli(args: Vec<String>) -> Result<(), String> {
    let cli = rule_cli::cli::parse_cli_from(args).map_err(|err| err.to_string())?;
    rule_cli::cli::run(cli).map_err(|err| err.to_string())?;
    Ok(())
}

fn dispatch_rule_cli(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    let root = ctx.workspace_root.to_string_lossy().to_string();
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let slug = format!("matrix/cli/{suffix}");
    let token = format!("Matrix CLI Rule {suffix}");

    match operation {
        "create" => run_rule_cli(vec![
            "rule".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "create".into(),
            "--title".into(),
            token,
            "--slug".into(),
            slug,
            "--file-kind".into(),
            "markdown".into(),
            "--section".into(),
            "matrix".into(),
            "--body".into(),
            "matrix body".into(),
        ]),
        "get" => {
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--file-kind".into(),
                "markdown".into(),
                "--section".into(),
                "matrix".into(),
                "--body".into(),
                "matrix body".into(),
            ])?;
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "get".into(),
                slug,
            ])
        },
        "search" => {
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token.clone(),
                "--slug".into(),
                slug,
                "--file-kind".into(),
                "markdown".into(),
                "--section".into(),
                "matrix".into(),
                "--body".into(),
                "matrix body".into(),
            ])?;
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "search".into(),
                token,
                "--limit".into(),
                "10".into(),
            ])
        },
        "update" => {
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--file-kind".into(),
                "markdown".into(),
                "--section".into(),
                "matrix".into(),
                "--body".into(),
                "matrix body".into(),
            ])?;
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "update".into(),
                slug,
                "--body".into(),
                "updated body".into(),
            ])
        },
        "delete" => {
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root.clone(),
                "create".into(),
                "--title".into(),
                token,
                "--slug".into(),
                slug.clone(),
                "--file-kind".into(),
                "markdown".into(),
                "--section".into(),
                "matrix".into(),
                "--body".into(),
                "matrix body".into(),
            ])?;
            run_rule_cli(vec![
                "rule".into(),
                "--json".into(),
                "--workspace-root".into(),
                root,
                "delete".into(),
                slug,
            ])
        },
        "scan" => run_rule_cli(vec![
            "rule".into(),
            "--json".into(),
            "--workspace-root".into(),
            root,
            "scan".into(),
        ]),
        other => Err(format!("unknown operation `{other}`")),
    }
    .map(|_| Cell::Passed)
}

/// All `(domain, operation)` cells of the matrix, in registration order.
pub fn cells() -> Vec<(&'static str, &'static str)> {
    let mut out = Vec::new();
    for domain in domain_names() {
        for &operation in OPERATIONS {
            out.push((domain, operation));
        }
    }
    out
}

/// Stable, path-safe Criterion benchmark id for a `domain x operation` cell.
///
/// Both the bench harness and the ingest runner derive the Criterion output
/// directory from this id, so they must agree on its form.
pub fn bench_id(
    domain: &str,
    operation: &str,
) -> String {
    format!("{domain}__{operation}")
}

/// Run a single matrix cell, selected by domain + operation name, against
/// `ctx`. This is the per-cell entry point reused by the benchmark harness.
pub fn run_one(
    domain: &str,
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    for candidate in domains() {
        if candidate.domain() == domain {
            return dispatch(&*candidate, "in-process", operation, ctx);
        }
    }
    Err(format!("unknown domain `{domain}`"))
}

/// The registered domain rows of the matrix.
fn domains() -> Vec<Box<dyn DomainOps>> {
    vec![
        Box::new(TicketDomain),
        Box::new(SpecDomain),
        Box::new(RuleDomain),
        Box::new(AuditDomain),
        Box::new(SessionDomain),
        Box::new(TestDomain),
        Box::new(DocDomain),
        Box::new(LogDomain),
    ]
}

/// Recorded result for one matrix cell.
#[derive(Debug, Clone)]
pub struct CellRecord {
    pub cell_id: String,
    pub domain: String,
    pub transport: String,
    pub operation: String,
    pub fixture_profile: String,
    pub expected_outcome: ExpectedOutcome,
    pub expected_blocked_reason: Option<String>,
    pub spec_id: String,
    pub execution_id: String,
    pub outcome: ValidationOutcome,
    pub duration_ms: u64,
    pub detail: String,
}

/// Full result of a matrix run. Holds the fixture so its `.test` store stays
/// readable until the caller drops the run.
pub struct MatrixRun {
    pub records: Vec<CellRecord>,
    pub test_store_root: PathBuf,
    _fixture: LoadedFixture,
}

impl MatrixRun {
    /// Open the isolated test store the matrix recorded executions into.
    pub fn test_store(&self) -> TestStoreConfig {
        TestStoreConfig::new(self.test_store_root.clone(), "default")
    }
}

/// Materialize the fixture and run the full domain x operation matrix,
/// recording a [`ValidationExecution`] (with duration) for every cell.
pub fn run_matrix() -> Result<MatrixRun, FixtureError> {
    let fixture = materialize_fixture()?;
    let workspace_root = fixture.workspace_root.clone();
    let ctx = MatrixCtx {
        workspace_root: workspace_root.clone(),
    };

    let test_store_root = workspace_root.join(".test");
    let test_store = TestStoreConfig::new(test_store_root.clone(), "default");
    let run_id = format!("matrix-{}", Utc::now().format("%Y%m%dT%H%M%SZ"));

    // Keep run_one strict: only create paths initialize missing roots.
    bootstrap_core_store_roots(&ctx);

    let mut records = Vec::new();
    let domain_ops = domains();

    for cell in transport_cells() {
        if let Some(domain) = domain_ops
            .iter()
            .find(|candidate| candidate.domain() == cell.domain)
        {
            let record = run_cell(
                &test_store,
                &**domain,
                &cell,
                &ctx,
                &run_id,
            );
            records.push(record);
        } else {
            let detail = format!(
                "unknown domain `{}` for cell `{}`",
                cell.domain, cell.cell_id
            );
            let spec_id = format!(
                "vt-matrix-{}",
                cell.cell_id.replace('.', "-")
            );
            records.push(CellRecord {
                cell_id: cell.cell_id.clone(),
                domain: cell.domain.clone(),
                transport: cell.transport.clone(),
                operation: cell.operation.clone(),
                fixture_profile: cell.fixture_profile.clone(),
                expected_outcome: cell.expected_outcome.clone(),
                expected_blocked_reason: cell.blocked_reason.clone(),
                spec_id,
                execution_id: format!(
                    "exec-{run_id}-{}",
                    cell.cell_id.replace('.', "-")
                ),
                outcome: ValidationOutcome::Failed,
                duration_ms: 0,
                detail,
            });
        }
    }

    Ok(MatrixRun {
        records,
        test_store_root,
        _fixture: fixture,
    })
}

fn bootstrap_core_store_roots(ctx: &MatrixCtx) {
    let _ = ticket_api::storage::TicketStore::open_or_init(
        &ctx.store_root(".ticket"),
    );
    let _ = spec_api::SpecStore::open_or_init(&ctx.store_root(".spec"));
    let _ = rule_api::RuleStore::open_or_init(&ctx.store_root(".rule"));
}

/// Record the per-cell validation spec, run the cell, time it, and record the
/// execution. This is the fixed harness machinery - it never changes when a
/// domain or operation is added.
fn run_cell(
    test_store: &TestStoreConfig,
    domain: &dyn DomainOps,
    cell: &CellSpec,
    ctx: &MatrixCtx,
    run_id: &str,
) -> CellRecord {
    let cell_slug = cell.cell_id.replace('.', "-");
    let spec_id = format!("vt-matrix-{cell_slug}");
    let execution_id = format!("exec-{run_id}-{cell_slug}");

    let mut spec = ValidationSpec::new(
        spec_id.clone(),
        format!(
            "matrix: {} {} {}",
            cell.domain, cell.transport, cell.operation
        ),
    );
    spec.detail = Some(format!(
        "Cross-domain operation matrix cell `{}`",
        cell.cell_id
    ));
    spec.links.ticket_ids = vec![MATRIX_TICKET_ID.to_string()];
    spec.provenance = ValidationProvenance {
        source_path: Some(file!().to_string()),
        test_id: Some(cell.cell_id.clone()),
        domain: Some(cell.domain.clone()),
        operation: Some(cell.operation.clone()),
        transport: Some(cell.transport.clone()),
        run_id: Some(run_id.to_string()),
    };
    // Best-effort: spec recording failure should not abort the whole matrix.
    let _ = test_store.record_spec(&spec);

    let started = Instant::now();
    let result = dispatch(domain, &cell.transport, &cell.operation, ctx);
    let duration_ms = started.elapsed().as_millis() as u64;

    let (outcome, detail) = match result {
        Ok(Cell::Passed) => (
            ValidationOutcome::Passed,
            format!("{} passed", cell.cell_id),
        ),
        Ok(Cell::Blocked(reason)) => (ValidationOutcome::Blocked, reason),
        Err(reason) => (ValidationOutcome::Failed, reason),
    };

    let mut execution = ValidationExecution::new(
        execution_id.clone(),
        spec_id.clone(),
        outcome.clone(),
        Utc::now(),
    );
    execution.duration_ms = Some(duration_ms);
    execution.detail = Some(detail.clone());
    execution.links.spec_ids = vec![spec_id.clone()];
    execution.links.ticket_ids = vec![MATRIX_TICKET_ID.to_string()];
    execution.provenance = ValidationProvenance {
        source_path: Some(file!().to_string()),
        test_id: Some(cell.cell_id.clone()),
        domain: Some(cell.domain.clone()),
        operation: Some(cell.operation.clone()),
        transport: Some(cell.transport.clone()),
        run_id: Some(run_id.to_string()),
    };
    let _ = test_store.record_execution(&execution);

    CellRecord {
        cell_id: cell.cell_id.clone(),
        domain: cell.domain.clone(),
        transport: cell.transport.clone(),
        operation: cell.operation.clone(),
        fixture_profile: cell.fixture_profile.clone(),
        expected_outcome: cell.expected_outcome.clone(),
        expected_blocked_reason: cell.blocked_reason.clone(),
        spec_id,
        execution_id,
        outcome,
        duration_ms,
        detail,
    }
}
