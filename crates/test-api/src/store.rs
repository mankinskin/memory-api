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
    ExecutionSort,
    TestError,
    ValidationExecution,
    ValidationOutcome,
    ValidationSpec,
    benchmark::{
        BenchmarkExecution,
        BenchmarkQuery,
    },
    store_index::{
        TestStoreIndexArtifacts,
        TestStoreIndexInput,
        generate_test_store_index,
    },
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
    /// Only return executions with duration >= this value.
    pub min_duration_ms: Option<u64>,
    /// Only return executions with duration <= this value.
    pub max_duration_ms: Option<u64>,
    /// Sort order for returned executions.
    pub sort: ExecutionSort,
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
            if let Some(min_duration_ms) = query.min_duration_ms {
                match exec.duration_ms {
                    Some(duration) if duration >= min_duration_ms => {},
                    _ => return false,
                }
            }
            if let Some(max_duration_ms) = query.max_duration_ms {
                match exec.duration_ms {
                    Some(duration) if duration <= max_duration_ms => {},
                    _ => return false,
                }
            }
            true
        });

        match query.sort {
            ExecutionSort::NewestFirst => {
                executions.sort_by(|a, b| b.executed_at.cmp(&a.executed_at).then(a.id.cmp(&b.id)));
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

        if let Some(limit) = query.limit {
            executions.truncate(limit);
        }
        Ok(executions)
    }

    // ── Benchmark persistence ───────────────────────────────────────────────

    /// Persist (create or overwrite) a benchmark execution. Returns the path.
    pub fn record_benchmark(
        &self,
        benchmark: &BenchmarkExecution,
    ) -> Result<PathBuf, TestError> {
        let path = self.benchmark_path(&benchmark.id)?;
        write_json(&path, benchmark)?;
        Ok(path)
    }

    /// Read a benchmark execution by id.
    pub fn get_benchmark(
        &self,
        id: &str,
    ) -> Result<BenchmarkExecution, TestError> {
        let path = self.benchmark_path(id)?;
        read_json_if_exists(&path)?.ok_or_else(|| TestError::BenchmarkNotFound(id.to_string()))
    }

    /// Query stored benchmarks, sorted by `executed_at` descending (newest first).
    pub fn list_benchmarks(
        &self,
        query: &BenchmarkQuery,
    ) -> Result<Vec<BenchmarkExecution>, TestError> {
        let mut benchmarks: Vec<BenchmarkExecution> =
            self.read_dir_json(&self.benchmarks_dir()?)?;

        benchmarks.retain(|bench| {
            if let Some(domain) = &query.domain {
                if &bench.domain != domain {
                    return false;
                }
            }
            if let Some(operation) = &query.operation {
                if &bench.operation != operation {
                    return false;
                }
            }
            if let Some(over_budget) = query.over_budget {
                if bench.over_budget != over_budget {
                    return false;
                }
            }
            true
        });

        benchmarks.sort_by(|a, b| b.executed_at.cmp(&a.executed_at).then(a.id.cmp(&b.id)));

        if let Some(limit) = query.limit {
            benchmarks.truncate(limit);
        }
        Ok(benchmarks)
    }

    // ── Store-index generation ──────────────────────────────────────────────

    /// Build the deterministic store-index artifacts from all recorded
    /// executions, specs (for slow thresholds), and benchmarks.
    pub fn generate_store_index(&self) -> Result<TestStoreIndexArtifacts, TestError> {
        let executions = self.list_executions(&ExecutionQuery::default())?;
        let specs = self.list_specs()?;
        let benchmarks = self.list_benchmarks(&BenchmarkQuery::default())?;
        let input = TestStoreIndexInput {
            executions: &executions,
            specs: &specs,
            benchmarks: &benchmarks,
        };
        Ok(generate_test_store_index(&input))
    }

    /// Write the store-index artifacts to `index.toon` and `README.md` in the
    /// workspace directory. Returns the two written paths.
    pub fn write_store_index(
        &self,
        artifacts: &TestStoreIndexArtifacts,
    ) -> Result<(PathBuf, PathBuf), TestError> {
        let dir = self.workspace_dir()?;
        fs::create_dir_all(&dir).map_err(|source| TestError::Io {
            path: dir.clone(),
            source,
        })?;

        let toon_path = dir.join("index.toon");
        fs::write(&toon_path, &artifacts.toon_sidecar).map_err(|source| TestError::Io {
            path: toon_path.clone(),
            source,
        })?;

        let md_path = dir.join("README.md");
        fs::write(&md_path, &artifacts.markdown).map_err(|source| TestError::Io {
            path: md_path.clone(),
            source,
        })?;

        Ok((toon_path, md_path))
    }

    /// Generate and write the store index in one step. Returns the digest and
    /// the two written paths.
    pub fn regenerate_store_index(&self) -> Result<(String, PathBuf, PathBuf), TestError> {
        let artifacts = self.generate_store_index()?;
        let (toon_path, md_path) = self.write_store_index(&artifacts)?;
        Ok((artifacts.digest, toon_path, md_path))
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

    fn benchmarks_dir(&self) -> Result<PathBuf, TestError> {
        Ok(self.workspace_dir()?.join("benchmarks"))
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

    fn benchmark_path(
        &self,
        id: &str,
    ) -> Result<PathBuf, TestError> {
        validate_segment(id).map_err(|_| TestError::InvalidId(id.to_string()))?;
        Ok(self.benchmarks_dir()?.join(format!("{id}.json")))
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
        passed.duration_ms = Some(40);
        passed.links = ValidationLinks {
            ticket_ids: vec!["ticket-x".to_string()],
            ..Default::default()
        };
        let mut blocked = ValidationExecution::blocked("exec-blocked", "vt-b", at(2));
        blocked.duration_ms = Some(80);
        blocked.links = ValidationLinks {
            ticket_ids: vec!["ticket-x".to_string()],
            ..Default::default()
        };
        let mut other = ValidationExecution::passed("exec-other", "vt-a", at(3));
        other.duration_ms = Some(15);

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

        let by_duration = cfg
            .list_executions(&ExecutionQuery {
                min_duration_ms: Some(20),
                max_duration_ms: Some(60),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_duration.len(), 1);
        assert_eq!(by_duration[0].id, "exec-pass");

        let slowest = cfg
            .list_executions(&ExecutionQuery {
                sort: ExecutionSort::SlowestFirst,
                ..Default::default()
            })
            .unwrap();
        assert_eq!(slowest[0].id, "exec-blocked");
        assert_eq!(slowest[1].id, "exec-pass");
        assert_eq!(slowest[2].id, "exec-other");
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

    #[test]
    fn records_and_queries_benchmarks_by_domain_and_over_budget() {
        use crate::benchmark::{
            BenchmarkExecution,
            BenchmarkQuery,
        };

        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);

        let mut get = BenchmarkExecution::new("bench-get", "get_by_id", "get", "ticket", at(1));
        get.mean_ns = 75_000_000;
        get.apply_budget(Some(50_000_000));

        let mut scan = BenchmarkExecution::new("bench-scan", "scan_root", "scan", "ticket", at(2));
        scan.mean_ns = 400_000_000;
        scan.apply_budget(Some(1_000_000_000));

        let spec_search =
            BenchmarkExecution::new("bench-search", "search_q", "search", "spec", at(3));

        cfg.record_benchmark(&get).unwrap();
        cfg.record_benchmark(&scan).unwrap();
        cfg.record_benchmark(&spec_search).unwrap();

        assert_eq!(cfg.get_benchmark("bench-get").unwrap(), get);

        let ticket_benches = cfg
            .list_benchmarks(&BenchmarkQuery {
                domain: Some("ticket".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(ticket_benches.len(), 2);
        // newest first
        assert_eq!(ticket_benches[0].id, "bench-scan");

        let over_budget = cfg
            .list_benchmarks(&BenchmarkQuery {
                over_budget: Some(true),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(over_budget.len(), 1);
        assert_eq!(over_budget[0].id, "bench-get");

        let by_op = cfg
            .list_benchmarks(&BenchmarkQuery {
                operation: Some("search".to_string()),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(by_op.len(), 1);
        assert_eq!(by_op[0].domain, "spec");
    }

    #[test]
    fn missing_benchmark_reports_not_found() {
        let dir = TempDir::new().unwrap();
        let cfg = config(&dir);
        assert!(matches!(
            cfg.get_benchmark("nope"),
            Err(TestError::BenchmarkNotFound(_))
        ));
    }
}
