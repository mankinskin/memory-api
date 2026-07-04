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
        source_worktree_root: PathBuf,
        target_worktree_root: PathBuf,
    },
    MissingSourceEntity {
        entity_id: Uuid,
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
        entity_id: Uuid,
        working_by: String,
    },
    InvisibleReference {
        related_entity_id: Uuid,
        direction: MoveReferenceDirection,
    },
    DirtyTrackedFiles {
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
    pub source_workspace_root: PathBuf,
    pub target_workspace_root: PathBuf,
    pub source_store_root: PathBuf,
    pub target_store_root: PathBuf,
    pub source_git_worktree_root: PathBuf,
    pub target_git_worktree_root: PathBuf,
    pub git_worktree_topology: GitWorktreeTopology,
    pub source_entity_path: PathBuf,
    pub destination_entity_path: PathBuf,
    pub inbound_related_entity_ids: Vec<Uuid>,
    pub outbound_related_entity_ids: Vec<Uuid>,
    pub reference_visibility: Vec<MoveReferenceVisibility>,
    pub active_board_entries: Vec<BoardEntry>,
    pub historical_board_entries: Vec<BoardEntry>,
    pub active_leases: Vec<MoveLeaseBlock>,
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
    let target_git_root = match git_toplevel(target_workspace_root) {
        Ok(root) => root,
        Err(reason) => {
            blockers.push(MoveBlocker::PathReferenceScanUnavailable { reason });
            source_git_root.clone()
        },
    };

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

    let mut reference_visibility = Vec::new();
    if target_store_present {
        for related_entity_id in inbound.iter().chain(outbound.iter()).copied() {
            let visible_from_destination =
                domain.entity_indexed_in(&target_store_root, &related_entity_id)?;
            let direction = if outbound.contains(&related_entity_id) {
                MoveReferenceDirection::Outbound
            } else {
                MoveReferenceDirection::Inbound
            };
            reference_visibility.push(MoveReferenceVisibility {
                related_entity_id,
                direction,
                visible_from_destination,
            });
        }
    }

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

    let path_reference_files = if source_entity_path.is_some() {
        let mut files = BTreeSet::new();

        match git_tracked_path_reference_files(&source_git_root, &resolved_source_path) {
            Ok(found) => {
                for file in found {
                    files.insert(source_git_root.join(file));
                }
            },
            Err(reason) => {
                blockers.push(MoveBlocker::PathReferenceScanUnavailable { reason });
            },
        }

        if source_git_root != target_git_root {
            match git_tracked_path_reference_files(&target_git_root, &resolved_source_path) {
                Ok(found) => {
                    for file in found {
                        files.insert(target_git_root.join(file));
                    }
                },
                Err(reason) => {
                    blockers.push(MoveBlocker::PathReferenceScanUnavailable { reason });
                },
            }
        }

        files.into_iter().collect()
    } else {
        Vec::new()
    };

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
    let _span_guard = tracing::info_span!(
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
    let _span_guard = tracing::info_span!(
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
    let span = tracing::info_span!(
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

    tracing::info!(
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
    if journal.lock_paths.is_empty() {
        journal.lock_paths = collect_lock_paths(
            journal.entity_id,
            &journal.source_store_root,
            &journal.target_store_root,
        );
    }
    persist_journal(&journal_root, &journal)?;

    let span = tracing::info_span!(
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
    tracing::info!(
        target: MOVE_TRACE_TARGET,
        phase = phase_name(&journal.phase),
        "move_journal_ready"
    );

    let result: MoveResult<()> = (|| {
        if journal.phase == MoveExecutionPhase::Planned {
            let started = Instant::now();
            acquire_lock_set(&journal.lock_paths)?;
            record_phase_timing(&mut journal, "lock_acquisition_ms", started.elapsed());
            journal.phase = MoveExecutionPhase::Locked;
            span.record("phase", phase_name(&journal.phase));
            journal.updated_at = Utc::now();
            journal
                .steps
                .push("acquired source/target store locks and move entity lock".to_string());
            persist_journal(&journal_root, &journal)?;
            tracing::debug!(
                target: MOVE_TRACE_TARGET,
                phase = phase_name(&journal.phase),
                "move_phase_advanced"
            );
        }

        if journal.phase == MoveExecutionPhase::Locked {
            if let Some(parent) = journal.destination_entity_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let started = Instant::now();
            if journal.source_entity_path.exists() {
                fs::rename(&journal.source_entity_path, &journal.destination_entity_path)?;
            }
            record_phase_timing(&mut journal, "rename_entity_ms", started.elapsed());
            journal.phase = MoveExecutionPhase::Moved;
            span.record("phase", phase_name(&journal.phase));
            journal.updated_at = Utc::now();
            journal.steps.push("moved entity folder".to_string());
            persist_journal(&journal_root, &journal)?;
            tracing::debug!(
                target: MOVE_TRACE_TARGET,
                phase = phase_name(&journal.phase),
                "move_phase_advanced"
            );
        }

        if journal.phase == MoveExecutionPhase::Moved {
            if journal.rewritten_path_files.is_empty() && journal.manual_followups.is_empty() {
                let started = Instant::now();
                let (rewritten, followups) = rewrite_path_references(plan)?;
                record_phase_timing(&mut journal, "rewrite_path_refs_ms", started.elapsed());
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
                let started = Instant::now();
                journal.migrated_board_entries =
                    domain.migrate_board_history(&journal.target_store_root, &journal.entity_id)?;
                record_phase_timing(&mut journal, "migrate_board_history_ms", started.elapsed());
                if !journal.migrated_board_entries.is_empty() {
                    journal.steps.push(format!(
                        "migrated {} historical board rows",
                        journal.migrated_board_entries.len()
                    ));
                }
            }

            let started = Instant::now();
            domain.reconcile_store_touched(
                &journal.source_store_root,
                &[journal.entity_id],
            )?;
            record_phase_timing(&mut journal, "scan_source_ms", started.elapsed());
            journal.phase = MoveExecutionPhase::SourceScanned;
            span.record("phase", phase_name(&journal.phase));
            journal.updated_at = Utc::now();
            journal.steps.push("scanned source store".to_string());
            persist_journal(&journal_root, &journal)?;
            tracing::debug!(
                target: MOVE_TRACE_TARGET,
                phase = phase_name(&journal.phase),
                rewritten_path_files = journal.rewritten_path_files.len(),
                manual_followups = journal.manual_followups.len(),
                migrated_board_entries = journal.migrated_board_entries.len(),
                "move_phase_advanced"
            );
        }

        if journal.phase == MoveExecutionPhase::SourceScanned {
            let started = Instant::now();
            domain.reconcile_store_touched(
                &journal.target_store_root,
                &[journal.entity_id],
            )?;
            record_phase_timing(&mut journal, "scan_target_ms", started.elapsed());
            journal.phase = MoveExecutionPhase::TargetScanned;
            span.record("phase", phase_name(&journal.phase));
            journal.updated_at = Utc::now();
            journal.steps.push("scanned target store".to_string());
            persist_journal(&journal_root, &journal)?;
            tracing::debug!(
                target: MOVE_TRACE_TARGET,
                phase = phase_name(&journal.phase),
                "move_phase_advanced"
            );
        }

        if journal.phase == MoveExecutionPhase::TargetScanned {
            let started = Instant::now();
            let source_path_exists = journal.source_entity_path.exists();
            let destination_path_exists = journal.destination_entity_path.exists();
            let source_seen = domain.entity_indexed_in(&journal.source_store_root, &journal.entity_id)?;
            let target_seen = domain.entity_indexed_in(&journal.target_store_root, &journal.entity_id)?;
            record_phase_timing(&mut journal, "validate_move_ms", started.elapsed());
            if source_path_exists || !destination_path_exists {
                let mut problems = Vec::new();
                if source_path_exists {
                    problems.push(format!(
                        "source entity folder {} still exists after the move",
                        normalize_slashes(&journal.source_entity_path),
                    ));
                }
                if !destination_path_exists {
                    problems.push(format!(
                        "destination entity folder {} does not exist after the move",
                        normalize_slashes(&journal.destination_entity_path),
                    ));
                }
                if source_seen {
                    problems.push(format!(
                        "source store {} still indexes entity {} after the move (source entity folder {} should no longer exist)",
                        normalize_slashes(&journal.source_store_root),
                        journal.entity_id,
                        normalize_slashes(&journal.source_entity_path),
                    ));
                }
                if !target_seen {
                    problems.push(format!(
                        "destination store {} does not index entity {} after the move (expected entity folder {} — check that the destination store root resolved without a Windows verbatim `\\\\?\\` prefix)",
                        normalize_slashes(&journal.target_store_root),
                        journal.entity_id,
                        normalize_slashes(&journal.destination_entity_path),
                    ));
                }
                return Err(MoveError::Domain(format!(
                    "post-move validation failed: {}",
                    problems.join("; ")
                )));
            }

            if source_seen {
                journal.steps.push(format!(
                    "source index still resolves entity {}; source folder is absent, so ownership is correct (run scan --force later to clear stale index visibility)",
                    journal.entity_id
                ));
            }
            if !target_seen {
                journal.steps.push(format!(
                    "destination index has not resolved entity {} yet; destination folder exists and ownership is correct",
                    journal.entity_id
                ));
            }

            journal.phase = MoveExecutionPhase::Validated;
            span.record("phase", phase_name(&journal.phase));
            journal.updated_at = Utc::now();
            journal.steps.push("validated move ownership".to_string());
            journal.failure = None;
            journal.next_recovery_step = None;
            persist_journal(&journal_root, &journal)?;
            tracing::debug!(
                target: MOVE_TRACE_TARGET,
                phase = phase_name(&journal.phase),
                "move_phase_advanced"
            );
        }

        Ok(())
    })();

    match result {
        Ok(()) => {
            release_lock_set(&journal.lock_paths);
            tracing::info!(
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

/// Lock paths the kernel acquires for a move (store + entity locks on both roots).
pub fn collect_lock_paths(
    entity_id: Uuid,
    source_store_root: &Path,
    target_store_root: &Path,
) -> Vec<PathBuf> {
    let mut paths = BTreeSet::new();
    for root in [source_store_root, target_store_root] {
        paths.insert(root.join(MOVE_LOCKS_DIR).join("store.lock"));
        paths.insert(root.join(MOVE_LOCKS_DIR).join(format!("entity-{}.lock", entity_id)));
    }
    paths.into_iter().collect()
}

fn acquire_lock_set(lock_paths: &[PathBuf]) -> MoveResult<()> {
    let mut acquired = Vec::new();
    for path in lock_paths {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        match fs::OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(_) => acquired.push(path.clone()),
            Err(error) => {
                release_lock_set(&acquired);
                return Err(MoveError::Domain(format!(
                    "move lock already held at {}: {}",
                    path.display(),
                    error
                )));
            },
        }
    }
    Ok(())
}

fn release_lock_set(lock_paths: &[PathBuf]) {
    for path in lock_paths {
        let _ = fs::remove_file(path);
    }
}

fn record_phase_timing(
    journal: &mut MoveJournal,
    key: &str,
    elapsed: std::time::Duration,
) {
    let millis = elapsed.as_millis().min(u64::MAX as u128) as u64;
    journal.phase_timings_ms.insert(key.to_string(), millis);
    tracing::debug!(
        target: MOVE_TRACE_TARGET,
        journal_id = %journal.id,
        entity_id = %journal.entity_id,
        phase = phase_name(&journal.phase),
        timing_key = key,
        elapsed_ms = millis,
        "move_phase_complete"
    );
}

fn phase_name(phase: &MoveExecutionPhase) -> &'static str {
    match phase {
        MoveExecutionPhase::Planned => "planned",
        MoveExecutionPhase::Locked => "locked",
        MoveExecutionPhase::Moved => "moved",
        MoveExecutionPhase::SourceScanned => "source_scanned",
        MoveExecutionPhase::TargetScanned => "target_scanned",
        MoveExecutionPhase::Validated => "validated",
        MoveExecutionPhase::RolledBack => "rolled_back",
    }
}

fn restore_rewritten_path(rewrite: &MovePathRewrite) -> MoveResult<()> {
    if let Some(previous_content) = &rewrite.previous_content {
        fs::write(&rewrite.path, previous_content.as_bytes())?;
        return Ok(());
    }

    if !rewrite.replacements.is_empty() {
        let bytes = fs::read(&rewrite.path)?;
        let current_content = String::from_utf8(bytes).map_err(|_| {
            MoveError::Domain(format!(
                "rewritten file {} is not valid utf-8 for rollback",
                normalize_slashes(&rewrite.path)
            ))
        })?;

        let mut restored = current_content.clone();
        for replacement in rewrite.replacements.iter().rev() {
            restored = restored.replace(&replacement.after, &replacement.before);
        }
        fs::write(&rewrite.path, restored.as_bytes())?;
        return Ok(());
    }

    if path_buf_is_empty(&rewrite.repo_root) || path_buf_is_empty(&rewrite.repo_relative_path) {
        return Err(MoveError::Domain(format!(
            "journal rewrite record for {} is missing rollback metadata",
            normalize_slashes(&rewrite.path)
        )));
    }

    git_restore_tracked_path(&rewrite.repo_root, &rewrite.repo_relative_path)
}

fn recovery_hint_for_phase(phase: &MoveExecutionPhase) -> &'static str {
    match phase {
        MoveExecutionPhase::Planned | MoveExecutionPhase::Locked => {
            "run resume_move to continue execution"
        },
        MoveExecutionPhase::Moved
        | MoveExecutionPhase::SourceScanned
        | MoveExecutionPhase::TargetScanned => {
            "run rollback_move for safety, or resume_move to retry"
        },
        MoveExecutionPhase::Validated | MoveExecutionPhase::RolledBack => "no recovery action needed",
    }
}

fn rewrite_path_references(
    plan: &MovePlan,
) -> MoveResult<(Vec<MovePathRewrite>, Vec<MoveManualFollowup>)> {
    let old_abs = normalize_slashes(&plan.source_entity_path);
    let new_abs = normalize_slashes(&plan.destination_entity_path);

    let mut relative_pairs = Vec::new();
    if let (Ok(old_rel), Ok(new_rel)) = (
        plan.source_entity_path
            .strip_prefix(&plan.source_git_worktree_root),
        plan.destination_entity_path
            .strip_prefix(&plan.source_git_worktree_root),
    ) {
        relative_pairs.push((normalize_slashes(old_rel), normalize_slashes(new_rel)));
    }
    if let (Ok(old_rel), Ok(new_rel)) = (
        plan.source_entity_path
            .strip_prefix(&plan.target_git_worktree_root),
        plan.destination_entity_path
            .strip_prefix(&plan.target_git_worktree_root),
    ) {
        relative_pairs.push((normalize_slashes(old_rel), normalize_slashes(new_rel)));
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

        let bytes = fs::read(&file_path)?;
        let Ok(previous_content) = String::from_utf8(bytes) else {
            followups.push(MoveManualFollowup {
                path: file_path,
                reason: "binary or non-utf8 content requires manual rewrite".to_string(),
            });
            continue;
        };

        let mut replacements = Vec::new();
        let mut replaced = previous_content.clone();
        if replaced.contains(&old_abs) {
            replaced = replaced.replace(&old_abs, &new_abs);
            replacements.push(MoveTextReplacement {
                before: old_abs.clone(),
                after: new_abs.clone(),
            });
        }
        for (old_rel, new_rel) in &relative_pairs {
            if !old_rel.is_empty() && replaced.contains(old_rel) {
                replaced = replaced.replace(old_rel, new_rel);
                replacements.push(MoveTextReplacement {
                    before: old_rel.clone(),
                    after: new_rel.clone(),
                });
            }
        }

        if replaced == previous_content {
            followups.push(MoveManualFollowup {
                path: file_path,
                reason: "no rewrite candidate matched file content".to_string(),
            });
            continue;
        }

        let Some((repo_root, repo_relative_path)) = tracked_repo_for_file(
            &file_path,
            &plan.source_git_worktree_root,
            &plan.target_git_worktree_root,
        ) else {
            followups.push(MoveManualFollowup {
                path: file_path,
                reason: "tracked rewrite file did not resolve under source or target git root"
                    .to_string(),
            });
            continue;
        };

        fs::write(&file_path, replaced.as_bytes())?;
        rewritten.push(MovePathRewrite {
            path: file_path,
            repo_root: repo_root.to_path_buf(),
            repo_relative_path,
            replacements,
            previous_content: None,
        });
    }

    Ok((rewritten, followups))
}

fn normalize_slashes(path: &Path) -> String {
    let raw = path.to_string_lossy().replace('\\', "/");
    raw.strip_prefix("//?/").unwrap_or(&raw).to_string()
}

fn journal_path(
    store_root: &Path,
    id: Uuid,
) -> PathBuf {
    store_root.join(MOVE_JOURNALS_DIR).join(format!("{}.json", id))
}

/// Persist a move journal under the store root's `move-journals/` directory.
pub fn persist_journal(
    store_root: &Path,
    journal: &MoveJournal,
) -> MoveResult<()> {
    let path = journal_path(store_root, journal.id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload =
        serde_json::to_vec_pretty(journal).map_err(|error| MoveError::Domain(error.to_string()))?;
    fs::write(path, payload).map_err(MoveError::Io)
}

/// Load a move journal from the store root's `move-journals/` directory.
pub fn load_journal(
    store_root: &Path,
    id: Uuid,
) -> MoveResult<MoveJournal> {
    let payload = fs::read(journal_path(store_root, id)).map_err(MoveError::Io)?;
    serde_json::from_slice(&payload).map_err(|error| MoveError::Domain(error.to_string()))
}

fn classify_git_worktree_topology(
    source_git_root: &Path,
    target_git_root: &Path,
) -> GitWorktreeTopology {
    if source_git_root == target_git_root {
        return GitWorktreeTopology::Same;
    }
    if target_git_root.starts_with(source_git_root) {
        return GitWorktreeTopology::ParentToSubmodule;
    }
    if source_git_root.starts_with(target_git_root) {
        return GitWorktreeTopology::SubmoduleToParent;
    }
    GitWorktreeTopology::Unrelated
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
    entity_path: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut candidates = BTreeSet::new();
    candidates.insert(entity_path.to_string_lossy().replace('\\', "/"));
    if let Ok(relative) = entity_path.strip_prefix(repo_root) {
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

fn tracked_repo_for_file<'a>(
    file: &Path,
    source_repo_root: &'a Path,
    target_repo_root: &'a Path,
) -> Option<(&'a Path, PathBuf)> {
    let mut candidates = Vec::new();
    if let Ok(relative) = file.strip_prefix(source_repo_root) {
        candidates.push((source_repo_root, relative.to_path_buf()));
    }
    if let Ok(relative) = file.strip_prefix(target_repo_root) {
        candidates.push((target_repo_root, relative.to_path_buf()));
    }

    candidates.sort_by_key(|(_, relative)| std::cmp::Reverse(relative.components().count()));
    candidates.into_iter().next()
}

fn git_restore_tracked_path(
    repo_root: &Path,
    repo_relative_path: &Path,
) -> MoveResult<()> {
    let relative = repo_relative_path.to_string_lossy().replace('\\', "/");
    let output = Command::new("git")
        .args([
            "-C",
            &repo_root.to_string_lossy(),
            "restore",
            "--worktree",
            "--source=HEAD",
            "--",
            &relative,
        ])
        .output()
        .map_err(|error| MoveError::Domain(error.to_string()))?;

    if !output.status.success() {
        return Err(MoveError::Domain(format!(
            "git restore failed for {} in {}: {}",
            relative,
            normalize_slashes(repo_root),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    Ok(())
}
