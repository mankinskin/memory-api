use super::*;

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
    let normalized: Vec<String> =
        paths.iter().map(|path| normalize_slashes(path)).collect();
    normalized.serialize(serializer)
}

fn deserialize_pathbuf_vec<'de, D>(
    deserializer: D
) -> Result<Vec<PathBuf>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Vec::<String>::deserialize(deserializer)?
        .into_iter()
        .map(PathBuf::from)
        .collect())
}

pub(super) fn path_buf_is_empty(path: &PathBuf) -> bool {
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
    #[serde(
        default,
        skip_serializing_if = "std::collections::BTreeMap::is_empty"
    )]
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
