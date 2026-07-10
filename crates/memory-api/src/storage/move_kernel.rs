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
pub use internal::{
    collect_lock_paths,
    load_journal,
    persist_journal,
};
/// Error type for the move kernel.
///
/// Domain trait implementations map their own storage errors onto
/// [`MoveError::Domain`]; the kernel raises [`MoveError::Io`] for filesystem
/// failures it performs directly (rename, lock files, journal persistence).

#[path = "move_kernel_types.rs"]
mod move_kernel_types;
use move_kernel_types::path_buf_is_empty;
pub use move_kernel_types::*;

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
        crate::workspace::resolve_workspace_root_from_store_root(
            &source_store_root,
            &index_dir,
        );
    let target_store_root = crate::workspace::resolve_store_root_from(
        target_workspace_root,
        &index_dir,
    );

    let subdir = domain.entity_subdir().to_string();
    let source_entity_path = domain.source_entity_path(entity_id)?;
    let resolved_source_path =
        source_entity_path.clone().unwrap_or_else(|| {
            source_store_root.join(&subdir).join(entity_id.to_string())
        });
    let destination_entity_path =
        target_store_root.join(&subdir).join(entity_id.to_string());

    let mut blockers = Vec::new();

    let source_git_root =
        git_toplevel(&source_workspace_root).map_err(MoveError::Domain)?;
    let target_git_root = resolve_target_git_root_or_block(
        target_workspace_root,
        &source_git_root,
        &mut blockers,
    );

    let git_worktree_topology =
        classify_git_worktree_topology(&source_git_root, &target_git_root);
    if git_worktree_topology == GitWorktreeTopology::Unrelated {
        blockers.push(MoveBlocker::DifferentGitWorktree {
            source_worktree_root: source_git_root.clone(),
            target_worktree_root: target_git_root.clone(),
        });
    }

    let target_store_present =
        domain.target_store_present(&target_store_root)?;
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
    let target_workspace_root =
        crate::workspace::resolve_workspace_root_from_store_root(
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

    if journal.destination_entity_path.exists()
        && !journal.source_entity_path.exists()
    {
        if let Some(parent) = journal.source_entity_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(
            &journal.destination_entity_path,
            &journal.source_entity_path,
        )?;
    }

    for rewrite in &journal.rewritten_path_files {
        restore_rewritten_path(rewrite)?;
    }

    if !journal.migrated_board_entries.is_empty() {
        let started = Instant::now();
        domain.restore_board_history(
            &journal.target_store_root,
            &journal.migrated_board_entries,
        )?;
        record_phase_timing(
            &mut journal,
            "rollback_restore_board_history_ms",
            started.elapsed(),
        );
    }

    let source_scan_started = Instant::now();
    domain.reconcile_store_touched(
        &journal.source_store_root,
        &[journal.entity_id],
    )?;
    record_phase_timing(
        &mut journal,
        "rollback_scan_source_ms",
        source_scan_started.elapsed(),
    );

    let target_scan_started = Instant::now();
    domain.reconcile_store_touched(
        &journal.target_store_root,
        &[journal.entity_id],
    )?;
    record_phase_timing(
        &mut journal,
        "rollback_scan_target_ms",
        target_scan_started.elapsed(),
    );

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
        advance_phase_source_scanned(
            domain,
            &mut journal,
            &journal_root,
            &span,
        )?;
        advance_phase_target_scanned(
            domain,
            &mut journal,
            &journal_root,
            &span,
        )?;

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
            journal.next_recovery_step =
                Some(recovery_hint_for_phase(&journal.phase).to_string());
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
