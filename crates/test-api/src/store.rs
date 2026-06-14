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
    TestError,
    ValidationExecution,
    ValidationOutcome,
    ValidationSpec,
};

/// Configuration describing where the test-result store lives.
///
/// Mirrors the `.ticket` / `.spec` store conventions: a root directory (the
/// `.test` directory) plus a workspace slug that scopes storage. Validation
/// specs and executions are persisted as JSON files:
///
/// ```text
/// <root>/<workspace_slug>/specs/<spec_id>.json
/// <root>/<workspace_slug>/executions/<execution_id>.json
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestStoreConfig {
    pub root: PathBuf,
    pub workspace_slug: String,
}

/// Filter for querying validation executions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecutionQuery {
    /// Only return executions linked to this ticket id.
    pub ticket_id: Option<String>,
    /// Only return executions for this validation spec id.
    pub validation_spec_id: Option<String>,
    /// Only return executions with this outcome.
    pub outcome: Option<ValidationOutcome>,
    /// Maximum number of executions to return (after sorting).
    pub limit: Option<usize>,
}

impl TestStoreConfig {
    pub fn new(
        root: impl Into<PathBuf>,
        workspace_slug: impl Into<String>,
    ) -> Self {
        Self {
            root: root.into(),
            workspace_slug: workspace_slug.into(),
        }
    }

    // ── Spec persistence ────────────────────────────────────────────────────

    /// Persist (create or overwrite) a validation spec. Returns the file path.
    pub fn record_spec(
        &self,
        spec: &ValidationSpec,
    ) -> Result<PathBuf, TestError> {
        let path = self.spec_path(&spec.id)?;
        write_json(&path, spec)?;
        Ok(path)
    }

    /// Read a validation spec by id.
    pub fn get_spec(
        &self,
        id: &str,
    ) -> Result<ValidationSpec, TestError> {
        let path = self.spec_path(id)?;
        read_json_if_exists(&path)?.ok_or_else(|| TestError::SpecNotFound(id.to_string()))
    }

    /// List all validation specs, sorted by id.
    pub fn list_specs(&self) -> Result<Vec<ValidationSpec>, TestError> {
        let mut specs: Vec<ValidationSpec> = self.read_dir_json(&self.specs_dir()?)?;
        specs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(specs)
    }

    // ── Execution persistence ───────────────────────────────────────────────

    /// Persist (create or overwrite) a validation execution. Returns the path.
    pub fn record_execution(
        &self,
        execution: &ValidationExecution,
    ) -> Result<PathBuf, TestError> {
        let path = self.execution_path(&execution.id)?;
        write_json(&path, execution)?;
        Ok(path)
    }

    /// Read a validation execution by id.
    pub fn get_execution(
        &self,
        id: &str,
    ) -> Result<ValidationExecution, TestError> {
        let path = self.execution_path(id)?;
        read_json_if_exists(&path)?.ok_or_else(|| TestError::ExecutionNotFound(id.to_string()))
    }

    /// Query stored executions, sorted by `executed_at` descending (newest first).
    pub fn list_executions(
        &self,
        query: &ExecutionQuery,
    ) -> Result<Vec<ValidationExecution>, TestError> {
        let mut executions: Vec<ValidationExecution> =
            self.read_dir_json(&self.executions_dir()?)?;

        executions.retain(|exec| {
            if let Some(ticket_id) = &query.ticket_id {
                if !exec.links.links_to_ticket(ticket_id) {
                    return false;
                }
            }
            if let Some(spec_id) = &query.validation_spec_id {
                if &exec.validation_spec_id != spec_id {
                    return false;
                }
            }
            if let Some(outcome) = &query.outcome {
                if &exec.outcome != outcome {
                    return false;
                }
            }
            true
        });

        executions.sort_by(|a, b| b.executed_at.cmp(&a.executed_at).then(a.id.cmp(&b.id)));

        if let Some(limit) = query.limit {
            executions.truncate(limit);
        }
        Ok(executions)
    }

    // ── Path helpers ────────────────────────────────────────────────────────

    fn workspace_dir(&self) -> Result<PathBuf, TestError> {
        if self.root.as_os_str().is_empty() {
            return Err(TestError::EmptyRoot);
        }
        validate_segment(&self.workspace_slug)
            .map_err(|_| TestError::InvalidWorkspaceSlug(self.workspace_slug.clone()))?;
        Ok(self.root.join(&self.workspace_slug))
    }

    fn specs_dir(&self) -> Result<PathBuf, TestError> {
        Ok(self.workspace_dir()?.join("specs"))
    }

    fn executions_dir(&self) -> Result<PathBuf, TestError> {
        Ok(self.workspace_dir()?.join("executions"))
    }

    fn spec_path(
        &self,
        id: &str,
    ) -> Result<PathBuf, TestError> {
        validate_segment(id).map_err(|_| TestError::InvalidId(id.to_string()))?;
        Ok(self.specs_dir()?.join(format!("{id}.json")))
    }

