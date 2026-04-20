use std::fs;
use std::path::Path;

use spec_api::SpecStore;
use spec_cli::{BootstrapArgs, cmd_bootstrap};
use tempfile::TempDir;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Create a minimal Rust crate in `dir` with a given set of source files.
/// `files` is a list of (relative-path-under-src, content) tuples.
fn make_crate(dir: &Path, name: &str, files: &[(&str, &str)]) {
    let cargo_toml = format!(
        "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2024\"\n"
    );
    fs::write(dir.join("Cargo.toml"), cargo_toml).unwrap();
    let src = dir.join("src");
    fs::create_dir_all(&src).unwrap();
    // Always write a lib.rs so it's a valid crate
    fs::write(src.join("lib.rs"), "// root\n").unwrap();
    for (rel, content) in files {
        let path = src.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }
}

fn open_store(tmp: &TempDir) -> SpecStore {
    SpecStore::open(tmp.path()).unwrap()
}

fn bootstrap_args(crate_dir: &Path, dry_run: bool) -> BootstrapArgs {
    BootstrapArgs {
        crate_path: crate_dir.to_path_buf(),
        component: None,
        dry_run,
        workspace_root: Some(crate_dir.to_path_buf()),
        target_root: None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn bootstrap_dry_run_returns_correct_count() {
    let index_tmp = TempDir::new().unwrap();
    let crate_tmp = TempDir::new().unwrap();

    make_crate(
        crate_tmp.path(),
        "my-crate",
        &[("store.rs", "pub struct Store;\npub fn open() {}\n")],
    );

    let mut store = open_store(&index_tmp);
    let args = bootstrap_args(crate_tmp.path(), true);
    let result = cmd_bootstrap(args, &mut store).unwrap();

    // dry_run must be true and would_create must be > 0
    assert_eq!(result["dry_run"], true);
    let would_create = result["would_create"].as_u64().unwrap_or(0);
    assert!(would_create > 0, "expected would_create > 0, got {would_create}");

    // Nothing should have been written to the store
    let list = store.entity_store().list_indexed(false).unwrap();
    assert_eq!(list.len(), 0, "dry_run must not write specs");
}

#[test]
fn bootstrap_creates_root_and_module_specs() {
    let index_tmp = TempDir::new().unwrap();
    let crate_tmp = TempDir::new().unwrap();

    make_crate(
        crate_tmp.path(),
        "my-crate",
        &[("store.rs", "pub struct Store;\npub fn open() -> Store { Store }\n")],
    );

    let mut store = open_store(&index_tmp);
    let args = bootstrap_args(crate_tmp.path(), false);
    let result = cmd_bootstrap(args, &mut store).unwrap();

    assert_eq!(result["dry_run"], false);
    let created = result["created"].as_u64().unwrap_or(0);
    // Root spec + at least the store.rs module spec
    assert!(created >= 2, "expected at least 2 specs created, got {created}");

    // Root crate spec exists
    let root = store.resolve_id("my-crate");
    assert!(root.is_ok(), "root crate spec should exist");

    // Module spec exists
    let module = store.resolve_id("my-crate/store");
    assert!(module.is_ok(), "module spec 'my-crate/store' should exist");
}

#[test]
fn bootstrap_skips_existing_slugs() {
    let index_tmp = TempDir::new().unwrap();
    let crate_tmp = TempDir::new().unwrap();

    make_crate(
        crate_tmp.path(),
        "my-crate",
        &[("store.rs", "pub struct Store;\n")],
    );

    let mut store = open_store(&index_tmp);

    // First run
    let args1 = bootstrap_args(crate_tmp.path(), false);
    let result1 = cmd_bootstrap(args1, &mut store).unwrap();
    let created_first = result1["created"].as_u64().unwrap_or(0);
    assert!(created_first > 0, "first run should create specs");

    // Second run — all should be skipped
    let args2 = bootstrap_args(crate_tmp.path(), false);
    let result2 = cmd_bootstrap(args2, &mut store).unwrap();

    let skipped = result2["skipped"].as_u64().unwrap_or(0);
    let created_second = result2["created"].as_u64().unwrap_or(0);
    assert_eq!(created_second, 0, "second run should create 0 new specs");
    assert!(skipped > 0, "second run should skip existing specs");
}

#[test]
fn bootstrap_code_refs_have_valid_lines() {
    let index_tmp = TempDir::new().unwrap();
    let crate_tmp = TempDir::new().unwrap();

    make_crate(
        crate_tmp.path(),
        "my-crate",
        &[(
            "store.rs",
            "/// Opens the store.\npub fn open() {}\n\n/// Main struct.\npub struct Store {\n    x: u32,\n}\n",
        )],
    );

    let mut store = open_store(&index_tmp);
    let args = bootstrap_args(crate_tmp.path(), false);
    cmd_bootstrap(args, &mut store).unwrap();

    // Retrieve the module spec and check code refs
    let module_id = store.resolve_id("my-crate/store").unwrap();
    let manifest = store.get(&module_id.to_string()).unwrap();

    assert!(
        !manifest.code_refs.is_empty(),
        "module spec should have code refs"
    );
    for cr in &manifest.code_refs {
        assert!(
            cr.line_start >= 1,
            "line_start must be >= 1, got {} for {}",
            cr.line_start,
            cr.symbol
        );
        assert!(
            cr.line_end >= cr.line_start,
            "line_end must be >= line_start for {}",
            cr.symbol
        );
    }
}

#[test]
fn bootstrap_module_body_contains_items() {
    let index_tmp = TempDir::new().unwrap();
    let crate_tmp = TempDir::new().unwrap();

    make_crate(
        crate_tmp.path(),
        "my-crate",
        &[("store.rs", "pub struct Store;\npub enum State { Open, Closed }\n")],
    );

    let mut store = open_store(&index_tmp);
    let args = bootstrap_args(crate_tmp.path(), false);
    cmd_bootstrap(args, &mut store).unwrap();

    let module_id = store.resolve_id("my-crate/store").unwrap();
    let (_, body) = store.get_full(&module_id.to_string()).unwrap();

    assert!(body.contains("Store"), "body should contain 'Store'");
    assert!(body.contains("State"), "body should contain 'State'");
}
