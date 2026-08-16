use crate::{
    BrowserFrontendPort,
    BrowserFrontendProbe,
    StartupProbe,
    StartupProbeError,
    StdioServerProbe,
    empty_workspace,
};
use serde::Serialize;
use std::{
    ffi::OsString,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartupMatrixClass {
    McpServer,
    Viewer,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartupMatrixOutcome {
    CleanStart,
    StoreRefusal,
    PollutionDetected,
    UnexpectedFailure,
}

#[derive(Debug, Serialize)]
pub struct StartupMatrixResult {
    pub tool: &'static str,
    pub class: StartupMatrixClass,
    pub outcome: StartupMatrixOutcome,
    pub created_paths: Vec<String>,
    pub removed_paths: Vec<String>,
    pub changed_paths: Vec<String>,
    pub detail: Option<String>,
}

struct ServerDescriptor {
    name: &'static str,
    package: &'static str,
    binary: &'static str,
    unset_env: &'static [&'static str],
    child: Option<(&'static str, &'static str)>,
    outcome: ExpectedOutcome,
}

struct ViewerDescriptor {
    name: &'static str,
    package: &'static str,
    binary: &'static str,
    unset_env: &'static [&'static str],
    port: BrowserFrontendPort,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExpectedOutcome {
    Initializes,
    RefusesWithoutStore,
}

const SERVERS: &[ServerDescriptor] = &[
    ServerDescriptor {
        name: "mcp-toolmon",
        package: "mcp-toolmon",
        binary: "mcp-toolmon",
        unset_env: &["COST_GATE_TELEMETRY_LOG"],
        child: Some(("fs-mcp", "fs-mcp")),
        outcome: ExpectedOutcome::Initializes,
    },
    ServerDescriptor {
        name: "context-mcp",
        package: "context-mcp",
        binary: "context-mcp",
        unset_env: &[],
        child: None,
        outcome: ExpectedOutcome::Initializes,
    },
    ServerDescriptor {
        name: "ticket-mcp",
        package: "ticket-mcp",
        binary: "ticket-mcp",
        unset_env: &["TICKET_INDEX_ROOT"],
        child: None,
        outcome: ExpectedOutcome::RefusesWithoutStore,
    },
    ServerDescriptor {
        name: "spec-mcp",
        package: "spec-mcp",
        binary: "spec-mcp",
        unset_env: &["SPEC_INDEX_ROOT", "TICKET_INDEX_ROOT"],
        child: None,
        outcome: ExpectedOutcome::RefusesWithoutStore,
    },
    ServerDescriptor {
        name: "test-mcp",
        package: "test-mcp",
        binary: "test-mcp",
        unset_env: &["TEST_STORE_ROOT"],
        child: None,
        outcome: ExpectedOutcome::Initializes,
    },
    ServerDescriptor {
        name: "feedback-mcp",
        package: "feedback-mcp",
        binary: "feedback-mcp",
        unset_env: &["FEEDBACK_STORE_ROOT"],
        child: None,
        outcome: ExpectedOutcome::Initializes,
    },
    ServerDescriptor {
        name: "session-mcp",
        package: "session-mcp",
        binary: "session-mcp",
        unset_env: &["SESSION_STORE_ROOT"],
        child: None,
        outcome: ExpectedOutcome::Initializes,
    },
    ServerDescriptor {
        name: "rule-mcp",
        package: "rule-mcp",
        binary: "rule-mcp",
        unset_env: &["RULE_INDEX_ROOT", "TICKET_INDEX_ROOT"],
        child: None,
        outcome: ExpectedOutcome::RefusesWithoutStore,
    },
    ServerDescriptor {
        name: "audit-mcp",
        package: "audit-mcp",
        binary: "audit-mcp",
        unset_env: &[],
        child: None,
        outcome: ExpectedOutcome::Initializes,
    },
    ServerDescriptor {
        name: "fs-mcp",
        package: "fs-mcp",
        binary: "fs-mcp",
        unset_env: &[],
        child: None,
        outcome: ExpectedOutcome::Initializes,
    },
    ServerDescriptor {
        name: "peek-mcp",
        package: "peek-mcp",
        binary: "peek-mcp",
        unset_env: &[],
        child: None,
        outcome: ExpectedOutcome::Initializes,
    },
    ServerDescriptor {
        name: "compact-terminal-mcp",
        package: "compact-terminal-mcp",
        binary: "compact-terminal-mcp",
        unset_env: &["COMPACT_TERMINAL_SPILL_DIR"],
        child: None,
        outcome: ExpectedOutcome::Initializes,
    },
];

const VIEWERS: &[ViewerDescriptor] = &[
    ViewerDescriptor {
        name: "doc-viewer",
        package: "doc-viewer",
        binary: "doc-viewer",
        unset_env: &["LOG_DIR", "AGENTS_DIR", "PORT"],
        port: BrowserFrontendPort::Environment("PORT"),
    },
    ViewerDescriptor {
        name: "log-viewer",
        package: "log-viewer",
        binary: "log-viewer",
        unset_env: &["LOG_DIR", "LOG_VIEWER_CONFIG"],
        port: BrowserFrontendPort::LogViewerConfig,
    },
    ViewerDescriptor {
        name: "ticket-viewer",
        package: "ticket-viewer",
        binary: "ticket-viewer",
        unset_env: &["PORT", "TICKET_INDEX_ROOT"],
        port: BrowserFrontendPort::Argument("--port"),
    },
    ViewerDescriptor {
        name: "spec-viewer",
        package: "spec-viewer",
        binary: "spec-viewer",
        unset_env: &["LOG_DIR", "PORT", "SPEC_INDEX_ROOT"],
        port: BrowserFrontendPort::Argument("--port"),
    },
];

pub fn run_startup_matrix() -> Result<Vec<StartupMatrixResult>, String> {
    run_startup_matrix_for(None)
}

pub fn run_startup_matrix_for(
    class: Option<StartupMatrixClass>
) -> Result<Vec<StartupMatrixResult>, String> {
    let workspace_root = workspace_root();
    let target_dir = cargo_target_dir(&workspace_root)?;
    let mut results = Vec::new();

    if class.is_none() || class == Some(StartupMatrixClass::McpServer) {
        for server in SERVERS {
            results.push(run_server(&workspace_root, &target_dir, server));
        }
    }
    if class.is_none() || class == Some(StartupMatrixClass::Viewer) {
        for viewer in VIEWERS {
            results.push(run_viewer(&workspace_root, &target_dir, viewer));
        }
    }
    Ok(results)
}

pub fn startup_matrix_succeeded(results: &[StartupMatrixResult]) -> bool {
    results.iter().all(|result| {
        matches!(
            result.outcome,
            StartupMatrixOutcome::CleanStart
                | StartupMatrixOutcome::StoreRefusal
        )
    })
}

fn run_server(
    workspace_root: &Path,
    target_dir: &Path,
    server: &ServerDescriptor,
) -> StartupMatrixResult {
    if let Err(detail) =
        build_binary(workspace_root, server.package, server.binary)
    {
        return failure(server.name, StartupMatrixClass::McpServer, detail);
    }
    if let Some((package, binary)) = server.child {
        if let Err(detail) = build_binary(workspace_root, package, binary) {
            return failure(server.name, StartupMatrixClass::McpServer, detail);
        }
    }

    let mut args = Vec::new();
    if let Some((_, child_binary)) = server.child {
        args.push(OsString::from("--"));
        args.push(binary_path(target_dir, child_binary).into_os_string());
    }
    let probe = StartupProbe::StdioServer(StdioServerProbe {
        executable: binary_path(target_dir, server.binary),
        args,
        unset_env: server.unset_env.iter().map(OsString::from).collect(),
    });
    let fixture = match empty_workspace() {
        Ok(fixture) => fixture,
        Err(error) =>
            return failure(
                server.name,
                StartupMatrixClass::McpServer,
                error.to_string(),
            ),
    };

    match probe.run(&fixture) {
        Ok(report)
            if report.delta.is_empty()
                && server.outcome == ExpectedOutcome::Initializes =>
            success(
                server.name,
                StartupMatrixClass::McpServer,
                StartupMatrixOutcome::CleanStart,
            ),
        Ok(report)
            if server.outcome == ExpectedOutcome::RefusesWithoutStore =>
            from_delta(
                server.name,
                StartupMatrixClass::McpServer,
                &report.delta,
                StartupMatrixOutcome::UnexpectedFailure,
                Some(
                    "expected store refusal but MCP initialize succeeded"
                        .to_string(),
                ),
            ),
        Ok(report) => from_delta(
            server.name,
            StartupMatrixClass::McpServer,
            &report.delta,
            StartupMatrixOutcome::PollutionDetected,
            Some(
                "MCP server initialized with a non-empty workspace delta"
                    .to_string(),
            ),
        ),
        Err(StartupProbeError::Initialize { evidence, .. })
            if server.outcome == ExpectedOutcome::RefusesWithoutStore
                && evidence.delta.is_empty()
                && !evidence.stderr.trim().is_empty() =>
            success(
                server.name,
                StartupMatrixClass::McpServer,
                StartupMatrixOutcome::StoreRefusal,
            ),
        Err(error) =>
            from_probe_error(server.name, StartupMatrixClass::McpServer, error),
    }
}

fn run_viewer(
    workspace_root: &Path,
    target_dir: &Path,
    viewer: &ViewerDescriptor,
) -> StartupMatrixResult {
    if let Err(detail) =
        build_binary(workspace_root, viewer.package, viewer.binary)
    {
        return failure(viewer.name, StartupMatrixClass::Viewer, detail);
    }
    let fixture = match empty_workspace() {
        Ok(fixture) => fixture,
        Err(error) =>
            return failure(
                viewer.name,
                StartupMatrixClass::Viewer,
                error.to_string(),
            ),
    };
    let probe = StartupProbe::BrowserFrontend(BrowserFrontendProbe {
        tool: viewer.name,
        executable: binary_path(target_dir, viewer.binary),
        args: Vec::new(),
        unset_env: viewer.unset_env.iter().map(OsString::from).collect(),
        port: viewer.port,
    });

    match probe.run(&fixture) {
        Ok(report) if report.delta.is_empty() => success(
            viewer.name,
            StartupMatrixClass::Viewer,
            StartupMatrixOutcome::CleanStart,
        ),
        Ok(report) => from_delta(
            viewer.name,
            StartupMatrixClass::Viewer,
            &report.delta,
            StartupMatrixOutcome::PollutionDetected,
            Some(viewer_pollution_detail(&report.delta)),
        ),
        Err(StartupProbeError::BrowserFrontendUnavailable {
            evidence, ..
        }) if evidence.delta.is_empty()
            && !evidence.stderr.trim().is_empty() =>
            success(
                viewer.name,
                StartupMatrixClass::Viewer,
                StartupMatrixOutcome::StoreRefusal,
            ),
        Err(error) =>
            from_probe_error(viewer.name, StartupMatrixClass::Viewer, error),
    }
}

fn from_probe_error(
    tool: &'static str,
    class: StartupMatrixClass,
    error: StartupProbeError,
) -> StartupMatrixResult {
    match error {
        StartupProbeError::Initialize {
            detail, evidence, ..
        }
        | StartupProbeError::BrowserFrontendUnavailable {
            detail,
            evidence,
            ..
        } => from_delta(
            tool,
            class,
            &evidence.delta,
            if evidence.delta.is_empty() {
                StartupMatrixOutcome::UnexpectedFailure
            } else {
                StartupMatrixOutcome::PollutionDetected
            },
            Some(detail),
        ),
        StartupProbeError::ForcedTermination { evidence, .. } => from_delta(
            tool,
            class,
            &evidence.delta,
            if evidence.delta.is_empty() {
                StartupMatrixOutcome::UnexpectedFailure
            } else {
                StartupMatrixOutcome::PollutionDetected
            },
            Some("startup probe required forced termination".to_string()),
        ),
        error => failure(tool, class, error.to_string()),
    }
}

fn viewer_pollution_detail(delta: &crate::WorkspaceDelta) -> String {
    if delta
        .added
        .iter()
        .any(|path| path.starts_with(Path::new("target/test-logs")))
    {
        "ClientLogState::with_path must remain lazy: an eager ClientLogState::default() during router construction recreated target/test-logs/".to_string()
    } else {
        "viewer startup must not create stores or logs".to_string()
    }
}

fn success(
    tool: &'static str,
    class: StartupMatrixClass,
    outcome: StartupMatrixOutcome,
) -> StartupMatrixResult {
    StartupMatrixResult {
        tool,
        class,
        outcome,
        created_paths: Vec::new(),
        removed_paths: Vec::new(),
        changed_paths: Vec::new(),
        detail: None,
    }
}

fn failure(
    tool: &'static str,
    class: StartupMatrixClass,
    detail: String,
) -> StartupMatrixResult {
    StartupMatrixResult {
        tool,
        class,
        outcome: StartupMatrixOutcome::UnexpectedFailure,
        created_paths: Vec::new(),
        removed_paths: Vec::new(),
        changed_paths: Vec::new(),
        detail: Some(detail),
    }
}

fn from_delta(
    tool: &'static str,
    class: StartupMatrixClass,
    delta: &crate::WorkspaceDelta,
    outcome: StartupMatrixOutcome,
    detail: Option<String>,
) -> StartupMatrixResult {
    StartupMatrixResult {
        tool,
        class,
        outcome,
        created_paths: delta
            .added
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        removed_paths: delta
            .removed
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        changed_paths: delta
            .changed
            .iter()
            .map(|path| path.display().to_string())
            .collect(),
        detail,
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("memory-fixtures must remain under the workspace root")
        .to_path_buf()
}

fn cargo_target_dir(workspace_root: &Path) -> Result<PathBuf, String> {
    let output = Command::new("cargo")
        .current_dir(workspace_root)
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .output()
        .map_err(|error| format!("run cargo metadata: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("parse cargo metadata: {error}"))?;
    metadata["target_directory"]
        .as_str()
        .map(PathBuf::from)
        .ok_or_else(|| {
            "cargo metadata did not report target_directory".to_string()
        })
}

fn build_binary(
    workspace_root: &Path,
    package: &str,
    binary: &str,
) -> Result<(), String> {
    let status = Command::new("cargo")
        .current_dir(workspace_root)
        .args(["build", "--package", package, "--bin", binary])
        .status()
        .map_err(|error| format!("spawn cargo build for {package}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("cargo build failed for {package} ({binary})"))
    }
}

fn binary_path(
    target_dir: &Path,
    binary: &str,
) -> PathBuf {
    target_dir
        .join("debug")
        .join(format!("{binary}{}", std::env::consts::EXE_SUFFIX))
}