    fn execution_path(
        &self,
        id: &str,
    ) -> Result<PathBuf, TestError> {
        validate_segment(id).map_err(|_| TestError::InvalidId(id.to_string()))?;
        Ok(self.executions_dir()?.join(format!("{id}.json")))
    }

    fn read_dir_json<T: DeserializeOwned>(
        &self,
        dir: &Path,
    ) -> Result<Vec<T>, TestError> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Vec::new()),
            Err(source) => {
                return Err(TestError::Io {
                    path: dir.to_path_buf(),
                    source,
                })
            },
        };

        let mut items = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| TestError::Io {
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

// ── Free functions ──────────────────────────────────────────────────────────

fn write_json<T: serde::Serialize>(
    path: &Path,
    value: &T,
) -> Result<(), TestError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| TestError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|source| TestError::Serialize {
        path: path.to_path_buf(),
        source,
    })?;
    fs::write(path, json).map_err(|source| TestError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn read_json_if_exists<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, TestError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(TestError::Io {
                path: path.to_path_buf(),
                source,
            })
        },
    };
    let value = serde_json::from_slice(&bytes).map_err(|source| TestError::Deserialize {
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

    use super::*;
    use crate::ValidationLinks;

    fn at(secs: u32) -> chrono::DateTime<chrono::Utc> {
        chrono::Utc
            .with_ymd_and_hms(2026, 6, 15, 12, 0, secs)
            .single()
            .unwrap()
    }

    fn config(dir: &TempDir) -> TestStoreConfig {
        TestStoreConfig::new(dir.path().join(".test"), "default")
    }

    #[test]
    fn records_and_reads_spec() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        let mut spec = ValidationSpec::new("vt-core-tests", "Core unit tests");
        spec.command = Some("cargo test -p ticket-vscode-core".to_string());

        let path = cfg.record_spec(&spec).unwrap();
        assert!(path.exists());
        assert_eq!(cfg.get_spec("vt-core-tests").unwrap(), spec);
    }

    #[test]
    fn records_and_reads_execution() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        let mut exec = ValidationExecution::passed("exec-1", "vt-core-tests", at(0));
        exec.links = ValidationLinks {
            ticket_ids: vec!["ticket-parity".to_string()],
            ..Default::default()
        };

        cfg.record_execution(&exec).unwrap();
        assert_eq!(cfg.get_execution("exec-1").unwrap(), exec);
    }

    #[test]
    fn missing_entries_report_not_found() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        assert!(matches!(
            cfg.get_spec("nope"),
            Err(TestError::SpecNotFound(_))
        ));
        assert!(matches!(
            cfg.get_execution("nope"),
            Err(TestError::ExecutionNotFound(_))
        ));
    }

    #[test]
    fn lists_executions_filtered_by_ticket_and_outcome() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);

        let mut passed = ValidationExecution::passed("exec-pass", "vt-a", at(1));
        passed.links = ValidationLinks {
            ticket_ids: vec!["ticket-x".to_string()],
            ..Default::default()
        };
        let mut blocked = ValidationExecution::blocked("exec-blocked", "vt-b", at(2));
        blocked.links = ValidationLinks {
            ticket_ids: vec!["ticket-x".to_string()],
            ..Default::default()
        };
        let other = ValidationExecution::passed("exec-other", "vt-a", at(3));

        cfg.record_execution(&passed).unwrap();
        cfg.record_execution(&blocked).unwrap();
        cfg.record_execution(&other).unwrap();

        let by_ticket = cfg
            .list_executions(&ExecutionQuery {
                ticket_id: Some("ticket-x".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_ticket.len(), 2);
        // newest first
        assert_eq!(by_ticket[0].id, "exec-blocked");

        let only_passed = cfg
            .list_executions(&ExecutionQuery {
                outcome: Some(ValidationOutcome::Passed),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(only_passed.len(), 2);

        let by_spec = cfg
            .list_executions(&ExecutionQuery {
                validation_spec_id: Some("vt-b".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_spec.len(), 1);
        assert_eq!(by_spec[0].id, "exec-blocked");
    }

    #[test]
    fn rejects_path_traversal_ids() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        let spec = ValidationSpec::new("../escape", "bad");
        assert!(matches!(
            cfg.record_spec(&spec),
            Err(TestError::InvalidId(_))
        ));
    }

    #[test]
    fn list_specs_sorted_and_empty_when_absent() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        assert!(cfg.list_specs().unwrap().is_empty());

        cfg.record_spec(&ValidationSpec::new("vt-b", "B")).unwrap();
        cfg.record_spec(&ValidationSpec::new("vt-a", "A")).unwrap();
        let specs = cfg.list_specs().unwrap();
        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].id, "vt-a");
        assert_eq!(specs[1].id, "vt-b");
    }
}
