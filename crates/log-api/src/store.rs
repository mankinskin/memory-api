use std::{
    fs,
    io::ErrorKind,
    path::{
        Path,
        PathBuf,
    },
};

use serde::de::DeserializeOwned;

use crate::{
    LogError,
    RuntimeLogSession,
    RuntimeLogStatus,
    RuntimeLogTransport,
    ValidationLogCapture,
};

/// Configuration describing where the validation-log store lives.
///
/// Mirrors the `.test` / `.ticket` store conventions: a root directory (the
/// `.log` directory) plus a workspace slug. Captures are persisted as JSON:
///
/// ```text
/// <root>/<workspace_slug>/captures/<capture_id>.json
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogStoreConfig {
    pub root: PathBuf,
    pub workspace_slug: String,
}

/// Filter for querying stored log captures.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogCaptureQuery {
    /// Only return captures linked to this validation execution id.
    pub execution_id: Option<String>,
    /// Maximum number of captures to return (after sorting).
    pub limit: Option<usize>,
}

/// Filter for querying runtime log sessions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeLogSessionQuery {
    pub status: Option<RuntimeLogStatus>,
    pub transport: Option<RuntimeLogTransport>,
    pub component: Option<String>,
    pub run_id: Option<String>,
    pub ticket_id: Option<String>,
    pub spec_id: Option<String>,
    pub validation_execution_id: Option<String>,
    pub journal_id: Option<String>,
    pub graph_operation_id: Option<String>,
    pub benchmark_id: Option<String>,
    pub agent_session_id: Option<String>,
    pub limit: Option<usize>,
}

impl LogStoreConfig {
    pub fn new(
        root: impl Into<PathBuf>,
        workspace_slug: impl Into<String>,
    ) -> Self {
        Self {
            root: root.into(),
            workspace_slug: workspace_slug.into(),
        }
    }

    /// Persist (create or overwrite) a log capture. Returns the file path.
    pub fn record_capture(
        &self,
        capture: &ValidationLogCapture,
    ) -> Result<PathBuf, LogError> {
        let path = self.capture_path(&capture.id)?;
        write_json(&path, capture)?;
        Ok(path)
    }

    /// Read a log capture by id.
    pub fn get_capture(
        &self,
        id: &str,
    ) -> Result<ValidationLogCapture, LogError> {
        let path = self.capture_path(id)?;
        read_json_if_exists(&path)?.ok_or_else(|| LogError::CaptureNotFound(id.to_string()))
    }

    /// Query stored captures, sorted by `captured_at` descending (newest first).
    pub fn list_captures(
        &self,
        query: &LogCaptureQuery,
    ) -> Result<Vec<ValidationLogCapture>, LogError> {
        let mut captures: Vec<ValidationLogCapture> =
            self.read_dir_json(&self.captures_dir()?)?;

        captures.retain(|capture| {
            if let Some(execution_id) = &query.execution_id {
                if &capture.validation_execution_id != execution_id
                    && !capture.links.links_to_execution(execution_id)
                {
                    return false;
                }
            }
            true
        });

        captures.sort_by(|a, b| b.captured_at.cmp(&a.captured_at).then(a.id.cmp(&b.id)));

        if let Some(limit) = query.limit {
            captures.truncate(limit);
        }
        Ok(captures)
    }

    /// Persist (create or overwrite) a runtime log session. Returns the file path.
    pub fn record_runtime_session(
        &self,
        session: &RuntimeLogSession,
    ) -> Result<PathBuf, LogError> {
        let path = self.runtime_session_path(&session.id)?;
        write_json(&path, session)?;
        Ok(path)
    }

    /// Read a runtime log session by id.
    pub fn get_runtime_session(
        &self,
        id: &str,
    ) -> Result<RuntimeLogSession, LogError> {
        let path = self.runtime_session_path(id)?;
        read_json_if_exists(&path)?.ok_or_else(|| LogError::RuntimeSessionNotFound(id.to_string()))
    }

