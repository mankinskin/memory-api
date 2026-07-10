use super::*;

#[test]
fn materializes_fixture_and_exposes_store_roots() {
    let fixture = materialize_fixture().expect("fixture should load");

    assert!(fixture.workspace_root.is_dir());
    assert_eq!(fixture.manifest.fixture_name, "memory-workspace-fixture");
    assert!(fixture.store_root("ticket-root").is_some());
    assert!(fixture.store_root("ticket-submodule-a").is_some());
    assert!(fixture.store_root("spec-submodule-b").is_some());
    assert!(fixture.store_root("rule-root").is_some());
    assert!(fixture.store_root("session-root").is_some());
    assert!(fixture.store_root("test-domain-root").is_some());
    assert!(fixture.store_root("log-root").is_some());

    for path in fixture.store_roots.values() {
        assert!(
            path.exists(),
            "expected fixture path to exist: {}",
            path.display()
        );
    }
}

#[test]
fn materializes_representative_domain_seeds() {
    let fixture = materialize_fixture().expect("fixture should load");

    assert!(
        fixture
            .workspace_root
            .join(".rule/rules/00000000-0000-0000-0000-0000000000c1/rule.toml")
            .is_file()
    );
    assert!(
        fixture
            .workspace_root
            .join(".session/sessions/default/fixture-session/session.json")
            .is_file()
    );
    assert!(
        fixture
            .workspace_root
            .join(".test-domain/default/executions/fixture-execution.json")
            .is_file()
    );
    assert!(
        fixture
            .workspace_root
            .join(".log/default/captures/fixture-log-capture.json")
            .is_file()
    );
    assert!(
        fixture
            .workspace_root
            .join("src/fixture_module.rs")
            .is_file()
    );
    assert!(fixture.workspace_root.join("docs/fixture.md").is_file());
}

#[test]
fn generates_benchmark_scale_ticket_variant() {
    let fixture = materialize_fixture_with_generated_tickets(50)
        .expect("fixture should load");
    let generated_dir = fixture.workspace_root.join(".ticket/tickets");
    let entries = fs::read_dir(&generated_dir)
        .unwrap()
        .filter_map(Result::ok)
        .count();

    assert!(
        entries >= 50,
        "expected generated tickets to be materialized"
    );
}

#[test]
fn git_fixture_initializes_root_and_submodule_worktrees() {
    let fixture = match materialize_git_fixture() {
        Ok(fixture) => fixture,
        Err(FixtureError::Git { detail, .. })
            if detail.contains("os error 2") =>
        {
            // git not installed in this environment; skip.
            return;
        },
        Err(err) => panic!("git fixture should materialize: {err}"),
    };

    assert!(fixture.workspace_root.join(".git").exists());
    assert!(fixture.workspace_root.join("submodule-a/.git").exists());
    assert!(fixture.workspace_root.join("submodule-b/.git").exists());

    let modules =
        fs::read_to_string(fixture.workspace_root.join(".gitmodules"))
            .expect("read .gitmodules");
    assert!(modules.contains("path = submodule-a"));
    assert!(modules.contains("path = submodule-b"));

    let output = Command::new("git")
        .current_dir(&fixture.workspace_root)
        .args(["ls-files", "-s", "submodule-a", "submodule-b"])
        .output()
        .expect("git ls-files");
    assert!(output.status.success());
    let index = String::from_utf8_lossy(&output.stdout);
    assert!(index.contains("160000"));
}

#[test]
fn materializes_ticket_perf_fixture_with_reference_heavy_files() {
    let perf =
        materialize_fixture_with_ticket_perf_load(TicketPerfFixtureOptions {
            root_generated_ticket_count: 24,
            submodule_generated_ticket_count: 12,
            tracked_reference_file_count: 6,
            references_per_file: 8,
        })
        .expect("perf fixture should load");

    assert_eq!(perf.root_ticket_ids.len(), 24);
    assert_eq!(perf.submodule_ticket_ids.len(), 12);
    assert_eq!(perf.tracked_reference_files.len(), 6);
    for path in &perf.tracked_reference_files {
        assert!(
            path.is_file(),
            "expected tracked reference file: {}",
            path.display()
        );
    }
}
