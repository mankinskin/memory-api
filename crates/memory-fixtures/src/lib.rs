use std::{
    collections::BTreeMap,
    fs,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
};

use serde::Deserialize;
use tempfile::TempDir;

#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error("io error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("manifest parse error in {path}: {source}")]
    ManifestParse {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("fixture root not found: {0}")]
    MissingFixtureRoot(PathBuf),
    #[error("git command {args:?} failed in {dir}: {detail}")]
    Git {
        dir: PathBuf,
        args: Vec<String>,
        detail: String,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct FixtureManifest {
    pub fixture_name: String,
    pub stores: Vec<StoreDef>,
    pub worktrees: Vec<WorktreeDef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct StoreDef {
    pub domain: String,
    pub relative_path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorktreeDef {
    pub name: String,
    pub relative_path: String,
    pub kind: String,
}

#[derive(Debug)]
pub struct LoadedFixture {
    pub tempdir: TempDir,
    pub workspace_root: PathBuf,
    pub manifest: FixtureManifest,
    pub store_roots: BTreeMap<String, PathBuf>,
}

#[derive(Debug, Clone, Copy)]
pub struct TicketPerfFixtureOptions {
    pub root_generated_ticket_count: usize,
    pub submodule_generated_ticket_count: usize,
    pub tracked_reference_file_count: usize,
    pub references_per_file: usize,
}

impl Default for TicketPerfFixtureOptions {
    fn default() -> Self {
        Self {
            root_generated_ticket_count: 180,
            submodule_generated_ticket_count: 96,
            tracked_reference_file_count: 16,
            references_per_file: 20,
        }
    }
}

impl TicketPerfFixtureOptions {
    pub fn heavy() -> Self {
        Self {
            root_generated_ticket_count: 240,
            submodule_generated_ticket_count: 64,
            tracked_reference_file_count: 18,
            references_per_file: 28,
        }
    }

    pub fn stress() -> Self {
        Self {
            root_generated_ticket_count: 480,
            submodule_generated_ticket_count: 160,
            tracked_reference_file_count: 36,
            references_per_file: 48,
        }
    }
}

#[derive(Debug)]
pub struct TicketPerfFixture {
    pub fixture: LoadedFixture,
    pub root_ticket_ids: Vec<String>,
    pub submodule_ticket_ids: Vec<String>,
    pub tracked_reference_files: Vec<PathBuf>,
}

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
        &format!("# {title}\n\nAppended representative fixture ticket for incremental scan and perf timing.\n"),
    )?;
    write_text(
        &ticket_dir.join("history.ndjson"),
        &format!(
            "{{\"rev\":1,\"ts\":\"2026-06-28T00:00:00Z\",\"fields\":{{\"state\":\"{state}\",\"title\":\"{title}\"}}}}\n"
        ),
    )?;
    Ok(ticket_dir)
}

impl LoadedFixture {
    pub fn store_root(
        &self,
        domain: &str,
    ) -> Option<&Path> {
        self.store_roots.get(domain).map(PathBuf::as_path)
    }
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
    options: TicketPerfFixtureOptions,
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
    options: TicketPerfFixtureOptions,
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

fn git_init_worktree(dir: &Path) -> Result<(), FixtureError> {
    git_init_repo(dir)?;
    git_commit_all(dir, "fixture baseline")
}

fn git_init_repo(dir: &Path) -> Result<(), FixtureError> {
    run_git(dir, &["init"])?;
    run_git(dir, &["config", "user.email", "fixture@example.com"])?;
    run_git(dir, &["config", "user.name", "Fixture"])
}

fn git_commit_all(
    dir: &Path,
    message: &str,
) -> Result<(), FixtureError> {
    run_git(dir, &["add", "-A"])?;
    run_git(dir, &["commit", "--no-gpg-sign", "-m", message])
}

fn run_git(
    dir: &Path,
    args: &[&str],
) -> Result<(), FixtureError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|source| FixtureError::Git {
            dir: dir.to_path_buf(),
            args: args.iter().map(|a| a.to_string()).collect(),
            detail: source.to_string(),
        })?;

    if !output.status.success() {
        return Err(FixtureError::Git {
            dir: dir.to_path_buf(),
            args: args.iter().map(|a| a.to_string()).collect(),
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(())
}

fn read_manifest(path: &Path) -> Result<FixtureManifest, FixtureError> {
    let text = fs::read_to_string(path).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| FixtureError::ManifestParse {
        path: path.to_path_buf(),
        source,
    })
}

fn seed_representative_data(workspace_root: &Path) -> Result<(), FixtureError> {
    seed_generated_tickets(workspace_root, 24)?;
    seed_rule_store(workspace_root)?;
    seed_session_store(workspace_root)?;
    seed_test_store(workspace_root)?;
    seed_log_store(workspace_root)?;
    seed_audit_inputs(workspace_root)?;
    Ok(())
}

fn seed_ticket_perf_scenario(
    fixture: &LoadedFixture,
    options: TicketPerfFixtureOptions,
) -> Result<(Vec<String>, Vec<String>, Vec<PathBuf>), FixtureError> {
    let root_store = fixture
        .store_root("ticket-root")
        .ok_or_else(|| FixtureError::MissingFixtureRoot(fixture.workspace_root.join(".ticket")))?;
    let submodule_store = fixture
        .store_root("ticket-submodule-a")
        .ok_or_else(|| FixtureError::MissingFixtureRoot(fixture.workspace_root.join("submodule-a/.ticket")))?;

    let root_ticket_ids = seed_perf_ticket_batch(
        &root_store.join("tickets"),
        options.root_generated_ticket_count,
        0x2000,
        "perf-root",
    )?;
    let submodule_ticket_ids = seed_perf_ticket_batch(
        &submodule_store.join("tickets"),
        options.submodule_generated_ticket_count,
        0x3000,
        "perf-submodule",
    )?;
    let tracked_reference_files = seed_tracked_reference_files(
        &fixture.workspace_root,
        &root_ticket_ids,
        &submodule_ticket_ids,
        options,
    )?;

    Ok((root_ticket_ids, submodule_ticket_ids, tracked_reference_files))
}

fn seed_perf_ticket_batch(
    ticket_root: &Path,
    count: usize,
    group: u16,
    label: &str,
) -> Result<Vec<String>, FixtureError> {
    let mut ids = Vec::with_capacity(count);
    for index in 0..count {
        let id = format!("00000000-0000-{group:04x}-0000-{index:012x}");
        let ticket_dir = ticket_root.join(&id);
        fs::create_dir_all(&ticket_dir).map_err(|source| FixtureError::Io {
            path: ticket_dir.clone(),
            source,
        })?;

        let state = match index % 4 {
            0 => "new",
            1 => "ready",
            2 => "in-implementation",
            _ => "in-review",
        };
        let title = format!("{label} ticket {index:03}");
        write_text(
            &ticket_dir.join("ticket.toml"),
            &format!(
                "id = \"{id}\"\ncreated_at = \"2026-06-28T00:00:00Z\"\ntitle = \"{title}\"\nstate = \"{state}\"\ntype = \"tracker-improvement\"\ncomponent = \"perf\"\n"
            ),
        )?;
        write_text(
            &ticket_dir.join("description.md"),
            &format!(
                "# {title}\n\nRepresentative performance fixture ticket used to exercise ticket move and health behavior over larger stores, repeated rewrites, and broad graph traversals.\n"
            ),
        )?;
        write_text(
            &ticket_dir.join("history.ndjson"),
            &format!(
                "{{\"rev\":1,\"ts\":\"2026-06-28T00:00:00Z\",\"fields\":{{\"state\":\"{state}\",\"title\":\"{title}\"}}}}\n"
            ),
        )?;
        ids.push(id);
    }

    Ok(ids)
}

fn seed_tracked_reference_files(
    workspace_root: &Path,
    root_ticket_ids: &[String],
    submodule_ticket_ids: &[String],
    options: TicketPerfFixtureOptions,
) -> Result<Vec<PathBuf>, FixtureError> {
    let mut tracked_files = Vec::with_capacity(options.tracked_reference_file_count);

    for index in 0..options.tracked_reference_file_count {
        let (path, source_prefix, ids) = if index % 2 == 0 {
            (
                workspace_root.join("docs").join(format!("perf-move-root-{index:02}.md")),
                "submodule-a/.ticket/tickets",
                submodule_ticket_ids,
            )
        } else {
            (
                workspace_root
                    .join("submodule-a")
                    .join("docs")
                    .join(format!("perf-move-submodule-{index:02}.md")),
                ".ticket/tickets",
                submodule_ticket_ids,
            )
        };

        let mut body = format!("# Perf move refs {index:02}\n\n");
        for ref_index in 0..options.references_per_file {
            let ticket_id = &ids[(index + ref_index) % ids.len()];
            body.push_str(&format!(
                "- ref {ref_index:02}: {source_prefix}/{ticket_id}/ticket.toml\n"
            ));
        }
        if !root_ticket_ids.is_empty() {
            body.push_str("\n## Mixed root references\n");
            for ref_index in 0..options.references_per_file.min(root_ticket_ids.len()) {
                let ticket_id = &root_ticket_ids[(index + ref_index) % root_ticket_ids.len()];
                body.push_str(&format!(
                    "- root {ref_index:02}: .ticket/tickets/{ticket_id}/ticket.toml\n"
                ));
            }
        }

        write_text(&path, &body)?;
        tracked_files.push(path);
    }

    Ok(tracked_files)
}

fn commit_perf_fixture_changes(fixture: &LoadedFixture) -> Result<(), FixtureError> {
    for worktree in &fixture.manifest.worktrees {
        if worktree.kind != "submodule" {
            continue;
        }
        let path = fixture.workspace_root.join(worktree.relative_path.replace('\\', "/"));
        git_commit_if_dirty(&path, "perf fixture load")?;
    }
    git_commit_if_dirty(&fixture.workspace_root, "perf fixture load")
}

fn git_commit_if_dirty(
    dir: &Path,
    message: &str,
) -> Result<(), FixtureError> {
    if !git_has_changes(dir)? {
        return Ok(());
    }
    git_commit_all(dir, message)
}

fn git_has_changes(dir: &Path) -> Result<bool, FixtureError> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output()
        .map_err(|source| FixtureError::Git {
            dir: dir.to_path_buf(),
            args: vec!["status".to_string(), "--porcelain".to_string()],
            detail: source.to_string(),
        })?;

    if !output.status.success() {
        return Err(FixtureError::Git {
            dir: dir.to_path_buf(),
            args: vec!["status".to_string(), "--porcelain".to_string()],
            detail: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(!String::from_utf8_lossy(&output.stdout).trim().is_empty())
}

fn seed_generated_tickets(
    workspace_root: &Path,
    count: usize,
) -> Result<(), FixtureError> {
    let ticket_root = workspace_root.join(".ticket/tickets");
    for index in 0..count {
        let id = format!("00000000-0000-0000-0000-100000000{index:03x}");
        let ticket_dir = ticket_root.join(&id);
        fs::create_dir_all(&ticket_dir).map_err(|source| FixtureError::Io {
            path: ticket_dir.clone(),
            source,
        })?;

        let state = match index % 4 {
            0 => "new",
            1 => "ready",
            2 => "in-implementation",
            _ => "in-review",
        };
        let title = if index == 0 {
            "Matrixsearchtoken seeded ticket".to_string()
        } else {
            format!("Representative fixture ticket {index:02}")
        };
        let body = format!(
            "id = \"{id}\"\ncreated_at = \"2026-06-28T00:00:00Z\"\ntitle = \"{title}\"\nstate = \"{state}\"\ntype = \"tracker-improvement\"\ncomponent = \"fixture\"\nspec_ids = [\"00000000-0000-0000-0000-0000000000b1\"]\n"
        );
        write_text(&ticket_dir.join("ticket.toml"), &body)?;
        write_text(
            &ticket_dir.join("history.ndjson"),
            &format!(
                "{{\"rev\":1,\"ts\":\"2026-06-28T00:00:00Z\",\"fields\":{{\"state\":\"{state}\",\"title\":\"{title}\"}}}}\n"
            ),
        )?;
    }
    Ok(())
}

fn seed_rule_store(workspace_root: &Path) -> Result<(), FixtureError> {
    let rule_dir =
        workspace_root.join(".rule/rules/00000000-0000-0000-0000-0000000000c1");
    fs::create_dir_all(&rule_dir).map_err(|source| FixtureError::Io {
        path: rule_dir.clone(),
        source,
    })?;
    write_text(
        &rule_dir.join("rule.toml"),
        "id = \"00000000-0000-0000-0000-0000000000c1\"\ncreated_at = \"2026-06-28T00:00:00Z\"\nslug = \"fixture/rule-search\"\ntitle = \"Matrixruletoken Rule\"\ntype = \"rule-entry\"\nstate = \"draft\"\nfile_kind = \"markdown\"\nsection = \"fixture\"\norder_key = 0\nrepo_scopes = []\npath_scopes = []\nsentence_anchors = []\nfeedback_helpful_count = 0\nfeedback_mixed_count = 0\nfeedback_not_helpful_count = 0\nfeedback_note_count = 0\nfeedback_unresolved_count = 0\n",
    )?;
    write_text(
        &rule_dir.join("body.md"),
        "Seeded representative rule body linked from fixture tickets and specs.\n",
    )?;
    Ok(())
}

fn seed_session_store(workspace_root: &Path) -> Result<(), FixtureError> {
    let session_dir =
        workspace_root.join(".session/sessions/default/fixture-session");
    fs::create_dir_all(&session_dir).map_err(|source| FixtureError::Io {
        path: session_dir.clone(),
        source,
    })?;
    write_text(
        &session_dir.join("session.json"),
        r#"{
  "session_id": "fixture-session",
  "source": "fixture-generator",
  "started_at": "2026-06-28T00:00:00Z",
  "captured_at": "2026-06-28T00:00:01Z",
  "metadata": {
    "workspace_slug": "default",
    "agent_id": "fixture",
    "ticket_id": "00000000-0000-0000-0000-100000000000",
    "trigger": "representative-fixture"
  },
  "links": {
    "ticket_ids": ["00000000-0000-0000-0000-100000000000"],
    "spec_ids": ["00000000-0000-0000-0000-0000000000b1"],
    "log_ids": ["fixture-log-capture"]
  }
}
"#,
    )?;
    write_text(
        &session_dir.join("transcript.json"),
        r#"{
  "session_id": "fixture-session",
  "captured_at": "2026-06-28T00:00:01Z",
  "turns": [
    {
      "sequence": 1,
      "role": "user",
      "content": "fixture session seeded turn",
      "captured_at": "2026-06-28T00:00:01Z"
    }
  ]
}
"#,
    )?;
    Ok(())
}

fn seed_test_store(workspace_root: &Path) -> Result<(), FixtureError> {
    let specs_dir = workspace_root.join(".test-domain/default/specs");
    let executions_dir = workspace_root.join(".test-domain/default/executions");
    fs::create_dir_all(&specs_dir).map_err(|source| FixtureError::Io {
        path: specs_dir.clone(),
        source,
    })?;
    fs::create_dir_all(&executions_dir).map_err(|source| FixtureError::Io {
        path: executions_dir.clone(),
        source,
    })?;
    write_text(
        &specs_dir.join("vt-fixture-domain.json"),
        r#"{
  "id": "vt-fixture-domain",
  "title": "Fixture domain validation",
  "slow_threshold_ms": 2000,
  "links": {
    "ticket_ids": ["00000000-0000-0000-0000-100000000000"],
    "spec_ids": ["00000000-0000-0000-0000-0000000000b1"]
  }
}
"#,
    )?;
    write_text(
        &executions_dir.join("fixture-execution.json"),
        r#"{
  "id": "fixture-execution",
  "validation_spec_id": "vt-fixture-domain",
  "outcome": "passed",
  "executed_at": "2026-06-28T00:00:02Z",
  "duration_ms": 12,
  "detail": "seeded representative validation execution",
  "links": {
    "ticket_ids": ["00000000-0000-0000-0000-100000000000"],
    "spec_ids": ["00000000-0000-0000-0000-0000000000b1"],
    "log_ids": ["fixture-log-capture"]
  },
  "provenance": {
    "source_path": "test-fixtures/memory-workspace-fixture",
    "test_id": "fixture.validation",
    "domain": "test",
    "operation": "get",
    "transport": "fixture",
    "run_id": "fixture-seed"
  }
}
"#,
    )?;
    Ok(())
}