    /// Query runtime log sessions, sorted by `started_at` descending (newest first).
    pub fn list_runtime_sessions(
        &self,
        query: &RuntimeLogSessionQuery,
    ) -> Result<Vec<RuntimeLogSession>, LogError> {
        let mut sessions: Vec<RuntimeLogSession> =
            self.read_dir_json(&self.runtime_sessions_dir()?)?;

        sessions.retain(|session| {
            if let Some(status) = &query.status {
                if &session.status != status {
                    return false;
                }
            }
            if let Some(transport) = &query.transport {
                if &session.transport != transport {
                    return false;
                }
            }
            if let Some(component) = &query.component {
                if &session.component != component {
                    return false;
                }
            }
            if let Some(run_id) = &query.run_id {
                if session.run_id.as_deref() != Some(run_id.as_str()) {
                    return false;
                }
            }
            if let Some(ticket_id) = &query.ticket_id {
                if !session.links.links_to_ticket(ticket_id) {
                    return false;
                }
            }
            if let Some(spec_id) = &query.spec_id {
                if !session.links.links_to_spec(spec_id) {
                    return false;
                }
            }
            if let Some(execution_id) = &query.validation_execution_id {
                if !session.links.links_to_execution(execution_id) {
                    return false;
                }
            }
            if let Some(journal_id) = &query.journal_id {
                if !session.links.links_to_journal(journal_id) {
                    return false;
                }
            }
            if let Some(graph_operation_id) = &query.graph_operation_id {
                if !session.links.links_to_graph_operation(graph_operation_id) {
                    return false;
                }
            }
            if let Some(benchmark_id) = &query.benchmark_id {
                if !session.links.links_to_benchmark(benchmark_id) {
                    return false;
                }
            }
            if let Some(agent_session_id) = &query.agent_session_id {
                if !session.links.links_to_agent_session(agent_session_id) {
                    return false;
                }
            }
            true
        });

        sessions.sort_by(|a, b| b.started_at.cmp(&a.started_at).then(a.id.cmp(&b.id)));

        if let Some(limit) = query.limit {
            sessions.truncate(limit);
        }

        Ok(sessions)
    }

    // ── Path helpers ────────────────────────────────────────────────────────

    fn workspace_dir(&self) -> Result<PathBuf, LogError> {
        if self.root.as_os_str().is_empty() {
            return Err(LogError::EmptyRoot);
        }
        validate_segment(&self.workspace_slug)
            .map_err(|_| LogError::InvalidWorkspaceSlug(self.workspace_slug.clone()))?;
        Ok(self.root.join(&self.workspace_slug))
    }

    fn captures_dir(&self) -> Result<PathBuf, LogError> {
        Ok(self.workspace_dir()?.join("captures"))
    }

    fn runtime_sessions_dir(&self) -> Result<PathBuf, LogError> {
        Ok(self.workspace_dir()?.join("sessions"))
    }

    fn capture_path(
        &self,
        id: &str,
    ) -> Result<PathBuf, LogError> {
        validate_segment(id).map_err(|_| LogError::InvalidId(id.to_string()))?;
        Ok(self.captures_dir()?.join(format!("{id}.json")))
    }

    fn runtime_session_path(
        &self,
        id: &str,
    ) -> Result<PathBuf, LogError> {
        validate_segment(id).map_err(|_| LogError::InvalidId(id.to_string()))?;
        Ok(self.runtime_sessions_dir()?.join(format!("{id}.json")))
    }

    fn read_dir_json<T: DeserializeOwned>(
        &self,
        dir: &Path,
    ) -> Result<Vec<T>, LogError> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(LogError::Io {
                    path: dir.to_path_buf(),
                    source,
                })
            },
        };

        let mut items = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| LogError::Io {
                path: dir.to_path_buf(),
                source,
            })?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            if let Some(item) = read_json_if_exists(&path)? {
                items.push(item);
            }
        }
        Ok(items)
    }
}

fn write_json<T: serde::Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), LogError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| LogError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|source| LogError::Serialize {
        path: path.to_path_buf(),
        source,
    })?;
    fs::write(path, json).map_err(|source| LogError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn read_json_if_exists<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, LogError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(LogError::Io {
                path: path.to_path_buf(),
                source,
            })
        },
    };
    let value = serde_json::from_slice(&bytes).map_err(|source| LogError::Deserialize {
        path: path.to_path_buf(),
        source,
    })?;
    Ok(Some(value))
}

