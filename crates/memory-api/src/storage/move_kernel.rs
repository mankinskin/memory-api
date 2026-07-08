//! Domain-neutral cross-workspace move kernel.
//!
//! This module owns the *generic* machinery for moving a filesystem-backed
//! entity (a folder identified by a [`Uuid`]) between two stores that may live
//! in the same git worktree or in a parent/submodule relationship:
//!
//! - read-only preflight planning ([`plan_move`]),
//! - journaled / resumable / rollbackable execution
//!   ([`execute_move`], [`resume_move`], [`rollback_move`]),
//! - git worktree topology classification, tracked-path-reference scanning and
//!   rewriting, dirty-file detection, lock-set management, and journal
//!   persistence.
//!
//! All domain-specific behavior (entity path resolution, edge/reference
//! enumeration, destination visibility, board/lease detection and historical
//! migration, store rescans) is injected through the [`MoveDomain`] trait, so
//! every domain store (ticket, spec, rule, audit, session, feedback) can opt
//! into the same safe move featureset without copying logic.
//!
//! The kernel contains **no** ticket-specific types: identities are bare
//! [`Uuid`]s and board rows are the shared [`BoardEntry`].

use std::{
    collections::BTreeSet,
    fs,
    path::{
        Path,
        PathBuf,
    },
    process::Command,
    time::Instant,
};

use chrono::Utc;
use serde::{
    Deserialize,
    Serialize,
};
use tracing::field::Empty;
use uuid::Uuid;

use crate::storage::board::BoardEntry;

const MOVE_LOCKS_DIR: &str = "move-locks";
const MOVE_JOURNALS_DIR: &str = "move-journals";
const MOVE_TRACE_TARGET: &str = "memory_api::storage::move_kernel";


#[path = "move_kernel/internal.rs"]
mod internal;
use internal::*;
pub use internal::{collect_lock_paths, load_journal, persist_journal};
/// Error type for the move kernel.
///
/// Domain trait implementations map their own storage errors onto
/// [`MoveError::Domain`]; the kernel raises [`MoveError::Io`] for filesystem
/// failures it performs directly (rename, lock files, journal persistence).
#[derive(Debug)]
pub enum MoveError {
    Io(std::io::Error),
    Domain(String),
}

impl std::fmt::Display for MoveError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            MoveError::Io(error) => write!(f, "{error}"),
            MoveError::Domain(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for MoveError {}

impl From<std::io::Error> for MoveError {
    fn from(error: std::io::Error) -> Self {
        MoveError::Io(error)
    }
}

/// Convenience result alias for kernel operations.
pub type MoveResult<T> = Result<T, MoveError>;

/// Direction of a cross-entity reference relative to the entity being moved.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum MoveReferenceDirection {
    Inbound,
    Outbound,
}

/// Relationship between the source and target git worktrees.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum GitWorktreeTopology {
    /// Both stores resolve to the same git worktree root.
    Same,
    /// Target worktree is nested inside the source worktree (submodule).
    ParentToSubmodule,
    /// Source worktree is nested inside the target worktree (submodule).
    SubmoduleToParent,
    /// The two worktrees are unrelated repositories.
    Unrelated,
}

/// Visibility of a related entity from the destination store.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveReferenceVisibility {
    pub related_entity_id: Uuid,
    pub direction: MoveReferenceDirection,
    pub visible_from_destination: bool,
}

/// A lease that blocks the move because the entity is actively leased.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveLeaseBlock {
    pub entity_id: Uuid,
    pub working_by: String,
}

/// A domain-neutral reason a move cannot proceed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MoveBlocker {
    DifferentGitWorktree {
        #[serde(
            serialize_with = "serialize_normalized_path",
            deserialize_with = "deserialize_pathbuf"
        )]
        source_worktree_root: PathBuf,
        #[serde(
            serialize_with = "serialize_normalized_path",
            deserialize_with = "deserialize_pathbuf"
        )]
        target_worktree_root: PathBuf,
    },
    MissingSourceEntity {
        entity_id: Uuid,
    },
    MissingTargetStore {
        #[serde(
            serialize_with = "serialize_normalized_path",
            deserialize_with = "deserialize_pathbuf"
        )]
        target_store_root: PathBuf,
    },
    ActiveOrStaleBoardEntry {
        entry_id: Uuid,
        status: String,
        agent_id: String,
    },
    ActiveLease {
        entity_id: Uuid,
        working_by: String,
    },
    InvisibleReference {
        related_entity_id: Uuid,
        direction: MoveReferenceDirection,
    },
    DirtyTrackedFiles {
        #[serde(
            serialize_with = "serialize_normalized_path_vec",
            deserialize_with = "deserialize_pathbuf_vec"
        )]
        files: Vec<PathBuf>,
    },
    PathReferenceScanUnavailable {
        reason: String,
    },
}

