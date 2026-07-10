//! End-to-end cross-domain operation matrix.
//!
//! Runs every `domain x operation` cell against a materialized fixture and
//! asserts that each cell produced a recorded `ValidationExecution` with a
//! duration, that no cell hard-failed, and that the executions are persisted in
//! the workspace `.test` store.

use std::collections::BTreeSet;

use memory_matrix::{
    ExpectedOutcome,
    FIXTURE_PROFILE_DEFAULT,
    OPERATIONS,
    TRANSPORTS,
    run_matrix,
    run_ticket_get_mcp_subprocess_failure_probe,
    run_ticket_spawn_fail_mcp_subprocess_failure_probe,
    transport_cells,
};
use test_api::{
    ExecutionQuery,
    ValidationOutcome,
};

const DOMAINS: &[&str] = &[
    "ticket", "spec", "rule", "audit", "session", "test", "doc", "log",
];

#[test]
fn transport_registry_has_canonical_cell_ids_and_blocked_reasons() {
    let cells = transport_cells();
    let expected_cells = DOMAINS.len() * TRANSPORTS.len() * OPERATIONS.len();
    assert_eq!(
        cells.len(),
        expected_cells,
        "registry should cover full matrix"
    );

    for cell in cells {
        assert_eq!(
            cell.cell_id,
            format!("{}.{}.{}", cell.domain, cell.operation, cell.transport),
            "cell_id should be <domain>.<operation>.<transport>"
        );
        assert_eq!(
            cell.fixture_profile, FIXTURE_PROFILE_DEFAULT,
            "every cell should carry a fixture profile"
        );

        match cell.expected_outcome {
            ExpectedOutcome::Passed => {
                assert!(
                    cell.blocked_reason.is_none(),
                    "pass cells should not carry blocked reasons"
                );
            },
            ExpectedOutcome::Blocked => {
                let reason = cell
                    .blocked_reason
                    .as_ref()
                    .expect("blocked cells must declare a reason");
                assert!(
                    !reason.trim().is_empty(),
                    "blocked reason should be non-empty"
                );
            },
        }
    }
}

