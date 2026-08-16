use std::{
    collections::{
        BTreeMap,
        HashSet,
    },
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
    IdentifiableArtifact,
    TestError,
    TraceableArtifact,
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
    /// Only return executions with this provenance domain.
    pub domain: Option<String>,
    /// Only return executions with this provenance operation.
    pub operation: Option<String>,
    /// Only return executions with this provenance transport.
    pub transport: Option<String>,
    /// Only return executions with this provenance run id.
    pub run_id: Option<String>,
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
        read_json_if_exists(&path)?
            .ok_or_else(|| TestError::SpecNotFound(id.to_string()))
    }

    /// List all validation specs, sorted by id.
    pub fn list_specs(&self) -> Result<Vec<ValidationSpec>, TestError> {
        let mut specs: Vec<ValidationSpec> =
            self.read_dir_json(&self.specs_dir()?)?;
        specs.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(specs)
    }

    // ── Execution persistence ───────────────────────────────────────────────

    /// Persist (create or overwrite) a validation execution. Returns the path.
    pub fn record_execution(
        &self,
        execution: &ValidationExecution,
    ) -> Result<PathBuf, TestError> {
        // Compile-time check that ValidationExecution implements TraceableArtifact + IdentifiableArtifact
        fn assert_interoperable<
            T: TraceableArtifact + IdentifiableArtifact<Id = str>,
        >() {
        }
        assert_interoperable::<ValidationExecution>();

        execution.validate_interoperability_contract()?;
        let path = self.execution_path(&execution.id)?;
        write_json(&path, execution)?;
        self.prune_execution_runs(2)?;
        Ok(path)
    }

    /// Read a validation execution by id.
    pub fn get_execution(
        &self,
        id: &str,
    ) -> Result<ValidationExecution, TestError> {
        let path = self.execution_path(id)?;
        read_json_if_exists(&path)?
            .ok_or_else(|| TestError::ExecutionNotFound(id.to_string()))
    }

    /// Query stored executions, sorted by `executed_at` descending (newest first).
    pub fn list_executions(
        &self,
        query: &ExecutionQuery,
    ) -> Result<Vec<ValidationExecution>, TestError> {
        let mut executions: Vec<ValidationExecution> =
            self.read_dir_json(&self.executions_dir()?)?;

        executions.retain(|exec| Self::matches_execution_query(exec, query));
        Self::sort_executions(&mut executions, query.sort);

        if let Some(limit) = query.limit {
            executions.truncate(limit);
        }
        Ok(executions)
    }

    fn matches_execution_query(
        exec: &ValidationExecution,
        query: &ExecutionQuery,
    ) -> bool {
        Self::matches_execution_identity_filters(exec, query)
            && Self::matches_execution_duration_filters(exec, query)
            && Self::matches_execution_provenance_filters(exec, query)
    }

    fn matches_execution_identity_filters(
        exec: &ValidationExecution,
        query: &ExecutionQuery,
    ) -> bool {
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
    }

    fn matches_execution_duration_filters(
        exec: &ValidationExecution,
        query: &ExecutionQuery,
    ) -> bool {
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
    }

    fn matches_execution_provenance_filters(
        exec: &ValidationExecution,
        query: &ExecutionQuery,
    ) -> bool {
        if let Some(domain) = &query.domain {
            if exec.provenance.domain.as_deref() != Some(domain.as_str()) {
                return false;
            }
        }
        if let Some(operation) = &query.operation {
            if exec.provenance.operation.as_deref() != Some(operation.as_str())
            {
                return false;
            }
        }
        if let Some(transport) = &query.transport {
            if exec.provenance.transport.as_deref() != Some(transport.as_str())
            {
                return false;
            }
        }
        if let Some(run_id) = &query.run_id {
            if exec.provenance.run_id.as_deref() != Some(run_id.as_str()) {
                return false;
            }
        }
        true
    }

    fn sort_executions(
        executions: &mut [ValidationExecution],
        sort: ExecutionSort,
    ) {
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
    }

    // ── Benchmark persistence ───────────────────────────────────────────────

    /// Persist (create or overwrite) a benchmark execution. Returns the path.
    pub fn record_benchmark(
        &self,
        benchmark: &BenchmarkExecution,
    ) -> Result<PathBuf, TestError> {
        // Compile-time check that BenchmarkExecution implements TraceableArtifact + IdentifiableArtifact
        fn assert_interoperable<
            T: TraceableArtifact + IdentifiableArtifact<Id = str>,
        >() {
        }
        assert_interoperable::<BenchmarkExecution>();

        benchmark.validate_interoperability_contract()?;
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
        read_json_if_exists(&path)?
            .ok_or_else(|| TestError::BenchmarkNotFound(id.to_string()))
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

        benchmarks.sort_by(|a, b| {
            b.executed_at.cmp(&a.executed_at).then(a.id.cmp(&b.id))
        });

        if let Some(limit) = query.limit {
            benchmarks.truncate(limit);
        }
        Ok(benchmarks)
    }

    // ── Store-index generation ──────────────────────────────────────────────

    /// Build the deterministic store-index artifacts from all recorded
    /// executions, specs (for slow thresholds), and benchmarks.
    pub fn generate_store_index(
        &self
    ) -> Result<TestStoreIndexArtifacts, TestError> {
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
        fs::write(&toon_path, &artifacts.toon_sidecar).map_err(|source| {
            TestError::Io {
                path: toon_path.clone(),
                source,
            }
        })?;

        let md_path = dir.join("README.md");
        fs::write(&md_path, &artifacts.markdown).map_err(|source| {
            TestError::Io {
                path: md_path.clone(),
                source,
            }
        })?;

        Ok((toon_path, md_path))
    }

    /// Generate and write the store index in one step. Returns the digest and
    /// the two written paths.
    pub fn regenerate_store_index(
        &self
    ) -> Result<(String, PathBuf, PathBuf), TestError> {
        let artifacts = self.generate_store_index()?;
        let (toon_path, md_path) = self.write_store_index(&artifacts)?;
        Ok((artifacts.digest, toon_path, md_path))
    }

    // ── Path helpers ────────────────────────────────────────────────────────

    fn workspace_dir(&self) -> Result<PathBuf, TestError> {
        if self.root.as_os_str().is_empty() {
            return Err(TestError::EmptyRoot);
        }
        validate_segment(&self.workspace_slug).map_err(|_| {
            TestError::InvalidWorkspaceSlug(self.workspace_slug.clone())
        })?;
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
        validate_segment(id)
            .map_err(|_| TestError::InvalidId(id.to_string()))?;
        Ok(self.specs_dir()?.join(format!("{id}.json")))
    }

    fn execution_path(
        &self,
        id: &str,
    ) -> Result<PathBuf, TestError> {
        validate_segment(id)
            .map_err(|_| TestError::InvalidId(id.to_string()))?;
        Ok(self.executions_dir()?.join(format!("{id}.json")))
    }

    fn benchmark_path(
        &self,
        id: &str,
    ) -> Result<PathBuf, TestError> {
        validate_segment(id)
            .map_err(|_| TestError::InvalidId(id.to_string()))?;
        Ok(self.benchmarks_dir()?.join(format!("{id}.json")))
    }

    fn read_dir_json<T: DeserializeOwned>(
        &self,
        dir: &Path,
    ) -> Result<Vec<T>, TestError> {
        let entries = match fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(err) if err.kind() == ErrorKind::NotFound =>
                return Ok(Vec::new()),
            Err(source) =>
                return Err(TestError::Io {
                    path: dir.to_path_buf(),
                    source,
                }),
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

    fn prune_execution_runs(
        &self,
        keep_runs: usize,
    ) -> Result<(), TestError> {
        let executions: Vec<ValidationExecution> =
            self.read_dir_json(&self.executions_dir()?)?;

        let mut newest_by_spec_run: BTreeMap<
            String,
            BTreeMap<String, chrono::DateTime<chrono::Utc>>,
        > = BTreeMap::new();
        for execution in &executions {
            let Some(run_id) = execution.provenance.run_id.as_ref() else {
                continue;
            };
            let newest_by_run = newest_by_spec_run
                .entry(execution.validation_spec_id.clone())
                .or_default();
            match newest_by_run.get(run_id) {
                Some(existing) if *existing >= execution.executed_at => {},
                _ => {
                    newest_by_run.insert(run_id.clone(), execution.executed_at);
                },
            }
        }

        let mut stale_spec_runs: HashSet<(String, String)> = HashSet::new();
        for (spec_id, newest_by_run) in newest_by_spec_run {
            if newest_by_run.len() <= keep_runs {
                continue;
            }

            let mut runs: Vec<(String, chrono::DateTime<chrono::Utc>)> =
                newest_by_run.into_iter().collect();
            runs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
            stale_spec_runs.extend(
                runs.into_iter()
                    .skip(keep_runs)
                    .map(|(run_id, _)| (spec_id.clone(), run_id)),
            );
        }

        if stale_spec_runs.is_empty() {
            return Ok(());
        }

        for execution in executions {
            let Some(run_id) = execution.provenance.run_id.as_ref() else {
                continue;
            };
            if !stale_spec_runs.contains(&(
                execution.validation_spec_id.clone(),
                run_id.clone(),
            )) {
                continue;
            }
            let path = self.execution_path(&execution.id)?;
            match fs::remove_file(&path) {
                Ok(()) => {},
                Err(err) if err.kind() == ErrorKind::NotFound => {},
                Err(source) => {
                    return Err(TestError::Io { path, source });
                },
            }
        }

        Ok(())
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
    let json = serde_json::to_string_pretty(value).map_err(|source| {
        TestError::Serialize {
            path: path.to_path_buf(),
            source,
        }
    })?;
    fs::write(path, json).map_err(|source| TestError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn read_json_if_exists<T: DeserializeOwned>(
    path: &Path
) -> Result<Option<T>, TestError> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(err) if err.kind() == ErrorKind::NotFound => return Ok(None),
        Err(source) =>
            return Err(TestError::Io {
                path: path.to_path_buf(),
                source,
            }),
    };
    let value = serde_json::from_slice(&bytes).map_err(|source| {
        TestError::Deserialize {
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
#[path = "store/tests.rs"]
mod tests;
