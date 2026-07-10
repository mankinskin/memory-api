use chrono::Utc;

use test_api::{
    TestStoreConfig,
    ValidationExecution,
    ValidationLinks,
    ValidationOutcome,
    ValidationProvenance,
};

use crate::matrix::{
    CellResult,
    DomainOps,
    MatrixCtx,
    blocked,
    pass,
};

pub(crate) struct TestDomain;

impl TestDomain {
    fn config(ctx: &MatrixCtx) -> TestStoreConfig {
        // Isolated from the matrix's own evidence store (`.test`).
        TestStoreConfig::new(ctx.store_root(".test-domain"), "default")
    }

    fn execution(
        id: &str,
        operation: &str,
        outcome: ValidationOutcome,
    ) -> ValidationExecution {
        let mut execution =
            ValidationExecution::new(id, "vt-test-domain", outcome, Utc::now());
        execution.duration_ms = Some(1);
        execution.links = ValidationLinks {
            ticket_ids: vec!["ticket-memory-matrix".to_string()],
            ..Default::default()
        };
        execution.provenance = ValidationProvenance {
            domain: Some("test".to_string()),
            operation: Some(operation.to_string()),
            run_id: Some("matrix-test-domain".to_string()),
            ..Default::default()
        };
        execution
    }
}

impl DomainOps for TestDomain {
    fn domain(&self) -> &'static str {
        "test"
    }

    fn create(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_execution(&Self::execution(
                "matrix-create",
                "create",
                ValidationOutcome::Passed,
            ))
            .map_err(|err| err.to_string())?;
        config
            .get_execution("matrix-create")
            .map_err(|err| err.to_string())?;
        pass()
    }

    fn get(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let config = Self::config(ctx);
        let fetched = config
            .get_execution("fixture-execution")
            .map_err(|err| err.to_string())?;
        if fetched.id == "fixture-execution" {
            pass()
        } else {
            Err(format!("unexpected execution id: {}", fetched.id))
        }
    }

    fn search(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let config = Self::config(ctx);
        let executions = config
            .list_executions(&test_api::ExecutionQuery {
                domain: Some("test".to_string()),
                operation: Some("get".to_string()),
                ..Default::default()
            })
            .map_err(|err| err.to_string())?;
        if executions.is_empty() {
            return Err("execution query returned no records".to_string());
        }
        pass()
    }

    fn update(
        &self,
        ctx: &MatrixCtx,
    ) -> CellResult {
        let config = Self::config(ctx);
        config
            .record_execution(&Self::execution(
                "matrix-update",
                "update",
                ValidationOutcome::Passed,
            ))
            .map_err(|err| err.to_string())?;
        config
            .record_execution(&Self::execution(
                "matrix-update",
                "update",
                ValidationOutcome::Failed,
            ))
            .map_err(|err| err.to_string())?;
        let fetched = config
            .get_execution("matrix-update")
            .map_err(|err| err.to_string())?;
        if fetched.outcome == ValidationOutcome::Failed {
            pass()
        } else {
            Err("re-record did not overwrite execution outcome".to_string())
        }
    }

    fn delete(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked("test-api exposes no delete operation for executions")
    }

    fn scan(
        &self,
        _ctx: &MatrixCtx,
    ) -> CellResult {
        blocked(
            "test-api has no scan/index reconcile; the store index is generated \
             (ticket 90de77b1), not scanned from disk",
        )
    }
}
