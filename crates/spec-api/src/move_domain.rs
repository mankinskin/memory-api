//! Spec-domain adapter onto the domain-neutral move kernel.
//!
//! This demonstrates that the generic cross-workspace move kernel in
//! [`memory_api::storage::move_kernel`] is reusable by a second domain: the spec
//! store implements [`MoveDomain`] and gains the same safe preflight/journaled
//! move featureset as the ticket store, without copying any move logic.
//!
//! Specs have no board or lease model, so those hooks return empty values.

use std::path::{
    Path,
    PathBuf,
};

use memory_api::{
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
    error::SpecError,
    store::SpecStore,
};

const SPEC_INDEX_DIR: &str = ".spec";

fn to_move_error(error: SpecError) -> MoveError {
    match error {
        SpecError::Storage(StorageError::Io(io)) => MoveError::Io(io),
        other => MoveError::Domain(other.to_string()),
    }
}

fn spec_entity_root(store_root: &Path) -> PathBuf {
    memory_api::workspace::resolve_store_root_from(store_root, SPEC_INDEX_DIR)
        .join("specs")
}

fn from_move_error(error: MoveError) -> SpecError {
    match error {
        MoveError::Io(io) => SpecError::Storage(StorageError::Io(io)),
        MoveError::Domain(message) =>
            SpecError::Storage(StorageError::Other(message)),
        MoveError::InteroperabilityContract {
            artifact_class,
            detail,
        } => SpecError::Storage(StorageError::Other(format!(
            "interoperability contract violation for {artifact_class}: {detail}"
        ))),
    }
}

/// Spec-domain implementation of the move kernel's [`MoveDomain`] trait.
pub struct SpecMoveDomain<'a> {
    store: &'a SpecStore,
}

impl<'a> SpecMoveDomain<'a> {
    pub fn new(store: &'a SpecStore) -> Self {
        Self { store }
    }
}