/// Inbound / outbound related entity ids enumerated by the domain.
#[derive(Debug, Clone, Default)]
pub struct MoveReferences {
    pub inbound: Vec<Uuid>,
    pub outbound: Vec<Uuid>,
}

/// Board rows associated with the entity, split into active/stale vs historical.
///
/// Domains without a board return an empty value via [`MoveBoardState::default`].
#[derive(Debug, Clone, Default)]
pub struct MoveBoardState {
    /// Entries that are currently active or stale (these block the move).
    pub active_entries: Vec<BoardEntry>,
    /// Completed / historical entries (migrated alongside the entity).
    pub historical_entries: Vec<BoardEntry>,
}

/// Domain-specific hooks the kernel needs to plan and execute a move.
///
/// Implementors are thin adapters over a concrete domain store. Every method is
/// expressed in domain-neutral terms ([`Uuid`] identities, [`BoardEntry`] rows,
/// store-root [`Path`]s) so the kernel never sees ticket-specific types.
pub trait MoveDomain {
    /// Subdirectory under a store root that holds entity folders, e.g.
    /// `"tickets"` or `"specs"`.
    fn entity_subdir(&self) -> &str;

    /// Store index directory name used for workspace<->store-root resolution,
    /// e.g. `".ticket"` or `".spec"`.
    fn store_index_dir(&self) -> &str;

    /// The source store's index/store root.
    fn source_store_root(&self) -> PathBuf;

    /// On-disk path of the entity in the source store, or `None` if the source
    /// store does not currently index it.
    fn source_entity_path(
        &self,
        entity_id: &Uuid,
    ) -> MoveResult<Option<PathBuf>>;

    /// Inbound and outbound related entity ids (graph edges). Domains without an
    /// edge model return [`MoveReferences::default`].
    fn related_entities(
        &self,
        entity_id: &Uuid,
    ) -> MoveResult<MoveReferences>;

    /// Whether the destination store exists at `target_store_root`.
    fn target_store_present(
        &self,
        target_store_root: &Path,
    ) -> MoveResult<bool>;

    /// Whether `entity_id` is indexed by the store rooted at `store_root`.
    fn entity_indexed_in(
        &self,
        store_root: &Path,
        entity_id: &Uuid,
    ) -> MoveResult<bool>;

    /// Board rows for the entity. Domains without a board return
    /// [`MoveBoardState::default`] (the default implementation).
    fn board_state(
        &self,
        _entity_id: &Uuid,
    ) -> MoveResult<MoveBoardState> {
        Ok(MoveBoardState::default())
    }

    /// Active leases for the entity. Domains without leases return an empty vec
    /// (the default implementation).
    fn active_leases(
        &self,
        _entity_id: &Uuid,
    ) -> MoveResult<Vec<MoveLeaseBlock>> {
        Ok(Vec::new())
    }

    /// Migrate historical board rows from the source store to the target store,
    /// returning the migrated rows. Domains without a board return an empty vec
    /// (the default implementation).
    ///
    /// Implementations must fail if any active/stale row is encountered.
    fn migrate_board_history(
        &self,
        _target_store_root: &Path,
        _entity_id: &Uuid,
    ) -> MoveResult<Vec<BoardEntry>> {
        Ok(Vec::new())
    }

    /// Restore previously migrated board rows back to the source store (rollback).
    fn restore_board_history(
        &self,
        _target_store_root: &Path,
        _entries: &[BoardEntry],
    ) -> MoveResult<()> {
        Ok(())
    }

    /// Force a full rescan of the store rooted at `store_root`.
    fn scan_store(
        &self,
        store_root: &Path,
    ) -> MoveResult<()>;

