use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;
use tempfile::TempDir;

#[derive(Debug, thiserror::Error)]
pub enum FixtureError {
    #[error("io error at {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("manifest parse error in {path}: {source}")]
    ManifestParse { path: PathBuf, source: toml::de::Error },
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

impl LoadedFixture {
    pub fn store_root(&self, domain: &str) -> Option<&Path> {
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
    generated_ticket_count: usize,
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

/// Materialize the fixture and initialize a real git repository at the root and
/// at each submodule worktree, so cross-worktree operations (notably ticket
/// `move`) can be exercised end-to-end against genuine git topology.
///
/// Each worktree gets an initial commit so tracked-file state is well-defined.
pub fn materialize_git_fixture() -> Result<LoadedFixture, FixtureError> {
    let fixture = materialize_fixture()?;

    git_init_worktree(&fixture.workspace_root)?;
    for worktree in &fixture.manifest.worktrees {
        if worktree.kind == "submodule" {
            let path = fixture
                .workspace_root
                .join(worktree.relative_path.replace('\\', "/"));
            git_init_worktree(&path)?;
        }
    }

    Ok(fixture)
}

fn git_init_worktree(dir: &Path) -> Result<(), FixtureError> {
    run_git(dir, &["init"])?;
    run_git(dir, &["config", "user.email", "fixture@example.com"])?;
    run_git(dir, &["config", "user.name", "Fixture"])?;
    run_git(dir, &["add", "-A"])?;
    run_git(dir, &["commit", "--no-gpg-sign", "-m", "fixture baseline"])?;
    Ok(())
}

fn run_git(dir: &Path, args: &[&str]) -> Result<(), FixtureError> {
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

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), FixtureError> {
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
        let to = dst.join(entry.file_name());

        if ty.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else if ty.is_file() {
            fs::copy(&from, &to).map_err(|source| FixtureError::Io {
                path: to,
                source,
            })?;
        }
    }

    Ok(())
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

        for path in fixture.store_roots.values() {
            assert!(path.exists(), "expected fixture path to exist: {}", path.display());
        }
    }

    #[test]
    fn generates_benchmark_scale_ticket_variant() {
        let fixture = materialize_fixture_with_generated_tickets(50).expect("fixture should load");
        let generated_dir = fixture.workspace_root.join(".ticket/tickets");
        let entries = fs::read_dir(&generated_dir)
            .unwrap()
            .filter_map(Result::ok)
            .count();

        assert!(entries >= 50, "expected generated tickets to be materialized");
    }

    #[test]
    fn git_fixture_initializes_root_and_submodule_worktrees() {
        let fixture = match materialize_git_fixture() {
            Ok(fixture) => fixture,
            Err(FixtureError::Git { detail, .. }) if detail.contains("os error 2") => {
                // git not installed in this environment; skip.
                return;
            }
            Err(err) => panic!("git fixture should materialize: {err}"),
        };

        assert!(fixture.workspace_root.join(".git").exists());
        assert!(fixture.workspace_root.join("submodule-a/.git").exists());
        assert!(fixture.workspace_root.join("submodule-b/.git").exists());
    }
}