use std::path::PathBuf;

use chrono::{
    DateTime,
    Utc,
};
use rmcp::{
    ErrorData as McpError,
    ServerHandler,
    ServiceExt,
    handler::server::{
        tool::ToolRouter,
        wrapper::Parameters,
    },
    model::*,
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
use test_api::{
    ExecutionQuery,
    ExecutionSort,
    TestError,
    TestStoreConfig,
    ValidationExecution,
    ValidationLinks,
    ValidationOutcome,
    ValidationProvenance,
    ValidationSpec,
};

// ── Input types ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecordSpecInput {
    /// Concrete workspace path, repo root, .test store path, or path inside that store. Do not use omitted, empty, 'default', '.', or '..' for entity creation.
    pub workspace: String,
    /// Stable validation spec id (e.g. `vt-core-tests`). Used as the file name.
    pub id: String,
    /// Human-readable title of the validation check.
    pub title: String,
    /// Command that performs the check (e.g. `cargo test -p ticket-vscode-core`).
    #[serde(default)]
    pub command: Option<String>,
    /// Additional detail or notes about the validation spec.
    #[serde(default)]
    pub detail: Option<String>,
    /// Slow-run budget threshold in milliseconds for this validation spec.
    #[serde(default)]
    pub slow_threshold_ms: Option<u64>,
    /// Ticket ids this spec is associated with.
    #[serde(default)]
    pub ticket_ids: Vec<String>,
    /// Related spec ids (spec-api) this validation spec covers.
    #[serde(default)]
    pub spec_ids: Vec<String>,
    /// Acceptance criterion ids this validation spec targets.
    #[serde(default)]
    pub acceptance_criterion_ids: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct RecordExecutionInput {
    /// Concrete workspace path, repo root, .test store path, or path inside that store. Do not use omitted, empty, 'default', '.', or '..' for entity creation.
    pub workspace: String,
    /// Stable execution id (e.g. `exec-vt-core-tests-20260615`). Used as file name.
    pub id: String,
    /// The validation spec id this execution is an outcome of.
    pub validation_spec_id: String,
    /// Outcome of the run: `passed`, `failed`, or `blocked`.
    pub outcome: String,
    /// RFC3339 timestamp of when the check ran. Defaults to now when omitted.
    #[serde(default)]
    pub executed_at: Option<String>,
    /// Wall time in milliseconds for the validated operation.
    #[serde(default)]
    pub duration_ms: Option<u64>,
    /// Optional throughput metric (ops/sec or items/sec).
    #[serde(default)]
    pub throughput: Option<f64>,
    /// Result summary or notes (command output highlights, blocker reason, etc.).
    #[serde(default)]
    pub detail: Option<String>,
    /// Ticket ids this execution provides evidence for.
    #[serde(default)]
    pub ticket_ids: Vec<String>,
    /// Related spec ids (spec-api) this execution provides evidence for.
    #[serde(default)]
    pub spec_ids: Vec<String>,
    /// Acceptance criterion ids this execution satisfies.
    #[serde(default)]
    pub acceptance_criterion_ids: Vec<String>,
    /// Doc-evidence ids linked to this execution.
    #[serde(default)]
    pub doc_evidence_ids: Vec<String>,
    /// Log ids linked to this execution.
    #[serde(default)]
    pub log_ids: Vec<String>,
    /// Source file path that produced this execution.
    #[serde(default)]
    pub source_path: Option<String>,
    /// Stable test/cell id inside source_path.
    #[serde(default)]
    pub test_id: Option<String>,
    /// Domain under test.
    #[serde(default)]
    pub domain: Option<String>,
    /// Operation under test.
    #[serde(default)]
    pub operation: Option<String>,
    /// Transport used (`cli`, `mcp`, `http`, `in-process`, ...).
    #[serde(default)]
    pub transport: Option<String>,
    /// Run id grouping executions from one harness invocation.
    #[serde(default)]
    pub run_id: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSpecInput {
    /// Validation spec id to fetch.
    pub id: String,
    /// Concrete workspace path, repo root, .test store path, or path inside
    /// that store. When omitted, searches every `.test` store discoverable
    /// from the server's workspace root.
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetExecutionInput {
    /// Validation execution id to fetch.
    pub id: String,
    /// Concrete workspace path, repo root, .test store path, or path inside
    /// that store. When omitted, searches every `.test` store discoverable
    /// from the server's workspace root.
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListSpecsInput {
    /// Concrete workspace path, repo root, .test store path, or path inside
    /// that store. When omitted, aggregates every `.test` store discoverable
    /// from the server's workspace root.
    #[serde(default)]
    pub workspace: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ListExecutionsInput {
    /// Concrete workspace path, repo root, .test store path, or path inside
    /// that store. When omitted, aggregates every `.test` store discoverable
    /// from the server's workspace root.
    #[serde(default)]
    pub workspace: Option<String>,
    /// Only return executions linked to this ticket id.
    #[serde(default)]
    pub ticket_id: Option<String>,
    /// Only return executions for this validation spec id.
    #[serde(default)]
    pub validation_spec_id: Option<String>,
    /// Only return executions with this outcome (`passed`, `failed`, `blocked`).
    #[serde(default)]
    pub outcome: Option<String>,
    /// Only return executions with duration >= this threshold.
    #[serde(default)]
    pub min_duration_ms: Option<u64>,
    /// Only return executions with duration <= this threshold.
    #[serde(default)]
    pub max_duration_ms: Option<u64>,
    /// Only return executions in this provenance domain.
    #[serde(default)]
    pub domain: Option<String>,
    /// Only return executions for this provenance operation.
    #[serde(default)]
    pub operation: Option<String>,
    /// Only return executions for this transport.
    #[serde(default)]
    pub transport: Option<String>,
    /// Only return executions for this run id.
    #[serde(default)]
    pub run_id: Option<String>,
    /// Sort order (`newest-first` or `slowest-first`).
    #[serde(default)]
    pub sort: Option<String>,
    /// Maximum number of executions to return (newest first).
    #[serde(default)]
    pub limit: Option<usize>,
}

// ── Server ───────────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct TestServer {
    store_root: PathBuf,
    workspace_slug: String,
    tool_router: ToolRouter<Self>,
}

impl TestServer {
    pub fn new(
        store_root: PathBuf,
        workspace_slug: String,
    ) -> Self {
        Self {
            store_root,
            workspace_slug,
            tool_router: Self::tool_router(),
        }
    }

    fn config(&self) -> TestStoreConfig {
        TestStoreConfig::new(
            self.store_root.clone(),
            self.workspace_slug.clone(),
        )
    }

    fn config_for_workspace(
        &self,
        workspace_selector: &str,
    ) -> Result<TestStoreConfig, McpError> {
        let workspace_selector =
            memory_kernel::workspace::validate_explicit_workspace_selector(Some(
                workspace_selector,
            ))
            .map_err(|err| McpError::invalid_params(err.to_string(), None))?;
        let store_root = memory_kernel::workspace::resolve_store_root_from(
            std::path::Path::new(workspace_selector),
            ".test",
        );
        Ok(TestStoreConfig::new(
            store_root,
            self.workspace_slug.clone(),
        ))
    }

    /// Every `.test` store discoverable from the server's workspace root,
    /// deduped, with the server's own fixed store listed first. Used by read
    /// tools when no explicit `workspace` argument is supplied, so evidence
    /// recorded to a nested store (e.g. `memory-api/.test`) via an explicit
    /// `workspace` on write remains discoverable from the aggregated root.
    fn discover_configs(&self) -> Vec<TestStoreConfig> {
        let workspace_root =
            memory_kernel::workspace::resolve_workspace_root_from_store_root(
                &self.store_root,
                ".test",
            );

        let mut seen = std::collections::BTreeSet::new();
        let mut configs = Vec::new();

        if seen.insert(self.store_root.clone()) {
            configs.push(self.config());
        }

        for root in memory_kernel::workspace::discover_workspace_store_roots(
            &workspace_root,
            ".test",
            "executions",
        ) {
            if seen.insert(root.clone()) {
                configs.push(TestStoreConfig::new(
                    root,
                    self.workspace_slug.clone(),
                ));
            }
        }

        configs
    }

    /// Resolve store configs for a read tool: a single explicit store when
    /// `workspace` is supplied, otherwise every discoverable store.
    fn configs_for_optional_workspace(
        &self,
        workspace: Option<&str>,
    ) -> Result<Vec<TestStoreConfig>, McpError> {
        match workspace {
            Some(selector) => Ok(vec![self.config_for_workspace(selector)?]),
            None => Ok(self.discover_configs()),
        }
    }

    /// Merge-sort executions gathered from multiple stores using the same
    /// ordering as `TestStoreConfig::list_executions`.
    fn merge_sort_executions(
        mut executions: Vec<ValidationExecution>,
        sort: ExecutionSort,
    ) -> Vec<ValidationExecution> {
        match sort {
            ExecutionSort::NewestFirst => {
                executions.sort_by(|a, b| {
                    b.executed_at.cmp(&a.executed_at).then(a.id.cmp(&b.id))
                });
            },
            ExecutionSort::SlowestFirst => {
                executions.sort_by(|a, b| {
                    b.duration_ms
                        .cmp(&a.duration_ms)
                        .then(b.executed_at.cmp(&a.executed_at))
                        .then(a.id.cmp(&b.id))
                });
            },
        }
        executions
    }

    fn json_result<T: Serialize>(
        value: &T
    ) -> Result<CallToolResult, McpError> {
        let text = serde_json::to_string(value).map_err(|err| {
            McpError::internal_error(format!("serialization: {err}"), None)
        })?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    fn test_err(err: TestError) -> McpError {
        match &err {
            TestError::EmptyRoot
            | TestError::InvalidId(_)
            | TestError::InvalidWorkspaceSlug(_)
            | TestError::SpecNotFound(_)
            | TestError::ExecutionNotFound(_) =>
                McpError::invalid_params(err.to_string(), None),
            _ => McpError::internal_error(format!("test error: {err}"), None),
        }
    }

    fn parse_outcome(raw: &str) -> Result<ValidationOutcome, McpError> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "passed" | "pass" => Ok(ValidationOutcome::Passed),
            "failed" | "fail" => Ok(ValidationOutcome::Failed),
            "blocked" | "block" => Ok(ValidationOutcome::Blocked),
            other => Err(McpError::invalid_params(
                format!(
                    "invalid outcome `{other}` (expected passed, failed, or blocked)"
                ),
                None,
            )),
        }
    }

    fn parse_timestamp(raw: Option<&str>) -> Result<DateTime<Utc>, McpError> {
        match raw {
            None => Ok(Utc::now()),
            Some(value) => {
                let trimmed = value.trim();
                if trimmed.is_empty() {
                    return Ok(Utc::now());
                }
                DateTime::parse_from_rfc3339(trimmed)
                    .map(|dt| dt.with_timezone(&Utc))
                    .map_err(|err| {
                        McpError::invalid_params(
                            format!("invalid executed_at `{trimmed}`: {err} (expected RFC3339)"),
                            None,
                        )
                    })
            },
        }
    }

    fn parse_sort(raw: Option<&str>) -> Result<ExecutionSort, McpError> {
        match raw {
            None => Ok(ExecutionSort::NewestFirst),
            Some(value) => match value.trim().to_ascii_lowercase().as_str() {
                "newest-first" | "newest" => Ok(ExecutionSort::NewestFirst),
                "slowest-first" | "slowest" => Ok(ExecutionSort::SlowestFirst),
                other => Err(McpError::invalid_params(
                    format!(
                        "invalid sort `{other}` (expected newest-first or slowest-first)"
                    ),
                    None,
                )),
            },
        }
    }
}

// ── Tool implementations ──────────────────────────────────────────────────────

#[tool_router]
impl TestServer {
    #[tool(
        name = "test_record_spec",
        description = "Record (create or overwrite) a validation spec describing a test or check to run."
    )]
    pub async fn test_record_spec(
        &self,
        Parameters(input): Parameters<RecordSpecInput>,
    ) -> Result<CallToolResult, McpError> {
        let spec = ValidationSpec {
            id: input.id,
            title: input.title,
            command: input.command,
            detail: input.detail,
            slow_threshold_ms: input.slow_threshold_ms,
            links: ValidationLinks {
                spec_ids: input.spec_ids,
                acceptance_criterion_ids: input.acceptance_criterion_ids,
                ticket_ids: input.ticket_ids,
                doc_evidence_ids: Vec::new(),
                log_ids: Vec::new(),
            },
            provenance: ValidationProvenance::default(),
        };
        let path = self
            .config_for_workspace(&input.workspace)?
            .record_spec(&spec)
            .map_err(Self::test_err)?;
        Self::json_result(&serde_json::json!({
            "status": "recorded",
            "kind": "validation-spec",
            "id": spec.id,
            "path": path.to_string_lossy(),
        }))
    }

    #[tool(
        name = "test_record_execution",
        description = "Record (create or overwrite) a validation execution capturing the outcome of a check, linked to tickets/specs/criteria."
    )]
    pub async fn test_record_execution(
        &self,
        Parameters(input): Parameters<RecordExecutionInput>,
    ) -> Result<CallToolResult, McpError> {
        let outcome = Self::parse_outcome(&input.outcome)?;
        let executed_at = Self::parse_timestamp(input.executed_at.as_deref())?;
        let execution = ValidationExecution {
            id: input.id,
            validation_spec_id: input.validation_spec_id,
            outcome,
            executed_at,
            duration_ms: input.duration_ms,
            throughput: input.throughput,
            detail: input.detail,
            links: ValidationLinks {
                spec_ids: input.spec_ids,
                acceptance_criterion_ids: input.acceptance_criterion_ids,
                ticket_ids: input.ticket_ids,
                doc_evidence_ids: input.doc_evidence_ids,
                log_ids: input.log_ids,
            },
            provenance: ValidationProvenance {
                source_path: input.source_path,
                test_id: input.test_id,
                domain: input.domain,
                operation: input.operation,
                transport: input.transport,
                run_id: input.run_id,
            },
        };
        let path = self
            .config_for_workspace(&input.workspace)?
            .record_execution(&execution)
            .map_err(Self::test_err)?;
        Self::json_result(&serde_json::json!({
            "status": "recorded",
            "kind": "validation-execution",
            "id": execution.id,
            "outcome": execution.outcome,
            "path": path.to_string_lossy(),
        }))
    }

    #[tool(
        name = "test_get_spec",
        description = "Fetch a single validation spec by id."
    )]
    pub async fn test_get_spec(
        &self,
        Parameters(input): Parameters<GetSpecInput>,
    ) -> Result<CallToolResult, McpError> {
        let configs =
            self.configs_for_optional_workspace(input.workspace.as_deref())?;
        let mut last_err = TestError::SpecNotFound(input.id.clone());
        for config in &configs {
            match config.get_spec(&input.id) {
                Ok(spec) => return Self::json_result(&spec),
                Err(err) => last_err = err,
            }
        }
        Err(Self::test_err(last_err))
    }

    #[tool(
        name = "test_get_execution",
        description = "Fetch a single validation execution by id."
    )]
    pub async fn test_get_execution(
        &self,
        Parameters(input): Parameters<GetExecutionInput>,
    ) -> Result<CallToolResult, McpError> {
        let configs =
            self.configs_for_optional_workspace(input.workspace.as_deref())?;
        let mut last_err = TestError::ExecutionNotFound(input.id.clone());
        for config in &configs {
            match config.get_execution(&input.id) {
                Ok(execution) => return Self::json_result(&execution),
                Err(err) => last_err = err,
            }
        }
        Err(Self::test_err(last_err))
    }

    #[tool(
        name = "test_list_specs",
        description = "List all validation specs, sorted by id. Aggregates across every discoverable `.test` store unless `workspace` pins to one."
    )]
    pub async fn test_list_specs(
        &self,
        Parameters(input): Parameters<ListSpecsInput>,
    ) -> Result<CallToolResult, McpError> {
        let configs =
            self.configs_for_optional_workspace(input.workspace.as_deref())?;
        let mut by_id: std::collections::BTreeMap<String, ValidationSpec> =
            std::collections::BTreeMap::new();
        for config in &configs {
            for spec in config.list_specs().map_err(Self::test_err)? {
                by_id.entry(spec.id.clone()).or_insert(spec);
            }
        }
        let specs: Vec<_> = by_id.into_values().collect();
        Self::json_result(&serde_json::json!({
            "count": specs.len(),
            "specs": specs,
        }))
    }

    #[tool(
        name = "test_list_executions",
        description = "Query validation executions by ticket id, validation spec id, and/or outcome (newest first). Aggregates across every discoverable `.test` store unless `workspace` pins to one."
    )]
    pub async fn test_list_executions(
        &self,
        Parameters(input): Parameters<ListExecutionsInput>,
    ) -> Result<CallToolResult, McpError> {
        let outcome = match input.outcome.as_deref() {
            Some(raw) => Some(Self::parse_outcome(raw)?),
            None => None,
        };
        let sort = Self::parse_sort(input.sort.as_deref())?;
        let query = ExecutionQuery {
            ticket_id: input.ticket_id,
            validation_spec_id: input.validation_spec_id,
            outcome,
            min_duration_ms: input.min_duration_ms,
            max_duration_ms: input.max_duration_ms,
            domain: input.domain,
            operation: input.operation,
            transport: input.transport,
            run_id: input.run_id,
            sort,
            limit: None,
        };
        let configs =
            self.configs_for_optional_workspace(input.workspace.as_deref())?;
        let mut seen_ids = std::collections::BTreeSet::new();
        let mut executions = Vec::new();
        for config in &configs {
            for execution in
                config.list_executions(&query).map_err(Self::test_err)?
            {
                if seen_ids.insert(execution.id.clone()) {
                    executions.push(execution);
                }
            }
        }
        let mut executions = Self::merge_sort_executions(executions, sort);
        if let Some(limit) = input.limit {
            executions.truncate(limit);
        }
        Self::json_result(&serde_json::json!({
            "count": executions.len(),
            "executions": executions,
        }))
    }
}

