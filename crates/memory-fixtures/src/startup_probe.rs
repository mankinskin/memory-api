use crate::{
    EmptyWorkspace,
    FixtureError,
    WorkspaceDelta,
    WorkspaceSnapshot,
};
use std::{
    ffi::OsString,
    io::{
        BufRead,
        BufReader,
        Read,
        Write,
    },
    net::{
        TcpListener,
        TcpStream,
    },
    path::PathBuf,
    process::{
        Command,
        Stdio,
    },
    sync::mpsc,
    thread,
    time::{
        Duration,
        Instant,
    },
};

const INITIALIZE_TIMEOUT: Duration = Duration::from_secs(5);
const HTTP_READY_TIMEOUT: Duration = Duration::from_secs(5);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProbeClass {
    StdioProtocolServer,
    BrowserFrontendViewer,
}

#[derive(Debug, Clone)]
pub struct StdioServerProbe {
    pub executable: PathBuf,
    pub args: Vec<OsString>,
    pub unset_env: Vec<OsString>,
}

#[derive(Debug, Clone)]
pub struct BrowserFrontendProbe {
    pub tool: &'static str,
    pub executable: PathBuf,
    pub args: Vec<OsString>,
    pub unset_env: Vec<OsString>,
    pub port: BrowserFrontendPort,
}

#[derive(Debug, Clone, Copy)]
pub enum BrowserFrontendPort {
    Environment(&'static str),
    Argument(&'static str),
    LogViewerConfig,
}

#[derive(Debug, Clone)]
pub enum StartupProbe {
    StdioServer(StdioServerProbe),
    BrowserFrontend(BrowserFrontendProbe),
}

#[derive(Debug, Clone)]
pub struct StartupProbeReport {
    pub class: ProbeClass,
    pub delta: WorkspaceDelta,
}

#[derive(Debug, Clone)]
pub struct StartupFailureEvidence {
    pub after: WorkspaceSnapshot,
    pub delta: WorkspaceDelta,
    pub exit_code: Option<i32>,
    pub stderr: String,
}

#[derive(Debug, thiserror::Error)]
pub enum StartupProbeError {
    #[error("fixture setup failed: {0}")]
    Fixture(#[from] FixtureError),
    #[error("failed to start {tool}: {source}")]
    Spawn {
        tool: String,
        source: std::io::Error,
    },
    #[error(
        "{tool} did not complete the MCP initialize handshake within {timeout:?}: {detail}"
    )]
    Initialize {
        tool: String,
        timeout: Duration,
        detail: String,
        evidence: StartupFailureEvidence,
    },
    #[error("{tool} did not stop after stdin closed and required termination")]
    ForcedTermination {
        tool: String,
        evidence: StartupFailureEvidence,
    },
    #[error(
        "{tool} did not serve an HTTP response to GET / within {timeout:?}: {detail}"
    )]
    BrowserFrontendUnavailable {
        tool: String,
        timeout: Duration,
        detail: String,
        evidence: StartupFailureEvidence,
    },
}

impl StartupProbe {
    pub fn class(&self) -> ProbeClass {
        match self {
            Self::StdioServer(_) => ProbeClass::StdioProtocolServer,
            Self::BrowserFrontend(_) => ProbeClass::BrowserFrontendViewer,
        }
    }

    pub fn run(
        &self,
        workspace: &EmptyWorkspace,
    ) -> Result<StartupProbeReport, StartupProbeError> {
        match self {
            Self::StdioServer(probe) => probe.run(workspace),
            Self::BrowserFrontend(probe) => probe.run(workspace),
        }
    }
}

impl BrowserFrontendProbe {
    fn run(
        &self,
        workspace: &EmptyWorkspace,
    ) -> Result<StartupProbeReport, StartupProbeError> {
        let before = workspace.snapshot()?;
        let port = reserve_ephemeral_port().map_err(|source| {
            StartupProbeError::Spawn {
                tool: self.tool.to_string(),
                source,
            }
        })?;
        let mut command = Command::new(&self.executable);
        command
            .args(&self.args)
            .current_dir(workspace.path())
            .env("WORKSPACE_ROOT", workspace.path())
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        for key in &self.unset_env {
            command.env_remove(key);
        }
        let log_viewer_config = match self.port {
            BrowserFrontendPort::Environment(name) => {
                command.env(name, port.to_string());
                None
            },
            BrowserFrontendPort::Argument(name) => {
                command.arg(name).arg(port.to_string());
                None
            },
            BrowserFrontendPort::LogViewerConfig => {
                let mut config =
                    tempfile::NamedTempFile::new().map_err(|source| {
                        StartupProbeError::Spawn {
                            tool: self.tool.to_string(),
                            source,
                        }
                    })?;
                writeln!(config, "[server]\nport = {port}").map_err(
                    |source| StartupProbeError::Spawn {
                        tool: self.tool.to_string(),
                        source,
                    },
                )?;
                command.env("LOG_VIEWER_CONFIG", config.path());
                Some(config)
            },
        };

        let mut child =
            command.spawn().map_err(|source| StartupProbeError::Spawn {
                tool: self.tool.to_string(),
                source,
            })?;
        let stderr = child.stderr.take().expect("stderr is piped");
        let stderr_reader = thread::spawn(move || {
            let mut output = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut output);
            output
        });

        let ready_result = wait_for_http_response(&mut child, port);
        let exit_code = terminate(&mut child);
        let stderr = stderr_reader.join().unwrap_or_default();
        drop(log_viewer_config);
        let after = workspace.snapshot()?;
        let evidence = StartupFailureEvidence {
            delta: before.diff(&after),
            after,
            exit_code,
            stderr,
        };
        if let Err(detail) = ready_result {
            return Err(StartupProbeError::BrowserFrontendUnavailable {
                tool: self.tool.to_string(),
                timeout: HTTP_READY_TIMEOUT,
                detail,
                evidence,
            });
        }

