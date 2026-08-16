//! Rule-domain adapter onto the domain-neutral move kernel.
//!
//! Mirrors the ticket and spec adapters: the rule store implements
//! [`MoveDomain`] and gains the same safe preflight/journaled move featureset
//! without copying any move logic. Rules have no board or lease model, so the
//! kernel's default no-op board/lease hooks apply.

use std::path::{
    Path,
    PathBuf,
};

use memory_kernel::{
    error::StorageError,
    storage::move_kernel::{
        self,
        MoveDomain,
        MoveError,
        MoveOutcome,
        MovePlan,
        MoveReferences,
        MoveResult,
    },
};
use uuid::Uuid;

use crate::{
    error::RuleError,
    store::RuleStore,
};

const RULE_INDEX_DIR: &str = ".rule";

fn to_move_error(error: RuleError) -> MoveError {
    match error {
        RuleError::Storage(StorageError::Io(io)) => MoveError::Io(io),
        other => MoveError::Domain(other.to_string()),
    }
}

fn rule_entity_root(store_root: &Path) -> PathBuf {
    memory_kernel::workspace::resolve_store_root_from(
        store_root,
        RULE_INDEX_DIR,
    )
    .join("rules")
}

fn from_move_error(error: MoveError) -> RuleError {
    match error {
        MoveError::Io(io) => RuleError::Storage(StorageError::Io(io)),
        MoveError::Domain(message) =>
            RuleError::Storage(StorageError::Other(message)),
        MoveError::InteroperabilityContract {
            artifact_class,
            detail,
        } => RuleError::Storage(StorageError::Other(format!(
            "interoperability contract violation for {artifact_class}: {detail}"
        ))),
    }
}

/// Rule-domain implementation of the move kernel's [`MoveDomain`] trait.
pub struct RuleMoveDomain<'a> {
    store: &'a RuleStore,
}

impl<'a> RuleMoveDomain<'a> {
    pub fn new(store: &'a RuleStore) -> Self {
        Self { store }
    }
}

impl MoveDomain for RuleMoveDomain<'_> {
    fn entity_subdir(&self) -> &str {
        "rules"
    }

    fn store_index_dir(&self) -> &str {
        RULE_INDEX_DIR
    }

    fn source_store_root(&self) -> PathBuf {
        self.store.entity_store().index_root.clone()
    }

    fn source_entity_path(
        &self,
        entity_id: &Uuid,
    ) -> MoveResult<Option<PathBuf>> {
        Ok(self
            .store
            .entity_store()
            .get_indexed(entity_id)
            .map_err(|error| to_move_error(error.into()))?
            .map(|entity| entity.path))
    }

    fn related_entities(
        &self,
        entity_id: &Uuid,
    ) -> MoveResult<MoveReferences> {
        let mut references = MoveReferences::default();
        for edge in self
            .store
            .entity_store()
            .list_all_edges()
            .map_err(|error| to_move_error(error.into()))?
        {
            if edge.from == *entity_id {
                references.outbound.push(edge.to);
            }
            if edge.to == *entity_id {
                references.inbound.push(edge.from);
            }
        }
        Ok(references)
    }

    fn target_store_present(
        &self,
        target_store_root: &Path,
    ) -> MoveResult<bool> {
        match RuleStore::open(target_store_root) {
            Ok(_) => Ok(true),
            Err(RuleError::Storage(StorageError::WorkspaceNotFound {
                ..
            })) => Ok(false),
            Err(error) => Err(to_move_error(error)),
        }
    }

    fn entity_indexed_in(
        &self,
        store_root: &Path,
        entity_id: &Uuid,
    ) -> MoveResult<bool> {
        let store = RuleStore::open(store_root).map_err(to_move_error)?;
        let entity_root = rule_entity_root(store_root);
        Ok(store
            .entity_store()
            .get_indexed(entity_id)
            .map_err(|error| to_move_error(error.into()))?
            .map(|entity| entity.path.starts_with(&entity_root))
            .unwrap_or(false))
    }

    fn scan_store(
        &self,
        store_root: &Path,
    ) -> MoveResult<()> {
        let mut store = RuleStore::open(store_root).map_err(to_move_error)?;
        store.scan(true).map_err(to_move_error)?;
        Ok(())
    }
}

