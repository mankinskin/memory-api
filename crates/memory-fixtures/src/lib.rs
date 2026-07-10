use std::{
    fs,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
};

pub fn append_fixture_ticket(
    store_root: &Path,
    id: &str,
    title: &str,
    state: &str,
    component: &str,
) -> Result<PathBuf, FixtureError> {
    let ticket_dir = store_root.join("tickets").join(id);
    fs::create_dir_all(&ticket_dir).map_err(|source| FixtureError::Io {
        path: ticket_dir.clone(),
        source,
    })?;
    write_text(
        &ticket_dir.join("ticket.toml"),
        &format!(
            "id = \"{id}\"\ncreated_at = \"2026-06-28T00:00:00Z\"\ntitle = \"{title}\"\nstate = \"{state}\"\ntype = \"tracker-improvement\"\ncomponent = \"{component}\"\n"
        ),
    )?;
    write_text(
        &ticket_dir.join("description.md"),
        &format!(
            "# {title}\n\nAppended representative fixture ticket for incremental scan and perf timing.\n"
        ),
    )?;
    write_text(
        &ticket_dir.join("history.ndjson"),
        &format!(
            "{{\"rev\":1,\"ts\":\"2026-06-28T00:00:00Z\",\"fields\":{{\"state\":\"{state}\",\"title\":\"{title}\"}}}}\n"
        ),
    )?;
    Ok(ticket_dir)
}

pub fn fixture_source_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/memory-workspace-fixture")
}

pub fn materialize_fixture() -> Result<LoadedFixture, FixtureError> {
    let source_root = fixture_source_root();
    if !source_root.is_dir() {
        return Err(FixtureError::MissingFixtureRoot(source_root));
    }

    let tempdir = tempfile::tempdir().map_err(|source| FixtureError::Io {
        path: PathBuf::from("<tempdir>"),
        source,
    })?;
    let workspace_root = tempdir.path().join("memory-workspace-fixture");
    copy_dir_recursive(&source_root, &workspace_root)?;
    seed_representative_data(&workspace_root)?;

    let manifest_path = workspace_root.join("fixtures.toml");
    let manifest = read_manifest(&manifest_path)?;
    let store_roots = manifest
        .stores
        .iter()
        .map(|store| {
            (
                store.domain.clone(),
                workspace_root.join(store.relative_path.replace('\\', "/")),
            )
        })
        .collect();

    Ok(LoadedFixture {
        tempdir,
        workspace_root,
        manifest,
        store_roots,
    })
}

pub fn materialize_fixture_with_generated_tickets(
    generated_ticket_count: usize
) -> Result<LoadedFixture, FixtureError> {
    let fixture = materialize_fixture()?;
    let ticket_root = fixture.workspace_root.join(".ticket/tickets");

    for index in 0..generated_ticket_count {
        let id = format!("00000000-0000-0000-0000-{index:012x}");
        let ticket_dir = ticket_root.join(&id);
        fs::create_dir_all(&ticket_dir).map_err(|source| FixtureError::Io {
            path: ticket_dir.clone(),
            source,
        })?;

        let body = format!(
            "id = \"{id}\"\ncreated_at = \"2026-06-28T00:00:00Z\"\ntitle = \"Generated fixture ticket {index}\"\nstate = \"new\"\ntype = \"tracker-improvement\"\ncomponent = \"fixture\"\n"
        );
        let ticket_path = ticket_dir.join("ticket.toml");
        fs::write(&ticket_path, body).map_err(|source| FixtureError::Io {
            path: ticket_path,
            source,
        })?;
    }

    Ok(fixture)
}

pub fn materialize_fixture_with_ticket_perf_load(
    options: TicketPerfFixtureOptions
) -> Result<TicketPerfFixture, FixtureError> {
    let fixture = materialize_fixture()?;
    let (root_ticket_ids, submodule_ticket_ids, tracked_reference_files) =
        seed_ticket_perf_scenario(&fixture, options)?;
    Ok(TicketPerfFixture {
        fixture,
        root_ticket_ids,
        submodule_ticket_ids,
        tracked_reference_files,
    })
}

/// Materialize the fixture and initialize a real git repository at the root and
/// at each submodule worktree, so cross-worktree operations (notably ticket
/// `move`) can be exercised end-to-end against genuine git topology.
///
/// Each worktree gets an initial commit so tracked-file state is well-defined.
pub fn materialize_git_fixture() -> Result<LoadedFixture, FixtureError> {
    let fixture = materialize_fixture()?;

    let submodules: Vec<(String, PathBuf)> = fixture
        .manifest
        .worktrees
        .iter()
        .filter(|worktree| worktree.kind == "submodule")
        .map(|worktree| {
            let relative_path = worktree.relative_path.replace('\\', "/");
            let path = fixture.workspace_root.join(&relative_path);
            (relative_path, path)
        })
        .collect();

    for (_, path) in &submodules {
        git_init_worktree(path)?;
    }

    git_init_repo(&fixture.workspace_root)?;
    for (relative_path, _) in &submodules {
        let local_url = format!("./{relative_path}");
        run_git(
            &fixture.workspace_root,
            &[
                "-c",
                "protocol.file.allow=always",
                "submodule",
                "add",
                "--force",
                &local_url,
                relative_path,
            ],
        )?;
    }
    git_commit_all(&fixture.workspace_root, "fixture baseline")?;

    Ok(fixture)
}

pub fn materialize_git_fixture_with_ticket_perf_load(
    options: TicketPerfFixtureOptions
) -> Result<TicketPerfFixture, FixtureError> {
    let fixture = materialize_git_fixture()?;
    let (root_ticket_ids, submodule_ticket_ids, tracked_reference_files) =
        seed_ticket_perf_scenario(&fixture, options)?;
    commit_perf_fixture_changes(&fixture)?;
    Ok(TicketPerfFixture {
        fixture,
        root_ticket_ids,
        submodule_ticket_ids,
        tracked_reference_files,
    })
}

#[path = "lib_types.rs"]
mod lib_types;
pub use lib_types::*;

#[path = "lib_helpers.rs"]
mod lib_helpers;
use lib_helpers::*;

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