/// Rejects identifiers that would escape the store directory or contain path
/// separators. Allows ASCII alphanumerics plus `-`, `_`, and `.` (but not `..`).
fn validate_segment(segment: &str) -> Result<(), ()> {
    if segment.is_empty() || segment == "." || segment == ".." {
        return Err(());
    }
    if segment.contains('/') || segment.contains('\\') || segment.contains("..") {
        return Err(());
    }
    if segment
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    {
        Ok(())
    } else {
        Err(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use pretty_assertions::assert_eq;
    use tempfile::TempDir;
    use test_api::ValidationExecution;

    use super::*;
    use crate::{
        RuntimeLogFormat,
        RuntimeLogLinks,
        RuntimeLogSession,
        RuntimeLogStatus,
        RuntimeLogTransport,
        ValidationLogCapture,
        ValidationLogKind,
    };

    fn at(secs: u32) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc
            .with_ymd_and_hms(2026, 6, 28, 12, 0, secs)
            .single()
            .unwrap()
    }

    fn config(dir: &TempDir) -> LogStoreConfig {
        LogStoreConfig::new(dir.path().join(".log"), "default")
    }

    fn capture(id: &str, exec_id: &str, secs: u32) -> ValidationLogCapture {
        let execution = ValidationExecution::passed(exec_id, "vt-a", at(secs));
        ValidationLogCapture::from_execution(
            id,
            &execution,
            ValidationLogKind::CombinedOutput,
            at(secs),
            "text/plain",
            format!("target/test-logs/{id}.log"),
        )
    }

    fn runtime_session(
        id: &str,
        secs: u32,
    ) -> RuntimeLogSession {
        let mut session = RuntimeLogSession::new(
            id,
            at(secs),
            RuntimeLogStatus::Active,
            "ticket-api",
            RuntimeLogTransport::Mcp,
            format!("target/test-logs/{id}.jsonl"),
            "application/json",
            RuntimeLogFormat::JsonLines,
        );
        session.operation = Some("scan".to_string());
        session.run_id = Some("run-1".to_string());
        session.links = RuntimeLogLinks {
            spec_ids: vec!["spec-1".to_string()],
            ticket_ids: vec!["ticket-1".to_string()],
            doc_evidence_ids: vec!["doc-1".to_string()],
            validation_execution_ids: vec!["exec-1".to_string()],
            benchmark_ids: vec!["bench-1".to_string()],
            agent_session_ids: vec!["agent-1".to_string()],
            journal_ids: vec!["journal-1".to_string()],
            graph_operation_ids: vec!["graph-op-1".to_string()],
        };
        session
    }

    #[test]
    fn records_and_reads_capture() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        let cap = capture("cap-1", "exec-1", 0);

        let path = cfg.record_capture(&cap).unwrap();
        assert!(path.exists());
        assert_eq!(cfg.get_capture("cap-1").unwrap(), cap);
    }

    #[test]
    fn missing_capture_reports_not_found() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        assert!(matches!(
            cfg.get_capture("nope"),
            Err(LogError::CaptureNotFound(_))
        ));
    }

    #[test]
    fn lists_captures_filtered_by_execution() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);

        cfg.record_capture(&capture("cap-a", "exec-1", 1)).unwrap();
        cfg.record_capture(&capture("cap-b", "exec-1", 2)).unwrap();
        cfg.record_capture(&capture("cap-c", "exec-2", 3)).unwrap();

        let by_exec = cfg
            .list_captures(&LogCaptureQuery {
                execution_id: Some("exec-1".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_exec.len(), 2);
        // newest first
        assert_eq!(by_exec[0].id, "cap-b");

        let all = cfg.list_captures(&LogCaptureQuery::default()).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn rejects_path_traversal_ids() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        let cap = capture("../escape", "exec-1", 0);
        assert!(matches!(
            cfg.record_capture(&cap),
            Err(LogError::InvalidId(_))
        ));
    }

    #[test]
    fn records_and_reads_runtime_session() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        let session = runtime_session("session-1", 1);

        let path = cfg.record_runtime_session(&session).unwrap();
        assert!(path.exists());
        assert_eq!(cfg.get_runtime_session("session-1").unwrap(), session);
    }

    #[test]
    fn missing_runtime_session_reports_not_found() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        assert!(matches!(
            cfg.get_runtime_session("nope"),
            Err(LogError::RuntimeSessionNotFound(_))
        ));
    }

    #[test]
    fn lists_runtime_sessions_with_filters() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);

        let mut a = runtime_session("session-a", 1);
        a.run_id = Some("run-a".to_string());
        let mut b = runtime_session("session-b", 2);
        b.transport = RuntimeLogTransport::Http;
        b.status = RuntimeLogStatus::Completed;
        b.run_id = Some("run-b".to_string());
        b.links.ticket_ids = vec!["ticket-2".to_string()];
        let c = runtime_session("session-c", 3);

        cfg.record_runtime_session(&a).unwrap();
        cfg.record_runtime_session(&b).unwrap();
        cfg.record_runtime_session(&c).unwrap();

        let all = cfg
            .list_runtime_sessions(&RuntimeLogSessionQuery::default())
            .unwrap();
        assert_eq!(all.len(), 3);
        assert_eq!(all[0].id, "session-c");

        let only_http_completed = cfg
            .list_runtime_sessions(&RuntimeLogSessionQuery {
                transport: Some(RuntimeLogTransport::Http),
                status: Some(RuntimeLogStatus::Completed),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(only_http_completed.len(), 1);
        assert_eq!(only_http_completed[0].id, "session-b");

        let ticket_2 = cfg
            .list_runtime_sessions(&RuntimeLogSessionQuery {
                ticket_id: Some("ticket-2".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(ticket_2.len(), 1);
        assert_eq!(ticket_2[0].id, "session-b");

        let run_a = cfg
            .list_runtime_sessions(&RuntimeLogSessionQuery {
                run_id: Some("run-a".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(run_a.len(), 1);
        assert_eq!(run_a[0].id, "session-a");
    }
}