impl MoveDomain for SpecMoveDomain<'_> {
    fn entity_subdir(&self) -> &str {
        "specs"
    }

    fn store_index_dir(&self) -> &str {
        SPEC_INDEX_DIR
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
        match SpecStore::open(target_store_root) {
            Ok(_) => Ok(true),
            Err(SpecError::Storage(StorageError::WorkspaceNotFound {
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
        let store = SpecStore::open(store_root).map_err(to_move_error)?;
        let entity_root = spec_entity_root(store_root);
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
        let store = SpecStore::open(store_root).map_err(to_move_error)?;
        store
            .entity_store()
            .scan(true)
            .map_err(|error| to_move_error(error.into()))?;
        Ok(())
    }
}

impl SpecStore {
    /// Build a read-only preflight plan for moving a spec to
    /// `target_workspace_root`, reusing the domain-neutral move kernel.
    pub fn plan_move_preflight(
        &self,
        spec_id: &Uuid,
        target_workspace_root: &Path,
    ) -> Result<MovePlan, SpecError> {
        let domain = SpecMoveDomain::new(self);
        move_kernel::plan_move(&domain, spec_id, target_workspace_root)
            .map_err(from_move_error)
    }

    /// Execute a supported spec move with a fresh journal.
    pub fn execute_move_with_journal(
        &self,
        plan: &MovePlan,
    ) -> Result<MoveOutcome, SpecError> {
        let domain = SpecMoveDomain::new(self);
        move_kernel::execute_move(&domain, plan).map_err(from_move_error)
    }

    /// Resume an interrupted spec move from its journal id.
    pub fn resume_move_with_journal(
        &self,
        journal_id: Uuid,
    ) -> Result<MoveOutcome, SpecError> {
        let domain = SpecMoveDomain::new(self);
        move_kernel::resume_move(&domain, journal_id).map_err(from_move_error)
    }

    /// Roll back a spec move from its journal id.
    pub fn rollback_move_with_journal(
        &self,
        journal_id: Uuid,
    ) -> Result<MoveOutcome, SpecError> {
        let domain = SpecMoveDomain::new(self);
        move_kernel::rollback_move(&domain, journal_id).map_err(from_move_error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_api::{
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
    fn spec_store_reuses_move_kernel_between_stores() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let mut source_store = SpecStore::init(&source_workspace).unwrap();
        let _target_store = SpecStore::init(&target_workspace).unwrap();

        let spec = crate::manifest::SpecManifest::new(
            "sample/spec",
            "Sample spec",
            "spec-api",
        );
        let spec_id: Uuid = source_store.create(&spec, "body", None).unwrap();
        source_store.scan(true).unwrap();

        let mut plan = source_store
            .plan_move_preflight(&spec_id, &target_workspace)
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

        let src = SpecStore::open(&source_workspace).unwrap();
        let dst = SpecStore::open(&target_workspace).unwrap();
        assert!(src.entity_store().get_indexed(&spec_id).unwrap().is_none());
        assert!(dst.entity_store().get_indexed(&spec_id).unwrap().is_some());
    }

    /// Spec hierarchy is slug-based and code refs are repo-relative, so a move
    /// rewrites neither: after relocation the parent link still resolves and the
    /// destination slug index reindexes the spec. This documents (per ticket
    /// 94a51f30 AC2) why those reference classes need no rewrite — the kernel's
    /// `scan_store(true)` rebuild of the destination slug index is sufficient.
    #[test]
    fn moved_spec_keeps_slug_hierarchy_and_code_refs() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let mut source_store = SpecStore::init(&source_workspace).unwrap();
        let _target_store = SpecStore::init(&target_workspace).unwrap();

        let parent = crate::manifest::SpecManifest::new(
            "track/parent",
            "Parent",
            "spec-api",
        );
        let _parent_id =
            source_store.create(&parent, "parent body", None).unwrap();

        let mut child = crate::manifest::SpecManifest::new(
            "track/child",
            "Child",
            "spec-api",
        );
        child.extra.insert(
            "parent".to_string(),
            serde_json::Value::String("track/parent".into()),
        );
        child.code_refs.push(crate::code_ref::CodeRef {
            file: "crates/spec-api/src/move_domain.rs".into(),
            symbol: "SpecMoveDomain".into(),
            kind: crate::code_ref::SymbolKind::Struct,
            line_start: 1,
            line_end: 2,
            description: None,
        });
        let child_id: Uuid =
            source_store.create(&child, "child body", None).unwrap();
        source_store.scan(true).unwrap();

        let mut plan = source_store
            .plan_move_preflight(&child_id, &target_workspace)
            .unwrap();
        plan.blockers.retain(|blocker| {
            !matches!(
                blocker,
                MoveBlocker::PathReferenceScanUnavailable { .. }
                    | MoveBlocker::DirtyTrackedFiles { .. }
            )
        });
        source_store.execute_move_with_journal(&plan).unwrap();

        let dst = SpecStore::open(&target_workspace).unwrap();
        let moved = dst.get(&child_id.to_string()).unwrap();
        // Hierarchy: parent slug pointer is preserved unchanged.
        assert_eq!(moved.parent(), Some("track/parent"));
        // Code refs: repo-relative paths are preserved verbatim.
        assert_eq!(moved.code_refs.len(), 1);
        assert_eq!(
            moved.code_refs[0].file,
            "crates/spec-api/src/move_domain.rs"
        );
        // Slug index: destination resolves the moved spec's slug to its id
        // after a scan rebuilds the in-memory slug index.
        let mut dst_scanned = SpecStore::open(&target_workspace).unwrap();
        dst_scanned.scan(true).unwrap();
        assert_eq!(dst_scanned.resolve_id("track/child").unwrap(), child_id);
    }

    #[test]
    fn spec_move_reports_invisible_related_spec_without_blocking() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        run_git(&repo, &["init"]);

        let source_workspace = repo.join("source");
        let target_workspace = repo.join("target");
        std::fs::create_dir_all(&source_workspace).unwrap();
        std::fs::create_dir_all(&target_workspace).unwrap();

        let mut source_store = SpecStore::init(&source_workspace).unwrap();
        let _target_store = SpecStore::init(&target_workspace).unwrap();

        let moving = crate::manifest::SpecManifest::new(
            "track/moving",
            "Moving",
            "spec-api",
        );
        let related = crate::manifest::SpecManifest::new(
            "track/related",
            "Related",
            "spec-api",
        );
        let moving_id =
            source_store.create(&moving, "moving body", None).unwrap();
        let related_id =
            source_store.create(&related, "related body", None).unwrap();
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