// ── MCP handler trait ─────────────────────────────────────────────────────────

#[tool_handler]
impl ServerHandler for TestServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            server_info: Implementation {
                name: env!("CARGO_PKG_NAME").to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                ..Default::default()
            },
            instructions: Some(
                "test-mcp provides direct access to the test-result store (test-api). Use named \
                 tools to record validation specs and executions, fetch them by id, and query \
                 executions by ticket/spec/outcome. Link executions to tickets via ticket_ids so \
                 tickets can reference stored evidence instead of inlining results."
                    .to_string(),
            ),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

// ── Server startup ────────────────────────────────────────────────────────────

pub async fn run_mcp_server(
    store_root: PathBuf,
    workspace_slug: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server = TestServer::new(store_root, workspace_slug);

    tracing::info!("Starting test-mcp server on stdio (direct store access)");

    let service = server.serve(stdio()).await.inspect_err(|err| {
        eprintln!("Server error: {err:?}");
    })?;

    service.waiting().await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[tokio::test]
    async fn record_spec_then_execution_and_query() {
        let dir = tempdir().unwrap();
        let store_root = dir.path().join(".test");
        let server = TestServer::new(store_root.clone(), "default".to_string());

        let spec = server
            .test_record_spec(Parameters(RecordSpecInput {
                workspace: store_root.display().to_string(),
                id: "vt-core-tests".to_string(),
                title: "Core unit tests".to_string(),
                command: Some("cargo test -p ticket-vscode-core".to_string()),
                detail: None,
                slow_threshold_ms: Some(1000),
                ticket_ids: vec!["ticket-parity".to_string()],
                spec_ids: vec![],
                acceptance_criterion_ids: vec![],
            }))
            .await
            .expect("record spec");
        assert!(!spec.is_error.unwrap_or(false));

        let exec = server
            .test_record_execution(Parameters(RecordExecutionInput {
                workspace: store_root.display().to_string(),
                id: "exec-1".to_string(),
                validation_spec_id: "vt-core-tests".to_string(),
                outcome: "passed".to_string(),
                executed_at: Some("2026-06-15T00:00:00Z".to_string()),
                duration_ms: Some(240),
                throughput: Some(4.2),
                detail: Some("16 passed".to_string()),
                ticket_ids: vec!["ticket-parity".to_string()],
                spec_ids: vec![],
                acceptance_criterion_ids: vec![],
                doc_evidence_ids: vec![],
                log_ids: vec![],
                source_path: Some(
                    "crates/memory-matrix/tests/matrix.rs".to_string(),
                ),
                test_id: Some("ticket.get".to_string()),
                domain: Some("ticket".to_string()),
                operation: Some("get".to_string()),
                transport: Some("in-process".to_string()),
                run_id: Some("run-1".to_string()),
            }))
            .await
            .expect("record execution");
        assert!(!exec.is_error.unwrap_or(false));

        let listed = server
            .test_list_executions(Parameters(ListExecutionsInput {
                workspace: None,
                ticket_id: Some("ticket-parity".to_string()),
                validation_spec_id: None,
                outcome: None,
                min_duration_ms: None,
                max_duration_ms: None,
                domain: Some("ticket".to_string()),
                operation: Some("get".to_string()),
                transport: Some("in-process".to_string()),
                run_id: Some("run-1".to_string()),
                sort: Some("slowest-first".to_string()),
                limit: None,
            }))
            .await
            .expect("list executions");
        assert!(!listed.is_error.unwrap_or(false));

        let specs = server
            .test_list_specs(Parameters(ListSpecsInput { workspace: None }))
            .await
            .expect("list specs");
        assert!(!specs.is_error.unwrap_or(false));
    }

    #[tokio::test]
    async fn invalid_outcome_is_rejected() {
        let dir = tempdir().unwrap();
        let store_root = dir.path().join(".test");
        let server = TestServer::new(store_root.clone(), "default".to_string());

        let result = server
            .test_record_execution(Parameters(RecordExecutionInput {
                workspace: store_root.display().to_string(),
                id: "exec-bad".to_string(),
                validation_spec_id: "vt-core-tests".to_string(),
                outcome: "nope".to_string(),
                executed_at: None,
                duration_ms: None,
                throughput: None,
                detail: None,
                ticket_ids: vec![],
                spec_ids: vec![],
                acceptance_criterion_ids: vec![],
                doc_evidence_ids: vec![],
                log_ids: vec![],
                source_path: None,
                test_id: None,
                domain: None,
                operation: None,
                transport: None,
                run_id: None,
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn missing_spec_reports_not_found() {
        let dir = tempdir().unwrap();
        let store_root = dir.path().join(".test");
        let server = TestServer::new(store_root, "default".to_string());

        let result = server
            .test_get_spec(Parameters(GetSpecInput {
                id: "missing".to_string(),
                workspace: None,
            }))
            .await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn list_executions_aggregates_nested_descendant_store() {
        let dir = tempdir().unwrap();
        let workspace_root = dir.path();
        let root_store = workspace_root.join(".test");
        let nested_workspace = workspace_root.join("nested");
        std::fs::create_dir_all(&nested_workspace).unwrap();

        // Server launched with its fixed root at the workspace root.
        let server = TestServer::new(root_store.clone(), "default".to_string());

        // Record straight into the root store.
        server
            .test_record_execution(Parameters(RecordExecutionInput {
                workspace: root_store.display().to_string(),
                id: "exec-root".to_string(),
                validation_spec_id: "vt-root".to_string(),
                outcome: "passed".to_string(),
                executed_at: Some("2026-07-13T00:00:00Z".to_string()),
                duration_ms: None,
                throughput: None,
                detail: None,
                ticket_ids: vec!["ticket-x".to_string()],
                spec_ids: vec![],
                acceptance_criterion_ids: vec![],
                doc_evidence_ids: vec![],
                log_ids: vec![],
                source_path: None,
                test_id: None,
                domain: Some("test-mcp".to_string()),
                operation: Some("aggregation-test".to_string()),
                transport: Some("in-process".to_string()),
                run_id: Some("run-root".to_string()),
            }))
            .await
            .expect("record root execution");

        // Record into a nested descendant store via an explicit workspace,
        // mirroring how a submodule-scoped caller records evidence today.
        let nested_store = nested_workspace.join(".test");
        server
            .test_record_execution(Parameters(RecordExecutionInput {
                workspace: nested_store.display().to_string(),
                id: "exec-nested".to_string(),
                validation_spec_id: "vt-nested".to_string(),
                outcome: "passed".to_string(),
                executed_at: Some("2026-07-14T00:00:00Z".to_string()),
                duration_ms: None,
                throughput: None,
                detail: None,
                ticket_ids: vec!["ticket-x".to_string()],
                spec_ids: vec![],
                acceptance_criterion_ids: vec![],
                doc_evidence_ids: vec![],
                log_ids: vec![],
                source_path: None,
                test_id: None,
                domain: Some("test-mcp".to_string()),
                operation: Some("aggregation-test".to_string()),
                transport: Some("in-process".to_string()),
                run_id: Some("run-nested".to_string()),
            }))
            .await
            .expect("record nested execution");

        assert!(nested_workspace.join(".test").is_dir());

        // Reading with no explicit workspace must aggregate both stores.
        let result = server
            .test_list_executions(Parameters(ListExecutionsInput {
                workspace: None,
                ticket_id: Some("ticket-x".to_string()),
                validation_spec_id: None,
                outcome: None,
                min_duration_ms: None,
                max_duration_ms: None,
                domain: None,
                operation: None,
                transport: None,
                run_id: None,
                sort: Some("newest-first".to_string()),
                limit: None,
            }))
            .await
            .expect("list executions");
        let text = result.content[0].as_text().unwrap().text.clone();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["count"], 2);
        let ids: Vec<&str> = parsed["executions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|exec| exec["id"].as_str().unwrap())
            .collect();
        // newest-first: exec-nested (2026-07-14) before exec-root (2026-07-13).
        assert_eq!(ids, vec!["exec-nested", "exec-root"]);

        // A limit is applied after the global merge, not per store.
        let limited = server
            .test_list_executions(Parameters(ListExecutionsInput {
                workspace: None,
                ticket_id: Some("ticket-x".to_string()),
                validation_spec_id: None,
                outcome: None,
                min_duration_ms: None,
                max_duration_ms: None,
                domain: None,
                operation: None,
                transport: None,
                run_id: None,
                sort: Some("newest-first".to_string()),
                limit: Some(1),
            }))
            .await
            .expect("list executions limited");
        let text = limited.content[0].as_text().unwrap().text.clone();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["executions"][0]["id"], "exec-nested");

        // Pinning to the nested workspace explicitly must exclude the root.
        let pinned = server
            .test_list_executions(Parameters(ListExecutionsInput {
                workspace: Some(nested_workspace.display().to_string()),
                ticket_id: Some("ticket-x".to_string()),
                validation_spec_id: None,
                outcome: None,
                min_duration_ms: None,
                max_duration_ms: None,
                domain: None,
                operation: None,
                transport: None,
                run_id: None,
                sort: None,
                limit: None,
            }))
            .await
            .expect("list executions pinned");
        let text = pinned.content[0].as_text().unwrap().text.clone();
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["count"], 1);
        assert_eq!(parsed["executions"][0]["id"], "exec-nested");

        // get_execution with no workspace must also find the nested record.
        let fetched = server
            .test_get_execution(Parameters(GetExecutionInput {
                id: "exec-nested".to_string(),
                workspace: None,
            }))
            .await
            .expect("get nested execution");
        assert!(!fetched.is_error.unwrap_or(false));
    }
}