fn seed_log_store(workspace_root: &Path) -> Result<(), FixtureError> {
    let captures_dir = workspace_root.join(".log/default/captures");
    fs::create_dir_all(&captures_dir).map_err(|source| FixtureError::Io {
        path: captures_dir.clone(),
        source,
    })?;
    write_text(
        &captures_dir.join("fixture-log-capture.json"),
        r#"{
  "id": "fixture-log-capture",
  "validation_execution_id": "fixture-execution",
  "kind": "combined-output",
  "captured_at": "2026-06-28T00:00:03Z",
  "media_type": "text/plain",
  "locator": "test-fixtures/memory-workspace-fixture/logs/fixture.log",
  "detail": "seeded representative log capture",
  "links": {
    "ticket_ids": ["00000000-0000-0000-0000-100000000000"],
    "spec_ids": ["00000000-0000-0000-0000-0000000000b1"],
    "validation_execution_ids": ["fixture-execution"]
  }
}
"#,
    )?;
    Ok(())
}

fn seed_audit_inputs(workspace_root: &Path) -> Result<(), FixtureError> {
    write_text(
        &workspace_root.join("src/fixture_module.rs"),
        "pub fn fixture_indexed_symbol() -> &'static str { \"fixture\" }\n",
    )?;
    write_text(
        &workspace_root.join("docs/fixture.md"),
        "# Fixture Doc\n\nReferences ticket 00000000-0000-0000-0000-100000000000 and spec fixture/root.\n",
    )?;
    Ok(())
}

