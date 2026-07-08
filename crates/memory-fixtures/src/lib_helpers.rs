use super::*;

pub(super) fn git_init_worktree(dir: &Path) -> Result<(), FixtureError> {
    git_init_repo(dir)?;
    git_commit_all(dir, "fixture baseline")
}

pub(super) fn git_init_repo(dir: &Path) -> Result<(), FixtureError> {
    run_git(dir, &["init"])?;
    run_git(dir, &["config", "user.email", "fixture@example.com"])?;
    run_git(dir, &["config", "user.name", "Fixture"])
}

pub(super) fn git_commit_all(
    dir: &Path,
    message: &str,
) -> Result<(), FixtureError> {
    run_git(dir, &["add", "-A"])?;
    run_git(dir, &["commit", "--no-gpg-sign", "-m", message])
}

pub(super) fn run_git(
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

pub(super) fn read_manifest(path: &Path) -> Result<FixtureManifest, FixtureError> {
    let text = fs::read_to_string(path).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    toml::from_str(&text).map_err(|source| FixtureError::ManifestParse {
        path: path.to_path_buf(),
        source,
    })
}

pub(super) fn seed_representative_data(workspace_root: &Path) -> Result<(), FixtureError> {
    seed_generated_tickets(workspace_root, 24)?;
    seed_rule_store(workspace_root)?;
    seed_session_store(workspace_root)?;
    seed_test_store(workspace_root)?;
    seed_log_store(workspace_root)?;
    seed_audit_inputs(workspace_root)?;
    Ok(())
}

pub(super) fn seed_ticket_perf_scenario(
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

pub(super) fn seed_perf_ticket_batch(
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

pub(super) fn seed_tracked_reference_files(
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

pub(super) fn commit_perf_fixture_changes(fixture: &LoadedFixture) -> Result<(), FixtureError> {
    for worktree in &fixture.manifest.worktrees {
        if worktree.kind != "submodule" {
            continue;
        }
        let path = fixture.workspace_root.join(worktree.relative_path.replace('\\', "/"));
        git_commit_if_dirty(&path, "perf fixture load")?;
    }
    git_commit_if_dirty(&fixture.workspace_root, "perf fixture load")
}

pub(super) fn git_commit_if_dirty(
    dir: &Path,
    message: &str,
) -> Result<(), FixtureError> {
    if !git_has_changes(dir)? {
        return Ok(());
    }
    git_commit_all(dir, message)
}

pub(super) fn git_has_changes(dir: &Path) -> Result<bool, FixtureError> {
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

pub(super) fn seed_generated_tickets(
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

pub(super) fn seed_rule_store(workspace_root: &Path) -> Result<(), FixtureError> {
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

pub(super) fn seed_session_store(workspace_root: &Path) -> Result<(), FixtureError> {
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

pub(super) fn seed_test_store(workspace_root: &Path) -> Result<(), FixtureError> {
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

pub(super) fn seed_log_store(workspace_root: &Path) -> Result<(), FixtureError> {
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

pub(super) fn seed_audit_inputs(workspace_root: &Path) -> Result<(), FixtureError> {
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

pub(super) fn write_text(
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

pub(super) fn copy_dir_recursive(
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
pub(super) fn is_runtime_artifact(name: &std::ffi::OsStr) -> bool {
    let Some(name) = name.to_str() else {
        return false;
    };
    name == "search_index"
        || name.ends_with(".db")
        || name.ends_with(".db-wal")
        || name.ends_with(".db-shm")
}