    /// Reconcile only a known touched subset when the caller already knows the
    /// affected ids (for example move execution for a single entity). Domains
    /// that do not support targeted reconciliation can fall back to `scan_store`.
    fn reconcile_store_touched(
        &self,
        store_root: &Path,
        touched_entity_ids: &[Uuid],
    ) -> MoveResult<()> {
        let _ = touched_entity_ids;
        self.scan_store(store_root)
    }
}

/// Read-only preflight plan for a move.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovePlan {
    pub entity_id: Uuid,
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub source_workspace_root: PathBuf,
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub target_workspace_root: PathBuf,
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub source_store_root: PathBuf,
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub target_store_root: PathBuf,
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub source_git_worktree_root: PathBuf,
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub target_git_worktree_root: PathBuf,
    pub git_worktree_topology: GitWorktreeTopology,
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub source_entity_path: PathBuf,
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub destination_entity_path: PathBuf,
    pub inbound_related_entity_ids: Vec<Uuid>,
    pub outbound_related_entity_ids: Vec<Uuid>,
    pub reference_visibility: Vec<MoveReferenceVisibility>,
    pub active_board_entries: Vec<BoardEntry>,
    pub historical_board_entries: Vec<BoardEntry>,
    pub active_leases: Vec<MoveLeaseBlock>,
    #[serde(
        serialize_with = "serialize_normalized_path_vec",
        deserialize_with = "deserialize_pathbuf_vec"
    )]
    pub path_reference_files: Vec<PathBuf>,
    pub blockers: Vec<MoveBlocker>,
    pub captured_at: chrono::DateTime<Utc>,
}

impl MovePlan {
    /// The move is supported only when there are no blockers.
    pub fn supported(&self) -> bool {
        self.blockers.is_empty()
    }
}

fn serialize_normalized_path<S>(
    path: &PathBuf,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&normalize_slashes(path))
}

fn deserialize_pathbuf<'de, D>(deserializer: D) -> Result<PathBuf, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(PathBuf::from(String::deserialize(deserializer)?))
}

fn serialize_normalized_path_vec<S>(
    paths: &[PathBuf],
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let normalized: Vec<String> = paths.iter().map(|path| normalize_slashes(path)).collect();
    normalized.serialize(serializer)
}

fn deserialize_pathbuf_vec<'de, D>(deserializer: D) -> Result<Vec<PathBuf>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Vec::<String>::deserialize(deserializer)?
        .into_iter()
        .map(PathBuf::from)
        .collect())
}

fn path_buf_is_empty(path: &PathBuf) -> bool {
    path.as_os_str().is_empty()
}

/// A tracked text file rewritten during move execution.
///
/// Rollback prefers git-backed restore metadata and falls back to the legacy
/// inline snapshot form when resuming older journals.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MovePathRewrite {
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub path: PathBuf,
    #[serde(
        default,
        skip_serializing_if = "path_buf_is_empty",
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub repo_root: PathBuf,
    #[serde(
        default,
        skip_serializing_if = "path_buf_is_empty",
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub repo_relative_path: PathBuf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub replacements: Vec<MoveTextReplacement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous_content: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveTextReplacement {
    pub before: String,
    pub after: String,
}

/// A tracked reference that requires manual follow-up (binary content, no match).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveManualFollowup {
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub path: PathBuf,
    pub reason: String,
}

/// Phases of a journaled move, persisted for resume/rollback.
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

/// Durable journal of a move, written after every phase transition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveJournal {
    pub id: Uuid,
    /// Identity of the moved entity. Accepts the legacy `ticket_id` key for
    /// journals written before the kernel generalization.
    #[serde(alias = "ticket_id")]
    pub entity_id: Uuid,
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub source_store_root: PathBuf,
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub target_store_root: PathBuf,
    /// Source on-disk entity path. Accepts the legacy `source_ticket_path` key.
    #[serde(alias = "source_ticket_path")]
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub source_entity_path: PathBuf,
    /// Destination on-disk entity path. Accepts the legacy
    /// `destination_ticket_path` key.
    #[serde(alias = "destination_ticket_path")]
    #[serde(
        serialize_with = "serialize_normalized_path",
        deserialize_with = "deserialize_pathbuf"
    )]
    pub destination_entity_path: PathBuf,
    pub phase: MoveExecutionPhase,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
    pub steps: Vec<String>,
    pub rollback_steps: Vec<String>,
    #[serde(
        default,
        serialize_with = "serialize_normalized_path_vec",
        deserialize_with = "deserialize_pathbuf_vec"
    )]
    pub lock_paths: Vec<PathBuf>,
    #[serde(default)]
    pub migrated_board_entries: Vec<BoardEntry>,
    #[serde(default)]
    pub rewritten_path_files: Vec<MovePathRewrite>,
    #[serde(default)]
    pub manual_followups: Vec<MoveManualFollowup>,
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub phase_timings_ms: std::collections::BTreeMap<String, u64>,
    pub failure: Option<String>,
    #[serde(default)]
    pub next_recovery_step: Option<String>,
}