fn write_text(
    path: &Path,
    content: &str,
) -> Result<(), FixtureError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| FixtureError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    fs::write(path, content).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn copy_dir_recursive(
    src: &Path,
    dst: &Path,
) -> Result<(), FixtureError> {
    fs::create_dir_all(dst).map_err(|source| FixtureError::Io {
        path: dst.to_path_buf(),
        source,
    })?;

    let entries = fs::read_dir(src).map_err(|source| FixtureError::Io {
        path: src.to_path_buf(),
        source,
    })?;

    for entry in entries {
        let entry = entry.map_err(|source| FixtureError::Io {
            path: src.to_path_buf(),
            source,
        })?;
        let ty = entry.file_type().map_err(|source| FixtureError::Io {
            path: entry.path(),
            source,
        })?;
        let from = entry.path();

        // Skip derived store runtime artifacts (SQLite databases, WAL/SHM
        // sidecars, and full-text `search_index/` directories). They are
        // rebuilt from the seed manifests by `scan(...)`, and copying them is
        // both wasteful and, on Windows, prone to failing when a prior store
        // connection still holds a lock on the `*.db-shm` file.
        if is_runtime_artifact(&entry.file_name()) {
            continue;
        }

        let to = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to)
                .map_err(|source| FixtureError::Io { path: to, source })?;
        }
    }

    Ok(())
}

/// Whether a directory entry is a derived store runtime artifact that must not
/// be copied into a materialized fixture (it is regenerated by `scan`).
fn is_runtime_artifact(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    name == "search_index"
        || name.ends_with(".db")
        || name.ends_with(".db-wal")
        || name.ends_with(".db-shm")
}

#[cfg(test)]
mod tests {
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
                .join(
                    ".rule/rules/00000000-0000-0000-0000-0000000000c1/rule.toml"
                )
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

        let modules = fs::read_to_string(fixture.workspace_root.join(".gitmodules"))
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
        let perf = materialize_fixture_with_ticket_perf_load(TicketPerfFixtureOptions {
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
            assert!(path.is_file(), "expected tracked reference file: {}", path.display());
        }
    }
}
