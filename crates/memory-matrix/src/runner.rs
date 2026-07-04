use super::*;

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
            let record = run_cell(&test_store, &**domain, &cell, &ctx, &run_id);
            records.push(record);
        } else {
            let detail = format!(
                "unknown domain `{}` for cell `{}`",
                cell.domain, cell.cell_id
            );
            let spec_id =
                format!("vt-matrix-{}", cell.cell_id.replace('.', "-"));
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

/// Run a single deterministic failing subprocess transport probe and persist
/// its execution detail in the same `.test` store format as full matrix runs.
pub fn run_ticket_get_mcp_subprocess_failure_probe(
) -> Result<MatrixRun, FixtureError> {
    let fixture = materialize_fixture()?;
    let workspace_root = fixture.workspace_root.clone();
    let ctx = MatrixCtx {
        workspace_root: workspace_root.clone(),
    };

    let test_store_root = workspace_root.join(".test");
    let test_store = TestStoreConfig::new(test_store_root.clone(), "default");
    let run_id = format!(
        "matrix-probe-{}",
        Utc::now().format("%Y%m%dT%H%M%SZ")
    );

    bootstrap_core_store_roots(&ctx);

    let cell = CellSpec {
        cell_id: "ticket.get.mcp_subprocess_failure".to_string(),
        domain: "ticket".to_string(),
        operation: "get".to_string(),
        transport: "mcp-subprocess-fail".to_string(),
        fixture_profile: FIXTURE_PROFILE_DEFAULT.to_string(),
        expected_outcome: ExpectedOutcome::Passed,
        blocked_reason: None,
    };

    let domain_ops = domains();
    let domain = domain_ops
        .iter()
        .find(|candidate| candidate.domain() == cell.domain)
        .expect("ticket domain should exist for subprocess failure probe");

    let record = run_cell(&test_store, &**domain, &cell, &ctx, &run_id);

    Ok(MatrixRun {
        records: vec![record],
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
    let metadata = DispatchMetadata {
        run_id: run_id.to_string(),
        cell_id: cell.cell_id.clone(),
        transport: cell.transport.clone(),
        operation: cell.operation.clone(),
        execution_id: execution_id.clone(),
    };
    let result = dispatch(
        domain,
        &cell.transport,
        &cell.operation,
        ctx,
        Some(&metadata),
    );
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