impl RuleStore {
    /// Build a read-only preflight plan for moving a rule to
    /// `target_workspace_root`, reusing the domain-neutral move kernel.
    pub fn plan_move_preflight(
        &self,
        rule_id: &Uuid,
        target_workspace_root: &Path,
    ) -> Result<MovePlan, RuleError> {
        let domain = RuleMoveDomain::new(self);
        move_kernel::plan_move(&domain, rule_id, target_workspace_root)
            .map_err(from_move_error)
    }

    /// Execute a supported rule move with a fresh journal.
    pub fn execute_move_with_journal(
        &self,
        plan: &MovePlan,
    ) -> Result<MoveOutcome, RuleError> {
        let domain = RuleMoveDomain::new(self);
        move_kernel::execute_move(&domain, plan).map_err(from_move_error)
    }

    /// Resume an interrupted rule move from its journal id.
    pub fn resume_move_with_journal(
        &self,
        journal_id: Uuid,
    ) -> Result<MoveOutcome, RuleError> {
        let domain = RuleMoveDomain::new(self);
        move_kernel::resume_move(&domain, journal_id).map_err(from_move_error)
    }

    /// Roll back a rule move from its journal id.
    pub fn rollback_move_with_journal(
        &self,
        journal_id: Uuid,
    ) -> Result<MoveOutcome, RuleError> {
        let domain = RuleMoveDomain::new(self);
        move_kernel::rollback_move(&domain, journal_id).map_err(from_move_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_kernel::{
        model::edge::EdgeRecord,
        storage::move_kernel::{
            MoveBlocker,
            MoveExecutionPhase,
            MoveReferenceDirection,
        },
    };
    use std::process::Command;
    use tempfile::tempdir;

    fn run_git(
        repo_root: &Path,
        args: &[&str],
    ) {
        let status = Command::new("git")
            .current_dir(repo_root)
            .args(args)
            .status()
            .expect("git command");
        assert!(status.success(), "git {args:?} failed: {status}");
    }

    #[test]
    fn rule_store_reuses_move_kernel_between_stores() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let mut source_store = RuleStore::init(&source_workspace).unwrap();
        let _target_store = RuleStore::init(&target_workspace).unwrap();

        let manifest = crate::manifest::RuleManifest::new(
            "sample/rule",
            "Sample rule",
            "agents",
            "main",
            "rule body",
        );
        let rule_id = source_store.create(&manifest, None).unwrap();
        source_store.scan(true).unwrap();

        let mut plan = source_store
            .plan_move_preflight(&rule_id, &target_workspace)
            .unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MoveBlocker::PathReferenceScanUnavailable { .. }
                    | MoveBlocker::DirtyTrackedFiles { .. }
            )
        });

        let outcome = source_store.execute_move_with_journal(&plan).unwrap();
        assert_eq!(outcome.journal.phase, MoveExecutionPhase::Validated);

        let src = RuleStore::open(&source_workspace).unwrap();
        let dst = RuleStore::open(&target_workspace).unwrap();
        assert!(src.entity_store().get_indexed(&rule_id).unwrap().is_none());
        assert!(dst.entity_store().get_indexed(&rule_id).unwrap().is_some());
    }

    #[test]
    fn rule_move_reports_invisible_related_rule_without_blocking() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let mut source_store = RuleStore::init(&source_workspace).unwrap();
        let _target_store = RuleStore::init(&target_workspace).unwrap();

        let moving = crate::manifest::RuleManifest::new(
            "sample/moving",
            "Moving rule",
            "agents",
            "main",
            "moving body",
        );
        let related = crate::manifest::RuleManifest::new(
            "sample/related",
            "Related rule",
            "agents",
            "main",
            "related body",
        );
        let moving_id = source_store.create(&moving, None).unwrap();
        let related_id = source_store.create(&related, None).unwrap();
        source_store
            .entity_store()
            .add_edge(EdgeRecord {
                from: moving_id,
                to: related_id,
                kind: "related".to_string(),
                created_at: chrono::Utc::now(),
            })
            .unwrap();
        source_store.scan(true).unwrap();

        let plan = source_store
            .plan_move_preflight(&moving_id, &target_workspace)
            .unwrap();

        assert!(plan.supported());
        assert!(plan.reference_visibility.iter().any(|entry| {
            entry.related_entity_id == related_id
                && entry.direction == MoveReferenceDirection::Outbound
                && !entry.visible_from_destination
        }));
        assert!(!plan.blockers.iter().any(|blocker| matches!(
            blocker,
            MoveBlocker::InvisibleReference { .. }
        )));
    }
}
