use memory_fixtures::materialize_fixture;
use memory_matrix::{
    Cell,
    MatrixCtx,
    run_one,
};

fn fresh_ctx_without_store(
    store_dir: &str,
) -> (memory_fixtures::LoadedFixture, MatrixCtx, std::path::PathBuf) {
    let fixture = materialize_fixture().expect("fixture should materialize");
    let ctx = MatrixCtx::new(fixture.workspace_root.clone());
    let store_root = fixture.workspace_root.join(store_dir);
    if store_root.exists() {
        std::fs::remove_dir_all(&store_root)
            .expect("should remove seeded store root");
    }
    assert!(
        !store_root.exists(),
        "fixture variant should start with missing {}",
        store_dir
    );
    (fixture, ctx, store_root)
}

#[test]
fn strict_read_ops_with_missing_roots_do_not_succeed_or_recreate_store() {
    for (domain, store_dir) in [
        ("ticket", ".ticket"),
        ("spec", ".spec"),
        ("rule", ".rule"),
    ] {
        for op in ["get", "search", "scan"] {
            let (_fixture, ctx, store_root) = fresh_ctx_without_store(store_dir);
            let result = run_one(domain, op, &ctx);
            assert!(
                !matches!(result, Ok(Cell::Passed)),
                "{domain}.{op} should not pass when {store_dir} is missing"
            );
            assert!(
                !store_root.exists(),
                "{domain}.{op} must not recreate missing {store_dir}"
            );
        }
    }
}

#[test]
fn explicit_create_controls_are_the_only_root_creating_path() {
    for (domain, store_dir) in [
        ("ticket", ".ticket"),
        ("spec", ".spec"),
        ("rule", ".rule"),
    ] {
        let (_fixture, ctx, store_root) = fresh_ctx_without_store(store_dir);
        let result = run_one(domain, "create", &ctx);
        assert!(
            matches!(result, Ok(Cell::Passed)),
            "{domain}.create should remain the explicit positive control"
        );
        assert!(
            store_root.exists(),
            "{domain}.create should explicitly create missing {store_dir}"
        );
    }
}