/// Result of a journaled move execution / resume / rollback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoveOutcome {
    pub journal: MoveJournal,
    pub resumed: bool,
    pub rolled_back: bool,
}

/// Build a read-only preflight plan for moving `entity_id` to
/// `target_workspace_root`.
pub fn plan_move<D: MoveDomain + ?Sized>(
    domain: &D,
    entity_id: &Uuid,
    target_workspace_root: &Path,
) -> MoveResult<MovePlan> {
    let index_dir = domain.store_index_dir().to_string();
    let source_store_root = domain.source_store_root();
    let source_workspace_root =
        crate::workspace::resolve_workspace_root_from_store_root(&source_store_root, &index_dir);
    let target_store_root = crate::workspace::resolve_store_root_from(target_workspace_root, &index_dir);

    let subdir = domain.entity_subdir().to_string();
    let source_entity_path = domain.source_entity_path(entity_id)?;
    let resolved_source_path = source_entity_path
        .clone()
        .unwrap_or_else(|| source_store_root.join(&subdir).join(entity_id.to_string()));
    let destination_entity_path = target_store_root.join(&subdir).join(entity_id.to_string());

    let mut blockers = Vec::new();

    let source_git_root = git_toplevel(&source_workspace_root).map_err(MoveError::Domain)?;
    let target_git_root = resolve_target_git_root_or_block(
        target_workspace_root,
        &source_git_root,
        &mut blockers,
    );

    let git_worktree_topology = classify_git_worktree_topology(&source_git_root, &target_git_root);
    if git_worktree_topology == GitWorktreeTopology::Unrelated {
        blockers.push(MoveBlocker::DifferentGitWorktree {
            source_worktree_root: source_git_root.clone(),
            target_worktree_root: target_git_root.clone(),
        });
    }

    let target_store_present = domain.target_store_present(&target_store_root)?;
    if !target_store_present {
        blockers.push(MoveBlocker::MissingTargetStore {
            target_store_root: target_store_root.clone(),
        });
    }

    if source_entity_path.is_none() {
        blockers.push(MoveBlocker::MissingSourceEntity {
            entity_id: *entity_id,
        });
    }

    let references = domain.related_entities(entity_id)?;
    let inbound: BTreeSet<Uuid> = references.inbound.into_iter().collect();
    let outbound: BTreeSet<Uuid> = references.outbound.into_iter().collect();

    let reference_visibility = build_reference_visibility(
        domain,
        &target_store_root,
        target_store_present,
        &inbound,
        &outbound,
    )?;

    let board_state = domain.board_state(entity_id)?;
    for entry in &board_state.active_entries {
        blockers.push(MoveBlocker::ActiveOrStaleBoardEntry {
            entry_id: entry.entry_id,
            status: format!("{:?}", entry.status),
            agent_id: entry.agent_id.clone(),
        });
    }

    let active_leases = domain.active_leases(entity_id)?;
    for lease in &active_leases {
        blockers.push(MoveBlocker::ActiveLease {
            entity_id: lease.entity_id,
            working_by: lease.working_by.clone(),
        });
    }

    let path_reference_files = collect_plan_path_reference_files(
        source_entity_path.is_some(),
        &source_git_root,
        &target_git_root,
        &resolved_source_path,
        &source_store_root,
        &target_store_root,
        &subdir,
        &mut blockers,
    );

    Ok(MovePlan {
        entity_id: *entity_id,
        source_workspace_root,
        target_workspace_root: target_workspace_root.to_path_buf(),
        source_store_root,
        target_store_root,
        source_git_worktree_root: source_git_root,
        target_git_worktree_root: target_git_root,
        git_worktree_topology,
        source_entity_path: resolved_source_path,
        destination_entity_path,
        inbound_related_entity_ids: inbound.into_iter().collect(),
        outbound_related_entity_ids: outbound.into_iter().collect(),
        reference_visibility,
        active_board_entries: board_state.active_entries,
        historical_board_entries: board_state.historical_entries,
        active_leases,
        path_reference_files,
        blockers,
        captured_at: Utc::now(),
    })
}

