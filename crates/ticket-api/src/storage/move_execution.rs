use std::{
    collections::BTreeSet,
    fs,
    path::{
        PathBuf,
    },
};

use chrono::Utc;
use serde::{
    Deserialize,
    Serialize,
};
use uuid::Uuid;

use crate::{
    error::StorageError,
    storage::{
        BoardEntry,
        BoardEntryStatus,
        move_planner::MovePreflightReport,
        store::TicketStore,
    },
    workspace,
};

const MOVE_LOCKS_DIR: &str = "move-locks";
const MOVE_JOURNALS_DIR: &str = "move-journals";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovePathRewrite {
    pub path: PathBuf,
    pub previous_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveManualFollowup {
    pub path: PathBuf,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MoveExecutionPhase {
    Planned,
    Locked,
    Moved,
    SourceScanned,
    TargetScanned,
    Validated,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveJournal {
    pub id: Uuid,
    pub ticket_id: Uuid,
    pub source_store_root: PathBuf,
    pub target_store_root: PathBuf,
    pub source_ticket_path: PathBuf,
    pub destination_ticket_path: PathBuf,
    pub phase: MoveExecutionPhase,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
    pub steps: Vec<String>,
    pub rollback_steps: Vec<String>,
    #[serde(default)]
    pub lock_paths: Vec<PathBuf>,
    #[serde(default)]
    pub migrated_board_entries: Vec<BoardEntry>,
    #[serde(default)]
    pub rewritten_path_files: Vec<MovePathRewrite>,
    #[serde(default)]
    pub manual_followups: Vec<MoveManualFollowup>,
    pub failure: Option<String>,
    #[serde(default)]
    pub next_recovery_step: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveExecutionOutcome {
    pub journal: MoveJournal,
    pub resumed: bool,
    pub rolled_back: bool,
}

impl TicketStore {
    pub fn execute_move_with_journal(
        &self,
        plan: &MovePreflightReport,
    ) -> Result<MoveExecutionOutcome, StorageError> {
        if !plan.supported() {
            return Err(StorageError::Other(
                "move preflight contains blockers".to_string(),
            ));
        }
        self.execute_or_resume_journal(plan, None, false)
    }

    pub fn resume_move_with_journal(
        &self,
        journal_id: Uuid,
    ) -> Result<MoveExecutionOutcome, StorageError> {
        let journal = self.load_journal(journal_id)?;
        let plan = self.plan_move_preflight(&journal.ticket_id, &workspace::resolve_workspace_root_from_store_root(
            &journal.target_store_root,
            workspace::TICKET_INDEX_DIR,
        ))?;
        self.execute_or_resume_journal(&plan, Some(journal), true)
    }

    pub fn rollback_move_with_journal(
        &self,
        journal_id: Uuid,
    ) -> Result<MoveExecutionOutcome, StorageError> {
        let mut journal = self.load_journal(journal_id)?;
        if journal.lock_paths.is_empty() {
            journal.lock_paths = Self::collect_lock_paths(
                journal.ticket_id,
                &journal.source_store_root,
                &journal.target_store_root,
            );
        }
        self.acquire_lock_set(&journal.lock_paths)?;

        if journal.destination_ticket_path.exists() && !journal.source_ticket_path.exists() {
            if let Some(parent) = journal.source_ticket_path.parent() {
                fs::create_dir_all(parent).map_err(StorageError::Io)?;
            }
            fs::rename(&journal.destination_ticket_path, &journal.source_ticket_path)
                .map_err(StorageError::Io)?;
        }

        let source_store = TicketStore::open_with(
            &journal.source_store_root,
            self.schema_registry().clone(),
        )?;
        let target_store = TicketStore::open_with(
            &journal.target_store_root,
            self.schema_registry().clone(),
        )?;

        for rewrite in &journal.rewritten_path_files {
            fs::write(&rewrite.path, rewrite.previous_content.as_bytes()).map_err(StorageError::Io)?;
        }

        if !journal.migrated_board_entries.is_empty() {
            source_store
                .board_import_entries(&journal.migrated_board_entries)
                .map_err(Self::map_board_error)?;
            let migrated_ids: Vec<Uuid> = journal
                .migrated_board_entries
                .iter()
                .map(|entry| entry.entry_id)
                .collect();
            target_store
                .board_delete_entries(&migrated_ids)
                .map_err(Self::map_board_error)?;
        }

        source_store.scan(true)?;
        target_store.scan(true)?;

        journal.phase = MoveExecutionPhase::RolledBack;
        journal.updated_at = Utc::now();
        journal
            .steps
            .push("rolled back ticket folder to source store".to_string());
        journal.failure = None;
        journal.next_recovery_step = None;
        self.persist_journal(&journal)?;
        self.release_lock_set(&journal.lock_paths);

        Ok(MoveExecutionOutcome {
            journal,
            resumed: false,
            rolled_back: true,
        })
    }

    fn execute_or_resume_journal(
        &self,
        plan: &MovePreflightReport,
        existing: Option<MoveJournal>,
        resumed: bool,
    ) -> Result<MoveExecutionOutcome, StorageError> {
        let mut journal = existing.unwrap_or_else(|| MoveJournal {
            id: Uuid::new_v4(),
            ticket_id: plan.ticket_id,
            source_store_root: plan.source_store_root.clone(),
            target_store_root: plan.target_store_root.clone(),
            source_ticket_path: plan.source_ticket_path.clone(),
            destination_ticket_path: plan.destination_ticket_path.clone(),
            phase: MoveExecutionPhase::Planned,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            steps: vec!["created move journal".to_string()],
            rollback_steps: vec![
                "rename destination ticket folder back to source path".to_string(),
                "restore migrated board history rows to source store".to_string(),
                "scan source and target stores".to_string(),
            ],
            lock_paths: Self::collect_lock_paths(
                plan.ticket_id,
                &plan.source_store_root,
                &plan.target_store_root,
            ),
            migrated_board_entries: Vec::new(),
            rewritten_path_files: Vec::new(),
            manual_followups: Vec::new(),
            failure: None,
            next_recovery_step: None,
        });
        if journal.lock_paths.is_empty() {
            journal.lock_paths = Self::collect_lock_paths(
                journal.ticket_id,
                &journal.source_store_root,
                &journal.target_store_root,
            );
        }
        self.persist_journal(&journal)?;

        let result: Result<(), StorageError> = (|| {
            if journal.phase == MoveExecutionPhase::Planned {
                self.acquire_lock_set(&journal.lock_paths)?;
                journal.phase = MoveExecutionPhase::Locked;
                journal.updated_at = Utc::now();
                journal
                    .steps
                    .push("acquired source/target store locks and move ticket lock".to_string());
                self.persist_journal(&journal)?;
            }

            if journal.phase == MoveExecutionPhase::Locked {
                if let Some(parent) = journal.destination_ticket_path.parent() {
                    fs::create_dir_all(parent).map_err(StorageError::Io)?;
                }
                if journal.source_ticket_path.exists() {
                    fs::rename(&journal.source_ticket_path, &journal.destination_ticket_path)
                        .map_err(StorageError::Io)?;
                }
                journal.phase = MoveExecutionPhase::Moved;
                journal.updated_at = Utc::now();
                journal.steps.push("moved ticket folder".to_string());
                self.persist_journal(&journal)?;
            }

            if journal.phase == MoveExecutionPhase::Moved {
                let source_store = TicketStore::open_with(
                    &journal.source_store_root,
                    self.schema_registry().clone(),
                )?;
                let target_store = TicketStore::open_with(
                    &journal.target_store_root,
                    self.schema_registry().clone(),
                )?;

                if journal.rewritten_path_files.is_empty() && journal.manual_followups.is_empty() {
                    let (rewritten, followups) = Self::rewrite_path_references(plan)?;
                    if !rewritten.is_empty() {
                        journal
                            .steps
                            .push(format!("rewrote {} tracked path reference files", rewritten.len()));
                    }
                    if !followups.is_empty() {
                        journal.steps.push(format!(
                            "recorded {} manual path-reference follow-ups",
                            followups.len()
                        ));
                    }
                    journal.rewritten_path_files = rewritten;
                    journal.manual_followups = followups;
                }

                if journal.migrated_board_entries.is_empty() {
                    journal.migrated_board_entries = Self::migrate_historical_board_entries(
                        &source_store,
                        &target_store,
                        journal.ticket_id,
                    )?;
                    if !journal.migrated_board_entries.is_empty() {
                        journal.steps.push(format!(
                            "migrated {} historical board rows",
                            journal.migrated_board_entries.len()
                        ));
                    }
                }

                source_store.scan(true)?;
                journal.phase = MoveExecutionPhase::SourceScanned;
                journal.updated_at = Utc::now();
                journal.steps.push("scanned source store".to_string());
                self.persist_journal(&journal)?;
            }

            if journal.phase == MoveExecutionPhase::SourceScanned {
                let target_store = TicketStore::open_with(
                    &journal.target_store_root,
                    self.schema_registry().clone(),
                )?;
                target_store.scan(true)?;
                journal.phase = MoveExecutionPhase::TargetScanned;
                journal.updated_at = Utc::now();
                journal.steps.push("scanned target store".to_string());
                self.persist_journal(&journal)?;
            }

            if journal.phase == MoveExecutionPhase::TargetScanned {
                let source_store = TicketStore::open_with(
                    &journal.source_store_root,
                    self.schema_registry().clone(),
                )?;
                let target_store = TicketStore::open_with(
                    &journal.target_store_root,
                    self.schema_registry().clone(),
                )?;
                let source_seen = source_store.get_indexed(&journal.ticket_id)?;
                let target_seen = target_store.get_indexed(&journal.ticket_id)?;
                if source_seen.is_some() || target_seen.is_none() {
                    let mut problems = Vec::new();
                    if source_seen.is_some() {
                        problems.push(format!(
                            "source store {} still indexes ticket {} after the move (source ticket folder {} should no longer exist)",
                            Self::normalize_slashes(&journal.source_store_root),
                            journal.ticket_id,
                            Self::normalize_slashes(&journal.source_ticket_path),
                        ));
                    }
                    if target_seen.is_none() {
                        problems.push(format!(
                            "destination store {} does not index ticket {} after the move (expected ticket folder {} — check that the destination store root resolved without a Windows verbatim `\\\\?\\` prefix)",
                            Self::normalize_slashes(&journal.target_store_root),
                            journal.ticket_id,
                            Self::normalize_slashes(&journal.destination_ticket_path),
                        ));
                    }
                    return Err(StorageError::Other(format!(
                        "post-move validation failed: {}",
                        problems.join("; ")
                    )));
                }

                journal.phase = MoveExecutionPhase::Validated;
                journal.updated_at = Utc::now();
                journal.steps.push("validated move ownership".to_string());
                journal.failure = None;
                journal.next_recovery_step = None;
                self.persist_journal(&journal)?;
            }

            Ok(())
        })();

        match result {
            Ok(()) => {
                self.release_lock_set(&journal.lock_paths);
                Ok(MoveExecutionOutcome {
                    journal,
                    resumed,
                    rolled_back: false,
                })
            }
            Err(error) => {
                journal.updated_at = Utc::now();
                journal.failure = Some(error.to_string());
                journal.next_recovery_step = Some(Self::recovery_hint_for_phase(&journal.phase).to_string());
                let _ = self.persist_journal(&journal);
                self.release_lock_set(&journal.lock_paths);
                Err(error)
            }
        }
    }

    fn collect_lock_paths(
        ticket_id: Uuid,
        source_store_root: &std::path::Path,
        target_store_root: &std::path::Path,
    ) -> Vec<PathBuf> {
        let mut paths = BTreeSet::new();
        for root in [source_store_root, target_store_root] {
            paths.insert(root.join(MOVE_LOCKS_DIR).join("store.lock"));
            paths.insert(root.join(MOVE_LOCKS_DIR).join(format!("ticket-{}.lock", ticket_id)));
        }
        paths.into_iter().collect()
    }

    fn acquire_lock_set(
        &self,
        lock_paths: &[PathBuf],
    ) -> Result<(), StorageError> {
        let mut acquired = Vec::new();
        for path in lock_paths {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(StorageError::Io)?;
            }
            match fs::OpenOptions::new().write(true).create_new(true).open(path) {
                Ok(_) => acquired.push(path.clone()),
                Err(error) => {
                    self.release_lock_set(&acquired);
                    return Err(StorageError::Other(format!(
                        "move lock already held at {}: {}",
                        path.display(),
                        error
                    )));
                }
            }
        }
        Ok(())
    }

    fn release_lock_set(
        &self,
        lock_paths: &[PathBuf],
    ) {
        for path in lock_paths {
            let _ = fs::remove_file(path);
        }
    }

    fn recovery_hint_for_phase(phase: &MoveExecutionPhase) -> &'static str {
        match phase {
            MoveExecutionPhase::Planned | MoveExecutionPhase::Locked => {
                "run resume_move_with_journal to continue execution"
            }
            MoveExecutionPhase::Moved
            | MoveExecutionPhase::SourceScanned
            | MoveExecutionPhase::TargetScanned => {
                "run rollback_move_with_journal for safety, or resume_move_with_journal to retry"
            }
            MoveExecutionPhase::Validated | MoveExecutionPhase::RolledBack => {
                "no recovery action needed"
            }
        }
    }

    fn migrate_historical_board_entries(
        source_store: &TicketStore,
        target_store: &TicketStore,
        ticket_id: Uuid,
    ) -> Result<Vec<BoardEntry>, StorageError> {
        let entries = source_store
            .board_list_entries_for_ticket(&ticket_id)
            .map_err(Self::map_board_error)?;

        let mut historical_entries = Vec::new();
        for entry in entries {
            if entry.status == BoardEntryStatus::Active || entry.status == BoardEntryStatus::Stale {
                return Err(StorageError::Other(format!(
                    "cannot move ticket {} while board entry {} is active/stale",
                    ticket_id, entry.entry_id
                )));
            }
            historical_entries.push(entry);
        }

        if historical_entries.is_empty() {
            return Ok(Vec::new());
        }

        target_store
            .board_import_entries(&historical_entries)
            .map_err(Self::map_board_error)?;
        let ids: Vec<Uuid> = historical_entries.iter().map(|entry| entry.entry_id).collect();
        source_store
            .board_delete_entries(&ids)
            .map_err(Self::map_board_error)?;

        Ok(historical_entries)
    }

    fn map_board_error(error: crate::storage::BoardError) -> StorageError {
        match error {
            crate::storage::BoardError::Storage(storage_error) => storage_error,
            other => StorageError::Other(other.to_string()),
        }
    }

    fn rewrite_path_references(
        plan: &MovePreflightReport,
    ) -> Result<(Vec<MovePathRewrite>, Vec<MoveManualFollowup>), StorageError> {
        let old_abs = Self::normalize_slashes(&plan.source_ticket_path);
        let new_abs = Self::normalize_slashes(&plan.destination_ticket_path);

        let mut relative_pairs = Vec::new();
        if let (Ok(old_rel), Ok(new_rel)) = (
            plan.source_ticket_path
                .strip_prefix(&plan.source_git_worktree_root),
            plan.destination_ticket_path
                .strip_prefix(&plan.source_git_worktree_root),
        ) {
            relative_pairs.push((
                Self::normalize_slashes(old_rel),
                Self::normalize_slashes(new_rel),
            ));
        }
        if let (Ok(old_rel), Ok(new_rel)) = (
            plan.source_ticket_path
                .strip_prefix(&plan.target_git_worktree_root),
            plan.destination_ticket_path
                .strip_prefix(&plan.target_git_worktree_root),
        ) {
            relative_pairs.push((
                Self::normalize_slashes(old_rel),
                Self::normalize_slashes(new_rel),
            ));
        }

        let mut rewritten = Vec::new();
        let mut followups = Vec::new();

        for file in &plan.path_reference_files {
            let file_path = file.clone();
            if !file_path.exists() {
                followups.push(MoveManualFollowup {
                    path: file_path,
                    reason: "tracked reference file missing on disk".to_string(),
                });
                continue;
            }

            let bytes = fs::read(&file_path).map_err(StorageError::Io)?;
            let Ok(previous_content) = String::from_utf8(bytes) else {
                followups.push(MoveManualFollowup {
                    path: file_path,
                    reason: "binary or non-utf8 content requires manual rewrite".to_string(),
                });
                continue;
            };

            let mut replaced = previous_content.replace(&old_abs, &new_abs);
            for (old_rel, new_rel) in &relative_pairs {
                if !old_rel.is_empty() {
                    replaced = replaced.replace(old_rel, new_rel);
                }
            }

            if replaced == previous_content {
                followups.push(MoveManualFollowup {
                    path: file_path,
                    reason: "no rewrite candidate matched file content".to_string(),
                });
                continue;
            }

            fs::write(&file_path, replaced.as_bytes()).map_err(StorageError::Io)?;
            rewritten.push(MovePathRewrite {
                path: file_path,
                previous_content,
            });
        }

        Ok((rewritten, followups))
    }

    fn normalize_slashes(path: &std::path::Path) -> String {
        let raw = path.to_string_lossy().replace('\\', "/");
        raw.strip_prefix("//?/").unwrap_or(&raw).to_string()
    }

    fn journal_path(
        &self,
        id: Uuid,
    ) -> PathBuf {
        self.index_root
            .join(MOVE_JOURNALS_DIR)
            .join(format!("{}.json", id))
    }

    fn persist_journal(
        &self,
        journal: &MoveJournal,
    ) -> Result<(), StorageError> {
        let path = self.journal_path(journal.id);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(StorageError::Io)?;
        }
        let payload = serde_json::to_vec_pretty(journal)
            .map_err(|error| StorageError::Other(error.to_string()))?;
        fs::write(path, payload).map_err(StorageError::Io)
    }

    fn load_journal(
        &self,
        id: Uuid,
    ) -> Result<MoveJournal, StorageError> {
        let payload = fs::read(self.journal_path(id)).map_err(StorageError::Io)?;
        serde_json::from_slice(&payload).map_err(|error| StorageError::Other(error.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::move_planner::MovePreflightBlocker;
    use std::process::Command;
    use tempfile::tempdir;

    fn run_git(repo_root: &std::path::Path, args: &[&str]) {
        let status = Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .status()
            .expect("git command");
        assert!(status.success(), "git {args:?} failed: {status}");
    }

    #[test]
    fn execute_move_with_journal_moves_ticket_between_stores() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let _target_store = TicketStore::init(&target_workspace).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let outcome = source_store.execute_move_with_journal(&plan).unwrap();
        assert_eq!(outcome.journal.phase, MoveExecutionPhase::Validated);

        let src = TicketStore::open(&source_workspace).unwrap();
        let dst = TicketStore::open(&target_workspace).unwrap();
        assert!(src.get_indexed(&id).unwrap().is_none());
        assert!(dst.get_indexed(&id).unwrap().is_some());
    }

    #[test]
    fn rollback_move_with_journal_restores_source_location() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let _target_store = TicketStore::init(&target_workspace).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let outcome = source_store.execute_move_with_journal(&plan).unwrap();
        let journal_id = outcome.journal.id;

        let _ = source_store.rollback_move_with_journal(journal_id).unwrap();

        let src = TicketStore::open(&source_workspace).unwrap();
        let dst = TicketStore::open(&target_workspace).unwrap();
        assert!(src.get_indexed(&id).unwrap().is_some());
        assert!(dst.get_indexed(&id).unwrap().is_none());
    }

    #[test]
    fn execute_move_with_journal_fails_when_board_entry_is_active() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let _target_store = TicketStore::init(&target_workspace).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        source_store
            .board_check_in(&id, "agent-a", 300, "working", Vec::new())
            .unwrap();

        let plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        assert!(!plan.supported());

        let error = source_store.execute_move_with_journal(&plan).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("move preflight contains blockers")
        );
        assert!(plan.source_ticket_path.exists());
        assert!(!plan.destination_ticket_path.exists());
    }

    #[test]
    fn execute_move_with_journal_migrates_historical_board_rows() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let target_store = TicketStore::init(&target_workspace).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        source_store
            .board_check_in(&id, "agent-a", 300, "working", Vec::new())
            .unwrap();
        source_store
            .board_check_out(&id, "agent-a", Some("done"))
            .unwrap();

        let mut plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let outcome = source_store.execute_move_with_journal(&plan).unwrap();
        assert_eq!(outcome.journal.phase, MoveExecutionPhase::Validated);
        assert_eq!(outcome.journal.migrated_board_entries.len(), 1);

        let src_entries = source_store.board_list_entries_for_ticket(&id).unwrap();
        let dst_entries = target_store.board_list_entries_for_ticket(&id).unwrap();
        assert!(src_entries.is_empty());
        assert_eq!(dst_entries.len(), 1);
        assert_eq!(dst_entries[0].status, BoardEntryStatus::Completed);
    }

    #[test]
    fn execute_move_with_journal_rewrites_path_references_and_rollback_restores() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let _target_store = TicketStore::init(&target_workspace).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        let source_rel = plan
            .source_ticket_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let destination_rel = plan
            .destination_ticket_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        let doc_path = repo.join("docs").join("ticket-path.md");
        std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
        std::fs::write(&doc_path, format!("ticket path: {}\n", source_rel)).unwrap();
        run_git(&repo, &["add", "docs/ticket-path.md"]);

        plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let outcome = source_store.execute_move_with_journal(&plan).unwrap();
        assert!(!outcome.journal.rewritten_path_files.is_empty());
        let rewritten_doc = std::fs::read_to_string(&doc_path).unwrap();
        assert!(rewritten_doc.contains(&destination_rel));

        let _rollback = source_store
            .rollback_move_with_journal(outcome.journal.id)
            .unwrap();
        let restored_doc = std::fs::read_to_string(&doc_path).unwrap();
        assert!(restored_doc.contains(&source_rel));
    }

    #[test]
    fn execute_move_with_journal_rewrites_parent_repo_refs_for_submodule_source() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        let nested_repo = repo.join("nested-repo");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::create_dir_all(&nested_repo).unwrap();
        run_git(&repo, &["init"]);
        run_git(&nested_repo, &["init"]);

        let source_store = TicketStore::init(&nested_repo).unwrap();
        let _target_store = TicketStore::init(&repo).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut plan = source_store.plan_move_preflight(&id, &repo).unwrap();
        let source_rel_from_parent = plan
            .source_ticket_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let destination_rel_from_parent = plan
            .destination_ticket_path
            .strip_prefix(&repo)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");

        let doc_path = repo.join("docs").join("submodule-ticket-path.md");
        std::fs::create_dir_all(doc_path.parent().unwrap()).unwrap();
        std::fs::write(
            &doc_path,
            format!("ticket path from parent repo: {}\n", source_rel_from_parent),
        )
        .unwrap();
        run_git(&repo, &["add", "docs/submodule-ticket-path.md"]);

        plan = source_store.plan_move_preflight(&id, &repo).unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let outcome = source_store.execute_move_with_journal(&plan).unwrap();
        assert!(!outcome.journal.rewritten_path_files.is_empty());

        let rewritten_doc = std::fs::read_to_string(&doc_path).unwrap();
        assert!(rewritten_doc.contains(&destination_rel_from_parent));
        assert!(!rewritten_doc.contains(&source_rel_from_parent));
    }

    #[test]
    fn resume_move_with_journal_continues_from_locked_phase() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let source_store = TicketStore::init(&source_workspace).unwrap();
        let _target_store = TicketStore::init(&target_workspace).unwrap();

        let id = source_store
            .create(
                None,
                "tracker-improvement",
                Some("move me"),
                Some("ready"),
                Default::default(),
                None,
                None,
            )
            .unwrap();

        let mut plan = source_store
            .plan_move_preflight(&id, &target_workspace)
            .unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MovePreflightBlocker::PathReferenceScanUnavailable { .. }
                    | MovePreflightBlocker::DirtyTrackedFiles { .. }
            )
        });

        let source_ticket = plan.source_ticket_path.clone();
        let destination_ticket = plan.destination_ticket_path.clone();
        let journal_id = Uuid::new_v4();
        let lock_paths = TicketStore::collect_lock_paths(
            id,
            &plan.source_store_root,
            &plan.target_store_root,
        );
        let journal = MoveJournal {
            id: journal_id,
            ticket_id: id,
            source_store_root: plan.source_store_root.clone(),
            target_store_root: plan.target_store_root.clone(),
            source_ticket_path: source_ticket,
            destination_ticket_path: destination_ticket,
            phase: MoveExecutionPhase::Locked,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            steps: vec!["created move journal".to_string()],
            rollback_steps: vec!["rename destination ticket folder back to source path".to_string()],
            lock_paths,
            migrated_board_entries: Vec::new(),
            rewritten_path_files: Vec::new(),
            manual_followups: Vec::new(),
            failure: None,
            next_recovery_step: None,
        };
        source_store.persist_journal(&journal).unwrap();

        let outcome = source_store.resume_move_with_journal(journal_id).unwrap();
        assert!(outcome.resumed);
        assert_eq!(outcome.journal.phase, MoveExecutionPhase::Validated);

        let src = TicketStore::open(&source_workspace).unwrap();
        let dst = TicketStore::open(&target_workspace).unwrap();
        assert!(src.get_indexed(&id).unwrap().is_none());
        assert!(dst.get_indexed(&id).unwrap().is_some());
    }
}