#[test]
fn every_cell_records_an_execution_with_duration() {
    let run = run_matrix().expect("matrix should run against the fixture");

    let expected_cells = DOMAINS.len() * TRANSPORTS.len() * OPERATIONS.len();
    assert_eq!(
        run.records.len(),
        expected_cells,
        "every domain x operation cell should be recorded"
    );

    // Every cell has a recorded duration and a non-empty detail.
    for record in &run.records {
        assert_eq!(
            record.cell_id,
            format!(
                "{}.{}.{}",
                record.domain, record.operation, record.transport
            ),
            "record cell_id should remain canonical"
        );
        assert_eq!(
            record.fixture_profile, FIXTURE_PROFILE_DEFAULT,
            "record should include fixture profile"
        );
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
        if matches!(record.expected_outcome, ExpectedOutcome::Passed) {
            assert_eq!(
                record.outcome,
                ValidationOutcome::Passed,
                "expected-passed cell should pass for {}",
                record.cell_id
            );
        }
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
            .map(|r| {
                format!(
                    "{}.{}@{}: {}",
                    r.domain, r.operation, r.transport, r.detail
                )
            })
            .collect::<Vec<_>>()
    );

    // Blocked cells must always carry a concrete reason (never silent skips).
    for record in &run.records {
        if record.outcome == ValidationOutcome::Blocked {
            assert!(
                matches!(record.expected_outcome, ExpectedOutcome::Blocked),
                "blocked execution should match registry expectation for {}",
                record.cell_id
            );
            assert!(
                record.expected_blocked_reason.is_some(),
                "blocked execution should carry a registry blocked reason"
            );
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
        for transport in ["in-process", "cli"] {
            for op in crud_ops {
                let record = run
                    .records
                    .iter()
                    .find(|r| {
                        r.domain == domain
                            && r.transport == transport
                            && r.operation == op
                    })
                    .unwrap_or_else(|| {
                        panic!("missing cell {domain}.{op}@{transport}")
                    });
                assert_eq!(
                    record.outcome,
                    ValidationOutcome::Passed,
                    "{domain}.{op}@{transport} should pass: {}",
                    record.detail
                );
            }
        }
    }
}

#[test]
fn move_cells_reflect_adapter_backing_by_domain_and_transport() {
    let run = run_matrix().expect("matrix should run against the fixture");

    for domain in DOMAINS {
        for transport in TRANSPORTS {
            let record = run
                .records
                .iter()
                .find(|r| {
                    &r.domain == domain
                        && r.transport == *transport
                        && r.operation == "move"
                })
                .unwrap_or_else(|| {
                    panic!("missing move cell for {domain}@{transport}")
                });

            let adapter_backed = ["ticket", "spec", "rule"].contains(domain);
            let should_pass = adapter_backed && *transport == "in-process";

            if should_pass {
                assert_eq!(
                    record.outcome,
                    ValidationOutcome::Passed,
                    "{domain}.move@{transport} should pass via adapter-backed move kernel"
                );
                continue;
            }

            assert_eq!(
                record.outcome,
                ValidationOutcome::Blocked,
                "{domain}.move@{transport} should be blocked when transport/domain move wiring is absent"
            );
            assert!(
                record.detail.to_lowercase().contains("move")
                    || record.detail.to_lowercase().contains("transport"),
                "{domain}.move@{transport} reason should mention move/transport: {}",
                record.detail
            );
        }
    }
}

fn is_wired_matrix_cell(
    domain: &str,
    operation: &str,
    transport: &str,
) -> bool {
    match transport {
        "in-process" => true,
        "cli" => is_cli_wired_cell(domain, operation),
        "mcp" => is_mcp_wired_cell(domain, operation),
        "http" => is_http_wired_cell(domain, operation),
        _ => false,
    }
}

fn is_cli_wired_cell(
    domain: &str,
    operation: &str,
) -> bool {
    ["ticket", "spec", "rule"].contains(&domain) && operation != "move"
}

fn is_mcp_wired_cell(
    domain: &str,
    operation: &str,
) -> bool {
    match domain {
        "ticket" =>
            ["create", "get", "search", "update", "delete"].contains(&operation),
        "spec" => ["create", "get", "search", "update", "delete", "scan"]
            .contains(&operation),
        "rule" =>
            ["create", "get", "search", "update", "scan"].contains(&operation),
        _ => false,
    }
}

fn is_http_wired_cell(
    domain: &str,
    operation: &str,
) -> bool {
    domain == "ticket" && ["get", "search"].contains(&operation)
}

#[test]
fn unwired_transports_are_explicitly_blocked_with_reason() {
    let run = run_matrix().expect("matrix should run against the fixture");

    for record in &run.records {
        if is_wired_matrix_cell(
            &record.domain,
            &record.operation,
            &record.transport,
        ) {
            continue;
        }

        assert_eq!(
            record.outcome,
            ValidationOutcome::Blocked,
            "{}.{}@{} should be blocked until this transport wiring lands",
            record.domain,
            record.operation,
            record.transport
        );
        let has_explicit_reason = record.detail.contains("transport")
            || (record.transport == "cli"
                && record.operation == "move"
                && record.detail.to_lowercase().contains("move"));
        assert!(
            has_explicit_reason,
            "{}.{}@{} should carry an explicit transport reason: {}",
            record.domain, record.operation, record.transport, record.detail
        );
    }
}

#[test]
fn ticket_create_mcp_cell_is_wired_and_passes() {
    let run = run_matrix().expect("matrix should run against the fixture");

    let record = run
        .records
        .iter()
        .find(|r| {
            r.domain == "ticket"
                && r.transport == "mcp"
                && r.operation == "create"
        })
        .expect("missing ticket.create@mcp cell");
    assert_eq!(
        record.outcome,
        ValidationOutcome::Passed,
        "ticket.create@mcp should pass via ticket-mcp server: {}",
        record.detail
    );
}

#[test]
fn ticket_get_mcp_cell_is_wired_and_passes() {
    let run = run_matrix().expect("matrix should run against the fixture");

    let record = run
        .records
        .iter()
        .find(|r| {
            r.domain == "ticket" && r.transport == "mcp" && r.operation == "get"
        })
        .expect("missing ticket.get@mcp cell");
    assert_eq!(
        record.outcome,
        ValidationOutcome::Passed,
        "ticket.get@mcp should pass via ticket-mcp server: {}",
        record.detail
    );
}

#[test]
fn ticket_search_mcp_cell_is_wired_and_passes() {
    let run = run_matrix().expect("matrix should run against the fixture");

    let record = run
        .records
        .iter()
        .find(|r| {
            r.domain == "ticket"
                && r.transport == "mcp"
                && r.operation == "search"
        })
        .expect("missing ticket.search@mcp cell");
    assert_eq!(
        record.outcome,
        ValidationOutcome::Passed,
        "ticket.search@mcp should pass via ticket-mcp server: {}",
        record.detail
    );
}

#[test]
fn ticket_update_mcp_cell_is_wired_and_passes() {
    let run = run_matrix().expect("matrix should run against the fixture");

    let record = run
        .records
        .iter()
        .find(|r| {
            r.domain == "ticket"
                && r.transport == "mcp"
                && r.operation == "update"
        })
        .expect("missing ticket.update@mcp cell");
    assert_eq!(
        record.outcome,
        ValidationOutcome::Passed,
        "ticket.update@mcp should pass via ticket-mcp server: {}",
        record.detail
    );
}

#[test]
fn ticket_delete_mcp_cell_is_wired_and_passes() {
    let run = run_matrix().expect("matrix should run against the fixture");

    let record = run
        .records
        .iter()
        .find(|r| {
            r.domain == "ticket"
                && r.transport == "mcp"
                && r.operation == "delete"
        })
        .expect("missing ticket.delete@mcp cell");
    assert_eq!(
        record.outcome,
        ValidationOutcome::Passed,
        "ticket.delete@mcp should pass via ticket-mcp server: {}",
        record.detail
    );
}

#[test]
fn spec_scan_mcp_cell_is_wired_and_passes() {
    let run = run_matrix().expect("matrix should run against the fixture");

    let record = run
        .records
        .iter()
        .find(|r| {
            r.domain == "spec" && r.transport == "mcp" && r.operation == "scan"
        })
        .expect("missing spec.scan@mcp cell");
    assert_eq!(
        record.outcome,
        ValidationOutcome::Passed,
        "spec.scan@mcp should pass via spec-mcp server: {}",
        record.detail
    );
}

#[test]
fn rule_scan_mcp_cell_is_wired_and_passes() {
    let run = run_matrix().expect("matrix should run against the fixture");

    let record = run
        .records
        .iter()
        .find(|r| {
            r.domain == "rule" && r.transport == "mcp" && r.operation == "scan"
        })
        .expect("missing rule.scan@mcp cell");
    assert_eq!(
        record.outcome,
        ValidationOutcome::Passed,
        "rule.scan@mcp should pass via rule-mcp server: {}",
        record.detail
    );
}

#[test]
fn ticket_get_http_cell_is_wired_and_passes() {
    let run = run_matrix().expect("matrix should run against the fixture");

    let record = run
        .records
        .iter()
        .find(|r| {
            r.domain == "ticket"
                && r.transport == "http"
                && r.operation == "get"
        })
        .expect("missing ticket.get@http cell");
    assert_eq!(
        record.outcome,
        ValidationOutcome::Passed,
        "ticket.get@http should pass via ticket-http router: {}",
        record.detail
    );
}

#[test]
fn ticket_search_http_cell_is_wired_and_passes() {
    let run = run_matrix().expect("matrix should run against the fixture");

    let record = run
        .records
        .iter()
        .find(|r| {
            r.domain == "ticket"
                && r.transport == "http"
                && r.operation == "search"
        })
        .expect("missing ticket.search@http cell");
    assert_eq!(
        record.outcome,
        ValidationOutcome::Passed,
        "ticket.search@http should pass via ticket-http router: {}",
        record.detail
    );
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

#[test]
fn subprocess_probe_persists_full_failure_bundle_fields() {
    let run = run_ticket_get_mcp_subprocess_failure_probe()
        .expect("subprocess probe run should execute");
    assert_eq!(run.records.len(), 1, "probe run should emit one record");

    let record = &run.records[0];
    assert_eq!(
        record.outcome,
        ValidationOutcome::Failed,
        "probe should deterministically fail"
    );

    let bundle: serde_json::Value = serde_json::from_str(&record.detail)
        .expect("probe failure detail should be structured json");

    assert_eq!(
        bundle["error_class"].as_str(),
        Some("non_zero_exit"),
        "probe should classify cargo invalid subcommand as non_zero_exit"
    );

    assert_eq!(bundle["invocation"]["executable"].as_str(), Some("cargo"));
    assert_eq!(
        bundle["invocation"]["args"][0].as_str(),
        Some("definitely-not-a-valid-subcommand")
    );
    assert!(
        bundle["invocation"]["cwd"]
            .as_str()
            .map(|value| !value.is_empty())
            .unwrap_or(false),
        "bundle should include invocation cwd"
    );
    assert!(
        bundle["invocation"]["env_selectors"]["TICKET_INDEX_ROOT"]
            .as_str()
            .map(|value| !value.is_empty())
            .unwrap_or(false),
        "bundle should include whitelisted env selector"
    );
    assert_eq!(
        bundle["invocation"]["env_selectors"]
            .as_object()
            .map(|selectors| selectors.len()),
        Some(1),
        "bundle should include only whitelisted env selector keys"
    );

    assert_eq!(
        bundle["correlation"]["cell_id"].as_str(),
        Some(record.cell_id.as_str())
    );
    assert_eq!(
        bundle["correlation"]["transport"].as_str(),
        Some(record.transport.as_str())
    );
    assert_eq!(
        bundle["correlation"]["operation"].as_str(),
        Some(record.operation.as_str())
    );
    assert!(
        bundle["correlation"]["run_id"]
            .as_str()
            .map(|value| value.starts_with("matrix-probe-"))
            .unwrap_or(false),
        "bundle should include deterministic run_id prefix"
    );
    assert!(
        bundle["correlation"]["request_or_tool_id"]
            .as_str()
            .map(|value| value.starts_with("initialize#"))
            .unwrap_or(false),
        "bundle should include request/tool identifier"
    );

    assert_eq!(
        bundle["linkage"]["test_execution_id"].as_str(),
        Some(record.execution_id.as_str())
    );
    assert!(
        bundle["linkage"]["journal_id"].is_null(),
        "probe has no journal id yet"
    );
    assert!(
        bundle["linkage"]["log_session_ids"]
            .as_array()
            .map(|items| !items.is_empty())
            .unwrap_or(false),
        "probe should emit non-empty log_session_ids when runtime sessions are available"
    );
    assert_eq!(
        bundle["linkage"]["log_session_ids_reason"].as_str(),
        Some("runtime sessions correlated by run_id + test_execution_id")
    );

    let max_bytes = bundle["output_tails"]["max_bytes"]
        .as_u64()
        .expect("bundle should include max tail bytes");
    let stdout_tail = bundle["output_tails"]["stdout_tail"]
        .as_str()
        .unwrap_or_default();
    let stderr_tail = bundle["output_tails"]["stderr_tail"]
        .as_str()
        .unwrap_or_default();
    assert!(
        (stdout_tail.len() as u64) <= max_bytes,
        "stdout tail should be bounded"
    );
    assert!(
        (stderr_tail.len() as u64) <= max_bytes,
        "stderr tail should be bounded"
    );
    assert!(
        !stderr_tail.trim().is_empty(),
        "stderr tail should capture failing subprocess output"
    );

    let store = run.test_store();
    let persisted = store
        .list_executions(&ExecutionQuery::default())
        .expect("probe execution should be queryable")
        .into_iter()
        .find(|execution| execution.id == record.execution_id)
        .expect("probe execution should be persisted by execution id");
    assert_eq!(
        persisted.outcome,
        ValidationOutcome::Failed,
        "persisted execution should preserve failed outcome"
    );

    let persisted_detail = persisted
        .detail
        .expect("persisted execution should include detail bundle");
    let persisted_bundle: serde_json::Value =
        serde_json::from_str(&persisted_detail)
            .expect("persisted detail should remain parseable json bundle");
    assert_eq!(
        persisted_bundle["linkage"]["test_execution_id"].as_str(),
        Some(record.execution_id.as_str()),
        "persisted bundle should retain linkage id"
    );
    assert!(
        persisted_bundle["linkage"]["log_session_ids"]
            .as_array()
            .map(|items| !items.is_empty())
            .unwrap_or(false),
        "persisted bundle should retain non-empty correlated log_session_ids"
    );

    let persisted_run_id = persisted
        .provenance
        .run_id
        .as_deref()
        .expect("persisted execution should preserve provenance run_id");
    assert_eq!(
        persisted_run_id,
        persisted_bundle["correlation"]["run_id"]
            .as_str()
            .expect("bundle should include correlation run_id"),
        "bundle correlation run_id should match execution provenance run_id"
    );
}

#[test]
fn subprocess_spawn_probe_reports_spawn_failure_bundle() {
    let run = run_ticket_spawn_fail_mcp_subprocess_failure_probe()
        .expect("spawn probe run should execute");
    assert_eq!(
        run.records.len(),
        1,
        "spawn probe run should emit one record"
    );

    let record = &run.records[0];
    assert_eq!(
        record.outcome,
        ValidationOutcome::Failed,
        "spawn probe should deterministically fail"
    );

    let bundle: serde_json::Value = serde_json::from_str(&record.detail)
        .expect("spawn probe failure detail should be structured json");
    assert_eq!(bundle["error_class"].as_str(), Some("spawn_failure"));
    assert_eq!(
        bundle["invocation"]["executable"].as_str(),
        Some("definitely-missing-ticket-mcp-binary")
    );
    assert_eq!(
        bundle["linkage"]["test_execution_id"].as_str(),
        Some(record.execution_id.as_str())
    );
    assert!(
        bundle["linkage"]["log_session_ids"]
            .as_array()
            .map(|items| !items.is_empty())
            .unwrap_or(false),
        "spawn probe should include correlated runtime log session ids"
    );
    assert!(
        bundle["message"]
            .as_str()
            .map(|msg| msg
                .contains("spawn ticket-mcp stdio sentinel process failed"))
            .unwrap_or(false),
        "spawn probe should preserve spawn failure message"
    );
}