/// Execute a supported move with a fresh journal.
pub fn execute_move<D: MoveDomain + ?Sized>(
    domain: &D,
    plan: &MovePlan,
) -> MoveResult<MoveOutcome> {
    let _span_guard = tracing::debug_span!(
        target: MOVE_TRACE_TARGET,
        "move_execute",
        entity_id = %plan.entity_id,
        supported = plan.supported(),
    )
    .entered();
    if !plan.supported() {
        return Err(MoveError::Domain(
            "move preflight contains blockers".to_string(),
        ));
    }
    execute_or_resume(domain, plan, None, false)
}

/// Resume an interrupted move identified by its journal id.
pub fn resume_move<D: MoveDomain + ?Sized>(
    domain: &D,
    journal_id: Uuid,
) -> MoveResult<MoveOutcome> {
    let _span_guard = tracing::debug_span!(
        target: MOVE_TRACE_TARGET,
        "move_resume",
        journal_id = %journal_id,
    )
    .entered();
    let journal = load_journal(&domain.source_store_root(), journal_id)?;
    tracing::debug!(
        target: MOVE_TRACE_TARGET,
        entity_id = %journal.entity_id,
        phase = phase_name(&journal.phase),
        "move_resume_journal_loaded"
    );
    let target_workspace_root = crate::workspace::resolve_workspace_root_from_store_root(
        &journal.target_store_root,
        domain.store_index_dir(),
    );
    let plan = plan_move(domain, &journal.entity_id, &target_workspace_root)?;
    execute_or_resume(domain, &plan, Some(journal), true)
}

/// Roll back a move identified by its journal id.
pub fn rollback_move<D: MoveDomain + ?Sized>(
    domain: &D,
    journal_id: Uuid,
) -> MoveResult<MoveOutcome> {
    let source_store_root = domain.source_store_root();
    let mut journal = load_journal(&source_store_root, journal_id)?;
    normalize_journal_entity_paths(domain, &mut journal);
    let span = tracing::debug_span!(
        target: MOVE_TRACE_TARGET,
        "move_rollback",
        journal_id = %journal.id,
        entity_id = %journal.entity_id,
        resumed = false,
        rolled_back = true,
        phase = Empty,
    );
    let _span_guard = span.enter();
    span.record("phase", phase_name(&journal.phase));
    if journal.lock_paths.is_empty() {
        journal.lock_paths = collect_lock_paths(
            journal.entity_id,
            &journal.source_store_root,
            &journal.target_store_root,
        );
    }
    acquire_lock_set(&journal.lock_paths)?;

    if journal.destination_entity_path.exists() && !journal.source_entity_path.exists() {
        if let Some(parent) = journal.source_entity_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&journal.destination_entity_path, &journal.source_entity_path)?;
    }

    for rewrite in &journal.rewritten_path_files {
        restore_rewritten_path(rewrite)?;
    }

    if !journal.migrated_board_entries.is_empty() {
        let started = Instant::now();
        domain.restore_board_history(&journal.target_store_root, &journal.migrated_board_entries)?;
        record_phase_timing(&mut journal, "rollback_restore_board_history_ms", started.elapsed());
    }

    let source_scan_started = Instant::now();
    domain.reconcile_store_touched(
        &journal.source_store_root,
        &[journal.entity_id],
    )?;
    record_phase_timing(&mut journal, "rollback_scan_source_ms", source_scan_started.elapsed());

    let target_scan_started = Instant::now();
    domain.reconcile_store_touched(
        &journal.target_store_root,
        &[journal.entity_id],
    )?;
    record_phase_timing(&mut journal, "rollback_scan_target_ms", target_scan_started.elapsed());

    journal.phase = MoveExecutionPhase::RolledBack;
    journal.updated_at = Utc::now();
    journal
        .steps
        .push("rolled back entity folder to source store".to_string());
    journal.failure = None;
    journal.next_recovery_step = None;
    persist_journal(&source_store_root, &journal)?;
    release_lock_set(&journal.lock_paths);

    tracing::debug!(
        target: MOVE_TRACE_TARGET,
        journal_id = %journal.id,
        entity_id = %journal.entity_id,
        phase = phase_name(&journal.phase),
        migrated_board_entries = journal.migrated_board_entries.len(),
        rewritten_path_files = journal.rewritten_path_files.len(),
        manual_followups = journal.manual_followups.len(),
        "move_rollback_complete"
    );

    Ok(MoveOutcome {
        journal,
        resumed: false,
        rolled_back: true,
    })
}

