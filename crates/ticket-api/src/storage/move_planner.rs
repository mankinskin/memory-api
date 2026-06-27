use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    process::Command,
};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    error::StorageError,
    storage::{
        indexed::{IndexedTicket, LeaseInfo},
        store::TicketStore,
        BoardEntry,
    },
    workspace,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveReferenceVisibility {
    pub related_ticket_id: Uuid,
    pub direction: MoveReferenceDirection,
    pub visible_from_destination: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MoveReferenceDirection {
    Inbound,
    Outbound,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MovePreflightBlocker {
    DifferentGitWorktree {
        source_worktree_root: PathBuf,
        target_worktree_root: PathBuf,
    },
    MissingSourceTicket {
        ticket_id: Uuid,
    },
    MissingTargetStore {
        target_store_root: PathBuf,
    },
    ActiveOrStaleBoardEntry {
        entry_id: Uuid,
        status: String,
        agent_id: String,
    },
    ActiveLease {
        ticket_id: Uuid,
        working_by: String,
    },
    InvisibleTicketReference {
        related_ticket_id: Uuid,
        direction: MoveReferenceDirection,
    },
    DirtyTrackedFiles {
        files: Vec<PathBuf>,
    },
    PathReferenceScanUnavailable {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovePreflightReport {
    pub ticket_id: Uuid,
    pub source_workspace_root: PathBuf,
    pub target_workspace_root: PathBuf,
    pub source_store_root: PathBuf,
    pub target_store_root: PathBuf,
    pub source_ticket_path: PathBuf,
    pub destination_ticket_path: PathBuf,
    pub source_ticket: Option<IndexedTicket>,
    pub target_ticket: Option<IndexedTicket>,
    pub inbound_related_ticket_ids: Vec<Uuid>,
    pub outbound_related_ticket_ids: Vec<Uuid>,
    pub reference_visibility: Vec<MoveReferenceVisibility>,
    pub active_board_entries: Vec<BoardEntry>,
    pub historical_board_entries: Vec<BoardEntry>,
    pub active_leases: Vec<LeaseInfo>,
    pub path_reference_files: Vec<PathBuf>,
    pub blockers: Vec<MovePreflightBlocker>,
    pub captured_at: chrono::DateTime<Utc>,
}

impl MovePreflightReport {
    pub fn supported(&self) -> bool {
        self.blockers.is_empty()
    }
}

impl TicketStore {
    pub fn plan_move_preflight(
        &self,
        ticket_id: &Uuid,
        target_workspace_root: &Path,
    ) -> Result<MovePreflightReport, StorageError> {
        let source_workspace_root =
            workspace::resolve_workspace_root_from_store_root(&self.index_root, workspace::TICKET_INDEX_DIR);
        let source_store_root = self.index_root.clone();
        let target_store_root = workspace::resolve_store_root_from(
            target_workspace_root,
            workspace::TICKET_INDEX_DIR,
        );
        let source_ticket = self.get_indexed(ticket_id)?;
        let source_ticket_path = source_ticket
            .as_ref()
            .map(|ticket| ticket.path.clone())
            .unwrap_or_else(|| source_store_root.join("tickets").join(ticket_id.to_string()));
        let destination_ticket_path = target_store_root
            .join("tickets")
            .join(ticket_id.to_string());

        let mut blockers = Vec::new();

        let source_git_root = git_toplevel(&source_workspace_root)
            .map_err(|reason| StorageError::Other(reason))?;
        let target_git_root = match git_toplevel(target_workspace_root) {
            Ok(root) => root,
            Err(reason) => {
                blockers.push(MovePreflightBlocker::PathReferenceScanUnavailable { reason });
                source_git_root.clone()
            },
        };

        if source_git_root != target_git_root {
            blockers.push(MovePreflightBlocker::DifferentGitWorktree {
                source_worktree_root: source_git_root.clone(),
                target_worktree_root: target_git_root.clone(),
            });
        }

        let target_store = match TicketStore::open_with(
            &target_store_root,
            self.schema_registry().clone(),
        ) {
            Ok(store) => Some(store),
            Err(StorageError::WorkspaceNotFound { path }) => {
                blockers.push(MovePreflightBlocker::MissingTargetStore {
                    target_store_root: path,
                });
                None
            },
            Err(error) => return Err(error),
        };

        if source_ticket.is_none() {
            blockers.push(MovePreflightBlocker::MissingSourceTicket { ticket_id: *ticket_id });
        }

        let all_edges = self.list_all_edges()?;
        let mut inbound_related_ticket_ids = BTreeSet::new();
        let mut outbound_related_ticket_ids = BTreeSet::new();
        for edge in &all_edges {
            if edge.from == *ticket_id {
                outbound_related_ticket_ids.insert(edge.to);
            }
            if edge.to == *ticket_id {
                inbound_related_ticket_ids.insert(edge.from);
            }
        }

        let mut reference_visibility = Vec::new();
        if let Some(target_store) = target_store.as_ref() {
            for related_ticket_id in inbound_related_ticket_ids
                .iter()
                .chain(outbound_related_ticket_ids.iter())
                .copied()
            {
                let visible_from_destination = target_store.get_indexed(&related_ticket_id)?.is_some();
                let direction = if outbound_related_ticket_ids.contains(&related_ticket_id) {
                    MoveReferenceDirection::Outbound
                } else {
                    MoveReferenceDirection::Inbound
                };
                if !visible_from_destination {
                    blockers.push(MovePreflightBlocker::InvisibleTicketReference {
                        related_ticket_id,
                        direction: direction.clone(),
                    });
                }
                reference_visibility.push(MoveReferenceVisibility {
                    related_ticket_id,
                    direction,
                    visible_from_destination,
                });
            }
        }

        let mut active_board_entries = Vec::new();
        let mut historical_board_entries = Vec::new();
        let mut active_leases = Vec::new();
        let board_snapshot = self.board_show(None).map_err(|error| match error {
            crate::storage::BoardError::Storage(storage_error) => storage_error,
            other => StorageError::Other(other.to_string()),
        })?;
        for entry in board_snapshot.entries {
            if entry.ticket_id == *ticket_id {
                if entry.status == crate::storage::BoardEntryStatus::Active
                    || entry.status == crate::storage::BoardEntryStatus::Stale
                {
                    blockers.push(MovePreflightBlocker::ActiveOrStaleBoardEntry {
                        entry_id: entry.entry_id,
                        status: format!("{:?}", entry.status),
                        agent_id: entry.agent_id.clone(),
                    });
                }
                active_board_entries.push(entry);
            }
        }
        let history_snapshot = self.board_history(None).map_err(|error| match error {
            crate::storage::BoardError::Storage(storage_error) => storage_error,
            other => StorageError::Other(other.to_string()),
        })?;
        for entry in history_snapshot.entries {
            if entry.ticket_id == *ticket_id {
                historical_board_entries.push(entry);
            }
        }
        for lease in self.list_leases()? {
            if lease.ticket_id == *ticket_id {
                blockers.push(MovePreflightBlocker::ActiveLease {
                    ticket_id: lease.ticket_id,
                    working_by: lease.working_by.clone(),
                });
                active_leases.push(lease);
            }
        }

        let path_reference_files = if source_ticket.is_some() {
            match git_tracked_path_reference_files(
                &source_git_root,
                &source_ticket_path,
            ) {
                Ok(files) => files,
                Err(reason) => {
                    blockers.push(MovePreflightBlocker::PathReferenceScanUnavailable { reason });
                    Vec::new()
                },
            }
        } else {
            Vec::new()
        };

        if !path_reference_files.is_empty() {
            let dirty_files = git_dirty_tracked_files(&source_git_root, &path_reference_files)
                .unwrap_or_else(|reason| {
                    blockers.push(MovePreflightBlocker::PathReferenceScanUnavailable { reason });
                    Vec::new()
                });
            if !dirty_files.is_empty() {
                blockers.push(MovePreflightBlocker::DirtyTrackedFiles { files: dirty_files });
            }
        }

        Ok(MovePreflightReport {
            ticket_id: *ticket_id,
            source_workspace_root,
            target_workspace_root: target_workspace_root.to_path_buf(),
            source_store_root,
            target_store_root,
            source_ticket_path,
            destination_ticket_path,
            source_ticket,
            target_ticket: target_store
                .as_ref()
                .and_then(|store| store.get_indexed(ticket_id).ok().flatten()),
            inbound_related_ticket_ids: inbound_related_ticket_ids.into_iter().collect(),
            outbound_related_ticket_ids: outbound_related_ticket_ids.into_iter().collect(),
            reference_visibility,
            active_board_entries,
            historical_board_entries,
            active_leases,
            path_reference_files,
            blockers,
            captured_at: Utc::now(),
        })
    }
}

fn git_toplevel(path: &Path) -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args(["-C", &path.to_string_lossy(), "rev-parse", "--show-toplevel"])
        .output()
        .map_err(|error| error.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if stdout.is_empty() {
        return Err("git rev-parse returned an empty worktree root".to_string());
    }

    Ok(PathBuf::from(stdout))
}

fn git_tracked_path_reference_files(
    repo_root: &Path,
    ticket_path: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut candidates = BTreeSet::new();
    candidates.insert(ticket_path.to_string_lossy().replace('\\', "/"));
    if let Ok(relative) = ticket_path.strip_prefix(repo_root) {
        candidates.insert(relative.to_string_lossy().replace('\\', "/"));
    }

    let mut files = BTreeSet::new();
    for candidate in candidates {
        let output = Command::new("git")
            .args(["-C", &repo_root.to_string_lossy(), "grep", "-nF", "--full-name", &candidate])
            .output()
            .map_err(|error| error.to_string())?;

        if !output.status.success() && output.status.code() != Some(1) {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Some((file, _)) = line.split_once(':') {
                files.insert(PathBuf::from(file));
            }
        }
    }

    Ok(files.into_iter().collect())
}

fn git_dirty_tracked_files(
    repo_root: &Path,
    files: &[PathBuf],
) -> Result<Vec<PathBuf>, String> {
    let mut dirty = Vec::new();
    for file in files {
        let output = Command::new("git")
            .args([
                "-C",
                &repo_root.to_string_lossy(),
                "status",
                "--porcelain",
                "--",
                &file.to_string_lossy(),
            ])
            .output()
            .map_err(|error| error.to_string())?;

        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
        }

        if !String::from_utf8_lossy(&output.stdout).trim().is_empty() {
            dirty.push(file.clone());
        }
    }

    Ok(dirty)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::edge::EdgeRecord;
    use std::{fs, process::Command};
    use tempfile::tempdir;

    fn run_git(repo_root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .status()
            .expect("git command");
        assert!(status.success(), "git {args:?} failed: {status}");
    }

    #[test]
    fn preflight_reports_invisible_reference_and_path_refs() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source-workspace");
        let target_workspace = repo.join("target-workspace");
        let docs_dir = repo.join("docs");
        fs::create_dir_all(&source_workspace).unwrap();
        fs::create_dir_all(&target_workspace).unwrap();
        fs::create_dir_all(&docs_dir).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let target_store = TicketStore::init(&target_workspace).unwrap();

        let source_ticket = source_store
            .create(
                None,
                "tracker-improvement",
                Some("source ticket"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();
        let invisible_inbound = source_store
            .create(
                None,
                "tracker-improvement",
                Some("invisible inbound"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let destination_visible = target_store
            .create(
                None,
                "tracker-improvement",
                Some("destination visible"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        source_store
            .add_edge(EdgeRecord {
                from: invisible_inbound,
                to: source_ticket,
                kind: "depends_on".to_string(),
                created_at: Utc::now(),
            })
            .unwrap();
        source_store
            .add_edge(EdgeRecord {
                from: source_ticket,
                to: destination_visible,
                kind: "depends_on".to_string(),
                created_at: Utc::now(),
            })
            .unwrap();

        let source_ticket_path = source_store
            .get_indexed(&source_ticket)
            .unwrap()
            .unwrap()
            .path;
        let relative_ticket_path = source_ticket_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let tracked_doc = docs_dir.join("move.md");
        fs::write(&tracked_doc, format!("See {relative_ticket_path}\n")).unwrap();
        run_git(&repo, &["add", "docs/move.md"]);

        let report = source_store
            .plan_move_preflight(&source_ticket, &target_workspace)
            .unwrap();

        assert!(!report.supported());
        assert!(report.reference_visibility.iter().any(|entry|
            entry.related_ticket_id == invisible_inbound
                && entry.direction == MoveReferenceDirection::Inbound
                && !entry.visible_from_destination
        ));
        assert!(report.path_reference_files.iter().any(|path| {
            path.ends_with("docs/move.md")
        }));
        assert!(report.blockers.iter().any(|blocker| matches!(
            blocker,
            MovePreflightBlocker::InvisibleTicketReference {
                related_ticket_id,
                direction: MoveReferenceDirection::Inbound,
            } if *related_ticket_id == invisible_inbound
        )));
    }
}