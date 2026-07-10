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
        capture.validate_interoperability_contract()?;
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
        read_json_if_exists(&path)?
            .ok_or_else(|| LogError::CaptureNotFound(id.to_string()))
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

        captures.sort_by(|a, b| {
            b.captured_at.cmp(&a.captured_at).then(a.id.cmp(&b.id))
        });

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
        session.validate_interoperability_contract()?;
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
        read_json_if_exists(&path)?
            .ok_or_else(|| LogError::RuntimeSessionNotFound(id.to_string()))
    }

    /// Query runtime log sessions, sorted by `started_at` descending (newest first).
    pub fn list_runtime_sessions(
        &self,
        query: &RuntimeLogSessionQuery,
    ) -> Result<Vec<RuntimeLogSession>, LogError> {
        let mut sessions: Vec<RuntimeLogSession> =
            self.read_dir_json(&self.runtime_sessions_dir()?)?;

        sessions.retain(|session| {
            Self::matches_runtime_session_query(session, query)
        });

        sessions.sort_by(|a, b| {
            b.started_at.cmp(&a.started_at).then(a.id.cmp(&b.id))
        });

        if let Some(limit) = query.limit {
            sessions.truncate(limit);
        }

        Ok(sessions)
    }

    fn matches_runtime_session_query(
        session: &RuntimeLogSession,
        query: &RuntimeLogSessionQuery,
    ) -> bool {
        Self::matches_runtime_session_core(session, query)
            && Self::matches_runtime_session_traceability(session, query)
    }

    fn matches_runtime_session_core(
        session: &RuntimeLogSession,
        query: &RuntimeLogSessionQuery,
    ) -> bool {
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
        true
    }

    fn matches_runtime_session_traceability(
        session: &RuntimeLogSession,
        query: &RuntimeLogSessionQuery,
    ) -> bool {
        Self::matches_runtime_session_primary_links(session, query)
            && Self::matches_runtime_session_secondary_links(session, query)
    }

    fn matches_runtime_session_primary_links(
        session: &RuntimeLogSession,
        query: &RuntimeLogSessionQuery,
    ) -> bool {
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
        true
    }

    fn matches_runtime_session_secondary_links(
        session: &RuntimeLogSession,
        query: &RuntimeLogSessionQuery,
    ) -> bool {
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
    }

    // ── Path helpers ────────────────────────────────────────────────────────

    fn workspace_dir(&self) -> Result<PathBuf, LogError> {
        if self.root.as_os_str().is_empty() {
            return Err(LogError::EmptyRoot);
        }
        validate_segment(&self.workspace_slug).map_err(|_| {
            LogError::InvalidWorkspaceSlug(self.workspace_slug.clone())
        })?;
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
        validate_segment(id)
            .map_err(|_| LogError::InvalidId(id.to_string()))?;
        Ok(self.captures_dir()?.join(format!("{id}.json")))
    }

    fn runtime_session_path(
        &self,
        id: &str,
    ) -> Result<PathBuf, LogError> {
        validate_segment(id)
            .map_err(|_| LogError::InvalidId(id.to_string()))?;
        Ok(self.runtime_sessions_dir()?.join(format!("{id}.json")))
    }

    fn read_dir_json<T: DeserializeOwned>(
        &self,
        dir: &Path,
    ) -> Result<Vec<T>, LogError> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == ErrorKind::NotFound =>
                return Ok(Vec::new()),
            Err(source) =>
                return Err(LogError::Io {
                    path: dir.to_path_buf(),
                    source,
                }),
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
    let json = serde_json::to_string_pretty(value).map_err(|source| {
        LogError::Serialize {
            path: path.to_path_buf(),
            source,
        }
    })?;
    fs::write(path, json).map_err(|source| LogError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn read_json_if_exists<T: DeserializeOwned>(
    path: &Path
) -> Result<Option<T>, LogError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) =>
            return Err(LogError::Io {
                path: path.to_path_buf(),
                source,
            }),
    };
    let value = serde_json::from_slice(&bytes).map_err(|source| {
        LogError::Deserialize {
            path: path.to_path_buf(),
            source,
        }
    })?;
    Ok(Some(value))
}

/// Rejects identifiers that would escape the store directory or contain path
/// separators. Allows ASCII alphanumerics plus `-`, `_`, and `.` (but not `..`).
fn validate_segment(segment: &str) -> Result<(), ()> {
    if segment.is_empty() || segment == "." || segment == ".." {
        return Err(());
    }
    if segment.contains('/') || segment.contains('\\') || segment.contains("..")
    {
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
#[path = "store_tests.rs"]
mod tests;