fn execute_or_resume<D: MoveDomain + ?Sized>(
    domain: &D,
    plan: &MovePlan,
    existing: Option<MoveJournal>,
    resumed: bool,
) -> MoveResult<MoveOutcome> {
    let journal_root = domain.source_store_root();
    let mut journal = existing.unwrap_or_else(|| MoveJournal {
        id: Uuid::new_v4(),
        entity_id: plan.entity_id,
        source_store_root: plan.source_store_root.clone(),
        target_store_root: plan.target_store_root.clone(),
        source_entity_path: plan.source_entity_path.clone(),
        destination_entity_path: plan.destination_entity_path.clone(),
        phase: MoveExecutionPhase::Planned,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        steps: vec!["created move journal".to_string()],
        rollback_steps: vec![
            "rename destination entity folder back to source path".to_string(),
            "restore migrated board history rows to source store".to_string(),
            "scan source and target stores".to_string(),
        ],
        lock_paths: collect_lock_paths(
            plan.entity_id,
            &plan.source_store_root,
            &plan.target_store_root,
        ),
        migrated_board_entries: Vec::new(),
        rewritten_path_files: Vec::new(),
        manual_followups: Vec::new(),
        phase_timings_ms: std::collections::BTreeMap::new(),
        failure: None,
        next_recovery_step: None,
    });
    normalize_journal_entity_paths(domain, &mut journal);
    if journal.lock_paths.is_empty() {
        journal.lock_paths = collect_lock_paths(
            journal.entity_id,
            &journal.source_store_root,
            &journal.target_store_root,
        );
    }
    persist_journal(&journal_root, &journal)?;

    let span = tracing::debug_span!(
        target: MOVE_TRACE_TARGET,
        "move_execute_or_resume",
        journal_id = %journal.id,
        entity_id = %journal.entity_id,
        resumed,
        rolled_back = false,
        phase = Empty,
    );
    let _span_guard = span.enter();
    span.record("phase", phase_name(&journal.phase));
    tracing::debug!(
        target: MOVE_TRACE_TARGET,
        phase = phase_name(&journal.phase),
        "move_journal_ready"
    );

    let result: MoveResult<()> = (|| {
        advance_phase_planned(&mut journal, &journal_root, &span)?;
        advance_phase_locked(&mut journal, &journal_root, &span)?;
        advance_phase_moved(domain, plan, &mut journal, &journal_root, &span)?;
        advance_phase_source_scanned(domain, &mut journal, &journal_root, &span)?;
        advance_phase_target_scanned(domain, &mut journal, &journal_root, &span)?;

        Ok(())
    })();

    match result {
        Ok(()) => {
            release_lock_set(&journal.lock_paths);
            tracing::debug!(
                target: MOVE_TRACE_TARGET,
                journal_id = %journal.id,
                entity_id = %journal.entity_id,
                phase = phase_name(&journal.phase),
                resumed,
                rewritten_path_files = journal.rewritten_path_files.len(),
                manual_followups = journal.manual_followups.len(),
                migrated_board_entries = journal.migrated_board_entries.len(),
                "move_execute_or_resume_complete"
            );
            Ok(MoveOutcome {
                journal,
                resumed,
                rolled_back: false,
            })
        },
        Err(error) => {
            journal.updated_at = Utc::now();
            journal.failure = Some(error.to_string());
            journal.next_recovery_step = Some(recovery_hint_for_phase(&journal.phase).to_string());
            let _ = persist_journal(&journal_root, &journal);
            release_lock_set(&journal.lock_paths);
            tracing::error!(
                target: MOVE_TRACE_TARGET,
                journal_id = %journal.id,
                entity_id = %journal.entity_id,
                phase = phase_name(&journal.phase),
                resumed,
                error = %error,
                "move_execute_or_resume_failed"
            );
            Err(error)
        },
    }
}