        Ok(StartupProbeReport {
            class: ProbeClass::BrowserFrontendViewer,
            delta: evidence.delta,
        })
    }
}

impl StdioServerProbe {
    fn run(
        &self,
        workspace: &EmptyWorkspace,
    ) -> Result<StartupProbeReport, StartupProbeError> {
        let before = workspace.snapshot()?;
        let tool = self.executable.display().to_string();
        let mut command = Command::new(&self.executable);
        command
            .args(&self.args)
            .current_dir(workspace.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for key in &self.unset_env {
            command.env_remove(key);
        }

        let mut child =
            command.spawn().map_err(|source| StartupProbeError::Spawn {
                tool: tool.clone(),
                source,
            })?;
        let stdout = child.stdout.take().expect("stdout is piped");
        let stderr = child.stderr.take().expect("stderr is piped");
        let (response_tx, response_rx) = mpsc::sync_channel(1);
        let stdout_reader = thread::spawn(move || {
            let mut line = String::new();
            let result = BufReader::new(stdout).read_line(&mut line);
            let _ = response_tx.send(result.map(|_| line));
        });
        let stderr_reader = thread::spawn(move || {
            let mut output = String::new();
            let _ = BufReader::new(stderr).read_to_string(&mut output);
            output
        });

        let handshake_result = (|| -> Result<(), String> {
            let initialize = serde_json::json!({
                "jsonrpc": "2.0",
                "id": 1,
                "method": "initialize",
                "params": {
                    "protocolVersion": "2025-03-26",
                    "capabilities": {},
                    "clientInfo": { "name": "storeless-startup-probe", "version": "1" }
                }
            });
            let stdin = child.stdin.as_mut().expect("stdin is piped");
            serde_json::to_writer(&mut *stdin, &initialize)
                .expect("initialize request serializes");
            stdin.write_all(b"\n").expect("initialize request writes");
            stdin.flush().expect("initialize request flushes");

            let line = response_rx
                .recv_timeout(INITIALIZE_TIMEOUT)
                .map_err(|error| error.to_string())?
                .map_err(|error| error.to_string())?;
            let response: serde_json::Value = serde_json::from_str(&line)
                .map_err(|error| {
                    format!("invalid JSON-RPC response {line:?}: {error}")
                })?;
            if response["id"] != 1 || response.get("result").is_none() {
                return Err(format!(
                    "unexpected JSON-RPC response: {response}"
                ));
            }
            Ok(())
        })();

        let (exit_code, forced_termination) = shutdown(&mut child);
        let _ = stdout_reader.join();
        let stderr = stderr_reader.join().unwrap_or_default();
        let after = workspace.snapshot()?;
        let evidence = StartupFailureEvidence {
            delta: before.diff(&after),
            after,
            exit_code,
            stderr,
        };
        if forced_termination {
            return Err(StartupProbeError::ForcedTermination {
                tool,
                evidence,
            });
        }
        if let Err(detail) = handshake_result {
            return Err(StartupProbeError::Initialize {
                tool,
                timeout: INITIALIZE_TIMEOUT,
                detail,
                evidence,
            });
        }

        if !evidence.stderr.is_empty() && !evidence.delta.is_empty() {
            eprintln!(
                "startup probe stderr for {}: {}",
                self.executable.display(),
                evidence.stderr
            );
        }
        Ok(StartupProbeReport {
            class: ProbeClass::StdioProtocolServer,
            delta: evidence.delta,
        })
    }
}

fn shutdown(child: &mut std::process::Child) -> (Option<i32>, bool) {
    child.stdin.take();
    let deadline = Instant::now() + SHUTDOWN_TIMEOUT;
    while Instant::now() < deadline {
        if child.try_wait().ok().flatten().is_some() {
            return (child.wait().ok().and_then(|status| status.code()), false);
        }
        thread::sleep(Duration::from_millis(10));
    }
    let _ = child.kill();
    (child.wait().ok().and_then(|status| status.code()), true)
}

fn reserve_ephemeral_port() -> std::io::Result<u16> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    listener.local_addr().map(|address| address.port())
}

fn wait_for_http_response(
    child: &mut std::process::Child,
    port: u16,
) -> Result<(), String> {
    let deadline = Instant::now() + HTTP_READY_TIMEOUT;
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], port));
    let mut last_error = None;
    while Instant::now() < deadline {
        match TcpStream::connect_timeout(&address, Duration::from_millis(100)) {
            Ok(mut stream) => {
                let _ =
                    stream.set_read_timeout(Some(Duration::from_millis(250)));
                stream
                    .write_all(b"GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n")
                    .map_err(|error| error.to_string())?;
                let mut response = [0; 16];
                match stream.read(&mut response) {
                    Ok(count) if response[..count].starts_with(b"HTTP/") => {
                        return Ok(());
                    },
                    Ok(_) =>
                        last_error = Some("response was not HTTP".to_string()),
                    Err(error) => last_error = Some(error.to_string()),
                }
            },
            Err(error) => last_error = Some(error.to_string()),
        }
        if let Some(status) =
            child.try_wait().map_err(|error| error.to_string())?
        {
            return Err(format!("process exited with {status}"));
        }
        thread::sleep(Duration::from_millis(25));
    }
    Err(last_error.unwrap_or_else(|| "timed out".to_string()))
}

fn terminate(child: &mut std::process::Child) -> Option<i32> {
    if child.try_wait().ok().flatten().is_none() {
        let _ = child.kill();
    }
    child.wait().ok().and_then(|status| status.code())
}
