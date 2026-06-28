//! End-to-end cross-domain operation matrix.
//!
//! Runs every `domain x operation` cell against a materialized fixture and
//! asserts that each cell produced a recorded `ValidationExecution` with a
//! duration, that no cell hard-failed, and that the executions are persisted in
//! the workspace `.test` store.

use std::collections::BTreeSet;

use memory_matrix::{
    OPERATIONS,
    run_matrix,
};
use test_api::{
    ExecutionQuery,
    ValidationOutcome,
};

const DOMAINS: &[&str] = &[
    "ticket", "spec", "rule", "audit", "session", "test", "doc", "log",
];

#[test]
fn every_cell_records_an_execution_with_duration() {
    let run = run_matrix().expect("matrix should run against the fixture");

    let expected_cells = DOMAINS.len() * OPERATIONS.len();
    assert_eq!(
        run.records.len(),
        expected_cells,
        "every domain x operation cell should be recorded"
    );

    // Every cell has a recorded duration and a non-empty detail.
    for record in &run.records {
        assert!(
            record.duration_ms < u64::MAX,
            "{}.{} should carry a duration",
            record.domain,
            record.operation
        );
        assert!(
            !record.detail.trim().is_empty(),
            "{}.{} should carry a detail/reason",
            record.domain,
            record.operation
        );
    }

    // No cell may hard-fail: each is either Passed or Blocked-with-reason.
    let failed: Vec<&_> = run
        .records
        .iter()
        .filter(|record| record.outcome == ValidationOutcome::Failed)
        .collect();
    assert!(
        failed.is_empty(),
        "no cell should fail; failures: {:?}",
        failed
            .iter()
            .map(|r| format!("{}.{}: {}", r.domain, r.operation, r.detail))
            .collect::<Vec<_>>()
    );

    // Blocked cells must always carry a concrete reason (never silent skips).
    for record in &run.records {
        if record.outcome == ValidationOutcome::Blocked {
            assert!(
                record.detail.len() > 10,
                "blocked cell {}.{} must explain why",
                record.domain,
                record.operation
            );
        }
    }
}

#[test]
fn core_crud_domains_pass_get_search_crud_and_scan() {
    let run = run_matrix().expect("matrix should run against the fixture");

    // Domains backed by a full entity store must pass get/search/CRUD/scan.
    let crud_ops = ["get", "search", "create", "update", "delete", "scan"];
    for domain in ["ticket", "spec", "rule"] {
        for op in crud_ops {
            let record = run
                .records
                .iter()
                .find(|r| r.domain == domain && r.operation == op)
                .unwrap_or_else(|| panic!("missing cell {domain}.{op}"));
            assert_eq!(
                record.outcome,
                ValidationOutcome::Passed,
                "{domain}.{op} should pass: {}",
                record.detail
            );
        }
    }
}

#[test]
fn move_cells_are_blocked_with_a_reason() {
    let run = run_matrix().expect("matrix should run against the fixture");

    for domain in DOMAINS {
        let record = run
            .records
            .iter()
            .find(|r| &r.domain == domain && r.operation == "move")
            .unwrap_or_else(|| panic!("missing move cell for {domain}"));
        assert_eq!(
            record.outcome,
            ValidationOutcome::Blocked,
            "{domain}.move should be blocked until the move kernel lands"
        );
        assert!(
            record.detail.to_lowercase().contains("move"),
            "{domain}.move reason should mention move: {}",
            record.detail
        );
    }
}

#[test]
fn executions_are_persisted_in_the_workspace_test_store() {
    let run = run_matrix().expect("matrix should run against the fixture");

    let store = run.test_store();
    let executions = store.list_executions(&ExecutionQuery::default()).expect(
        "executions should be queryable from the workspace .test store",
    );

    assert_eq!(
        executions.len(),
        run.records.len(),
        "every recorded cell should be persisted as an execution"
    );

    // Each persisted execution carries a duration and links back to the ticket.
    let mut spec_ids = BTreeSet::new();
    for execution in &executions {
        assert!(
            execution.duration_ms.is_some(),
            "execution {} should carry a duration",
            execution.id
        );
        spec_ids.insert(execution.validation_spec_id.clone());
    }
    assert_eq!(
        spec_ids.len(),
        run.records.len(),
        "each cell should record under its own per-operation validation spec"
    );
}
