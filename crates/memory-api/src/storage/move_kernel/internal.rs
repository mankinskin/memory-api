use super::*;

pub(super) fn resolve_target_git_root_or_block(
    target_workspace_root: &Path,
    source_git_root: &Path,
    blockers: &mut Vec<MoveBlocker>,
) -> PathBuf {
    match git_toplevel(target_workspace_root) {
        Ok(root) => root,
        Err(reason) => {
            blockers.push(MoveBlocker::PathReferenceScanUnavailable { reason });
            source_git_root.to_path_buf()
        },
    }
}

pub(super) fn build_reference_visibility<D: MoveDomain + ?Sized>(
    domain: &D,
    target_store_root: &Path,
    target_store_present: bool,
    inbound: &BTreeSet<Uuid>,
    outbound: &BTreeSet<Uuid>,
) -> MoveResult<Vec<MoveReferenceVisibility>> {
    if !target_store_present {
        return Ok(Vec::new());
    }

    let mut visibility = Vec::new();
    for related_entity_id in inbound.iter().chain(outbound.iter()).copied() {
        let visible_from_destination =
            domain.entity_indexed_in(target_store_root, &related_entity_id)?;
        let direction = if outbound.contains(&related_entity_id) {
            MoveReferenceDirection::Outbound
        } else {
            MoveReferenceDirection::Inbound
        };
        visibility.push(MoveReferenceVisibility {
            related_entity_id,
            direction,
            visible_from_destination,
        });
    }

    Ok(visibility)
}

pub(super) fn collect_plan_path_reference_files(
    source_entity_exists: bool,
    source_git_root: &Path,
    target_git_root: &Path,
    resolved_source_path: &Path,
    source_store_root: &Path,
    target_store_root: &Path,
    subdir: &str,
    blockers: &mut Vec<MoveBlocker>,
) -> Vec<PathBuf> {
    if !source_entity_exists {
        return Vec::new();
    }

    let mut files = BTreeSet::new();
    collect_candidate_reference_files(
        source_git_root,
        resolved_source_path,
        source_store_root,
        target_store_root,
        subdir,
        blockers,
        &mut files,
    );

    if source_git_root != target_git_root {
        collect_candidate_reference_files(
            target_git_root,
            resolved_source_path,
            source_store_root,
            target_store_root,
            subdir,
            blockers,
            &mut files,
        );
    }

    files.into_iter().collect()
}

pub(super) fn collect_candidate_reference_files(
    git_root: &Path,
    resolved_source_path: &Path,
    source_store_root: &Path,
    target_store_root: &Path,
    subdir: &str,
    blockers: &mut Vec<MoveBlocker>,
    files: &mut BTreeSet<PathBuf>,
) {
    match git_tracked_path_reference_files(git_root, resolved_source_path) {
        Ok(found) =>
            for file in found {
                let candidate = git_root.join(file);
                if is_persistent_move_reference_file(
                    &candidate,
                    source_store_root,
                    target_store_root,
                    subdir,
                ) {
                    files.insert(candidate);
                }
            },
        Err(reason) =>
            blockers.push(MoveBlocker::PathReferenceScanUnavailable { reason }),
    }
}

pub(super) fn advance_phase_planned(
    journal: &mut MoveJournal,
    journal_root: &Path,
    span: &tracing::Span,
) -> MoveResult<()> {
    if journal.phase != MoveExecutionPhase::Planned {
        return Ok(());
    }

    let started = Instant::now();
    acquire_lock_set(&journal.lock_paths)?;
    record_phase_timing(journal, "lock_acquisition_ms", started.elapsed());
    journal.phase = MoveExecutionPhase::Locked;
    span.record("phase", phase_name(&journal.phase));
    journal.updated_at = Utc::now();
    journal.steps.push(
        "acquired source/target store locks and move entity lock".to_string(),
    );
    persist_journal(journal_root, journal)?;
    tracing::debug!(
        target: MOVE_TRACE_TARGET,
        phase = phase_name(&journal.phase),
        "move_phase_advanced"
    );
    Ok(())
}

pub(super) fn advance_phase_locked(
    journal: &mut MoveJournal,
    journal_root: &Path,
    span: &tracing::Span,
) -> MoveResult<()> {
    if journal.phase != MoveExecutionPhase::Locked {
        return Ok(());
    }

    if let Some(parent) = journal.destination_entity_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let started = Instant::now();
    if journal.source_entity_path.exists() {
        fs::rename(
            &journal.source_entity_path,
            &journal.destination_entity_path,
        )?;
    }
    record_phase_timing(journal, "rename_entity_ms", started.elapsed());
    journal.phase = MoveExecutionPhase::Moved;
    span.record("phase", phase_name(&journal.phase));
    journal.updated_at = Utc::now();
    journal.steps.push("moved entity folder".to_string());
    persist_journal(journal_root, journal)?;
    tracing::debug!(
        target: MOVE_TRACE_TARGET,
        phase = phase_name(&journal.phase),
        "move_phase_advanced"
    );
    Ok(())
}

pub(super) fn advance_phase_moved<D: MoveDomain + ?Sized>(
    domain: &D,
    plan: &MovePlan,
    journal: &mut MoveJournal,
    journal_root: &Path,
    span: &tracing::Span,
) -> MoveResult<()> {
    if journal.phase != MoveExecutionPhase::Moved {
        return Ok(());
    }

    if journal.rewritten_path_files.is_empty()
        && journal.manual_followups.is_empty()
    {
        let started = Instant::now();
        let (rewritten, followups) = rewrite_path_references(plan)?;
        record_phase_timing(journal, "rewrite_path_refs_ms", started.elapsed());
        if !rewritten.is_empty() {
            journal.steps.push(format!(
                "rewrote {} tracked path reference files",
                rewritten.len()
            ));
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
        journal.migrated_board_entries = domain.migrate_board_history(
            &journal.target_store_root,
            &journal.entity_id,
        )?;
        record_phase_timing(
            journal,
            "migrate_board_history_ms",
            started.elapsed(),
        );
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
    record_phase_timing(journal, "scan_source_ms", started.elapsed());
    journal.phase = MoveExecutionPhase::SourceScanned;
    span.record("phase", phase_name(&journal.phase));
    journal.updated_at = Utc::now();
    journal.steps.push("scanned source store".to_string());
    persist_journal(journal_root, journal)?;
    tracing::debug!(
        target: MOVE_TRACE_TARGET,
        phase = phase_name(&journal.phase),
        rewritten_path_files = journal.rewritten_path_files.len(),
        manual_followups = journal.manual_followups.len(),
        migrated_board_entries = journal.migrated_board_entries.len(),
        "move_phase_advanced"
    );
    Ok(())
}

pub(super) fn advance_phase_source_scanned<D: MoveDomain + ?Sized>(
    domain: &D,
    journal: &mut MoveJournal,
    journal_root: &Path,
    span: &tracing::Span,
) -> MoveResult<()> {
    if journal.phase != MoveExecutionPhase::SourceScanned {
        return Ok(());
    }

    let started = Instant::now();
    domain.reconcile_store_touched(
        &journal.target_store_root,
        &[journal.entity_id],
    )?;
    record_phase_timing(journal, "scan_target_ms", started.elapsed());
    journal.phase = MoveExecutionPhase::TargetScanned;
    span.record("phase", phase_name(&journal.phase));
    journal.updated_at = Utc::now();
    journal.steps.push("scanned target store".to_string());
    persist_journal(journal_root, journal)?;
    tracing::debug!(
        target: MOVE_TRACE_TARGET,
        phase = phase_name(&journal.phase),
        "move_phase_advanced"
    );
    Ok(())
}

pub(super) fn advance_phase_target_scanned<D: MoveDomain + ?Sized>(
    domain: &D,
    journal: &mut MoveJournal,
    journal_root: &Path,
    span: &tracing::Span,
) -> MoveResult<()> {
    if journal.phase != MoveExecutionPhase::TargetScanned {
        return Ok(());
    }

    let started = Instant::now();
    let source_path_exists = journal.source_entity_path.exists();
    let destination_path_exists = journal.destination_entity_path.exists();
    let source_seen = domain
        .entity_indexed_in(&journal.source_store_root, &journal.entity_id)?;
    let target_seen = domain
        .entity_indexed_in(&journal.target_store_root, &journal.entity_id)?;
    record_phase_timing(journal, "validate_move_ms", started.elapsed());
    if source_path_exists || !destination_path_exists {
        return Err(build_post_move_validation_error(
            journal,
            source_path_exists,
            destination_path_exists,
            source_seen,
            target_seen,
        ));
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
    persist_journal(journal_root, journal)?;
    tracing::debug!(
        target: MOVE_TRACE_TARGET,
        phase = phase_name(&journal.phase),
        "move_phase_advanced"
    );
    Ok(())
}

pub(super) fn build_post_move_validation_error(
    journal: &MoveJournal,
    source_path_exists: bool,
    destination_path_exists: bool,
    source_seen: bool,
    target_seen: bool,
) -> MoveError {
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
            "destination store {} does not index entity {} after the move (expected entity folder {} — check that the destination store root resolved without a Windows verbatim `\\?\\` prefix)",
            normalize_slashes(&journal.target_store_root),
            journal.entity_id,
            normalize_slashes(&journal.destination_entity_path),
        ));
    }
    MoveError::Domain(format!(
        "post-move validation failed: {}",
        problems.join("; ")
    ))
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
        paths.insert(
            root.join(MOVE_LOCKS_DIR)
                .join(format!("entity-{}.lock", entity_id)),
        );
    }
    paths.into_iter().collect()
}

pub(super) fn acquire_lock_set(lock_paths: &[PathBuf]) -> MoveResult<()> {
    let mut acquired = Vec::new();
    for path in lock_paths {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
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

pub(super) fn release_lock_set(lock_paths: &[PathBuf]) {
    for path in lock_paths {
        let _ = fs::remove_file(path);
    }
}

pub(super) fn record_phase_timing(
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

pub(super) fn phase_name(phase: &MoveExecutionPhase) -> &'static str {
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

pub(super) fn restore_rewritten_path(
    rewrite: &MovePathRewrite
) -> MoveResult<()> {
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
            restored =
                restored.replace(&replacement.after, &replacement.before);
        }
        fs::write(&rewrite.path, restored.as_bytes())?;
        return Ok(());
    }

    if path_buf_is_empty(&rewrite.repo_root)
        || path_buf_is_empty(&rewrite.repo_relative_path)
    {
        return Err(MoveError::Domain(format!(
            "journal rewrite record for {} is missing rollback metadata",
            normalize_slashes(&rewrite.path)
        )));
    }

    git_restore_tracked_path(&rewrite.repo_root, &rewrite.repo_relative_path)
}

pub(super) fn recovery_hint_for_phase(
    phase: &MoveExecutionPhase
) -> &'static str {
    match phase {
        MoveExecutionPhase::Planned | MoveExecutionPhase::Locked =>
            "run resume_move to continue execution",
        MoveExecutionPhase::Moved
        | MoveExecutionPhase::SourceScanned
        | MoveExecutionPhase::TargetScanned =>
            "run rollback_move for safety, or resume_move to retry",
        MoveExecutionPhase::Validated | MoveExecutionPhase::RolledBack =>
            "no recovery action needed",
    }
}

pub(super) fn rewrite_path_references(
    plan: &MovePlan
) -> MoveResult<(Vec<MovePathRewrite>, Vec<MoveManualFollowup>)> {
    let old_abs = normalize_slashes(&plan.source_entity_path);
    let new_abs = normalize_slashes(&plan.destination_entity_path);

    let mut relative_pairs = Vec::new();
    if let (Ok(old_rel), Ok(new_rel)) = (
        safe_strip_prefix(
            &plan.source_entity_path,
            &plan.source_git_worktree_root,
        ),
        safe_strip_prefix(
            &plan.destination_entity_path,
            &plan.source_git_worktree_root,
        ),
    ) {
        relative_pairs
            .push((normalize_slashes(&old_rel), normalize_slashes(&new_rel)));
    }
    if let (Ok(old_rel), Ok(new_rel)) = (
        safe_strip_prefix(
            &plan.source_entity_path,
            &plan.target_git_worktree_root,
        ),
        safe_strip_prefix(
            &plan.destination_entity_path,
            &plan.target_git_worktree_root,
        ),
    ) {
        relative_pairs
            .push((normalize_slashes(&old_rel), normalize_slashes(&new_rel)));
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
                reason: "binary or non-utf8 content requires manual rewrite"
                    .to_string(),
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

pub(super) fn normalize_slashes(path: &Path) -> String {
    crate::workspace::normalize_slashes(path)
}

pub(super) fn normalize_journal_entity_paths<D: MoveDomain + ?Sized>(
    domain: &D,
    journal: &mut MoveJournal,
) {
    let subdir = domain.entity_subdir();
    let expected_source = journal
        .source_store_root
        .join(subdir)
        .join(journal.entity_id.to_string());
    let expected_destination = journal
        .target_store_root
        .join(subdir)
        .join(journal.entity_id.to_string());

    if journal.source_entity_path != expected_source {
        journal.source_entity_path = expected_source;
    }
    if journal.destination_entity_path != expected_destination {
        journal.destination_entity_path = expected_destination;
    }
}

pub(super) fn journal_path(
    store_root: &Path,
    id: Uuid,
) -> PathBuf {
    store_root
        .join(MOVE_JOURNALS_DIR)
        .join(format!("{}.json", id))
}

/// Persist a move journal under the store root's `move-journals/` directory.
///
/// Enforces the journal-backed operation interoperability contract at this
/// persistence boundary: a journal missing authoritative identity, replay/
/// rollback lineage, or deterministic mutation payload ownership is rejected
/// and never written to disk.
pub fn persist_journal(
    store_root: &Path,
    journal: &MoveJournal,
) -> MoveResult<()> {
    journal.validate_interoperability_contract()?;
    let path = journal_path(store_root, journal.id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = serde_json::to_vec_pretty(journal)
        .map_err(|error| MoveError::Domain(error.to_string()))?;
    fs::write(path, payload).map_err(MoveError::Io)
}

/// Load a move journal from the store root's `move-journals/` directory.
pub fn load_journal(
    store_root: &Path,
    id: Uuid,
) -> MoveResult<MoveJournal> {
    let payload =
        fs::read(journal_path(store_root, id)).map_err(MoveError::Io)?;
    serde_json::from_slice(&payload)
        .map_err(|error| MoveError::Domain(error.to_string()))
}

pub(super) fn classify_git_worktree_topology(
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

pub(super) fn git_toplevel(path: &Path) -> Result<PathBuf, String> {
    let output = Command::new("git")
        .args([
            "-C",
            &path.to_string_lossy(),
            "rev-parse",
            "--show-toplevel",
        ])
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

pub(super) fn git_tracked_path_reference_files(
    repo_root: &Path,
    entity_path: &Path,
) -> Result<Vec<PathBuf>, String> {
    let mut candidates = BTreeSet::new();
    candidates.insert(entity_path.to_string_lossy().replace('\\', "/"));
    if let Ok(relative) = safe_strip_prefix(entity_path, repo_root) {
        candidates.insert(relative.to_string_lossy().replace('\\', "/"));
    }

    let mut files = BTreeSet::new();
    for candidate in candidates {
        let output = Command::new("git")
            .args([
                "-C",
                &repo_root.to_string_lossy(),
                "grep",
                "-nF",
                "--full-name",
                &candidate,
            ])
            .output()
            .map_err(|error| error.to_string())?;

        if !output.status.success() && output.status.code() != Some(1) {
            return Err(String::from_utf8_lossy(&output.stderr)
                .trim()
                .to_string());
        }

        for line in String::from_utf8_lossy(&output.stdout).lines() {
            if let Some((file, _)) = line.split_once(':') {
                files.insert(PathBuf::from(file));
            }
        }
    }

    Ok(files.into_iter().collect())
}

pub(super) fn is_persistent_move_reference_file(
    file: &Path,
    source_store_root: &Path,
    target_store_root: &Path,
    entity_subdir: &str,
) -> bool {
    let entity_subdir = Path::new(entity_subdir);
    for store_root in [source_store_root, target_store_root] {
        let is_hidden_store = store_root.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            s.starts_with('.') && !s.starts_with(".tmp")
        });
        if is_hidden_store {
            if let Ok(relative) = safe_strip_prefix(file, store_root) {
                return relative.starts_with(entity_subdir);
            }
        }
    }

    true
}

pub(super) fn tracked_repo_for_file<'a>(
    file: &Path,
    source_repo_root: &'a Path,
    target_repo_root: &'a Path,
) -> Option<(&'a Path, PathBuf)> {
    let mut candidates = Vec::new();
    if let Ok(relative) = safe_strip_prefix(file, source_repo_root) {
        candidates.push((source_repo_root, relative));
    }
    if let Ok(relative) = safe_strip_prefix(file, target_repo_root) {
        candidates.push((target_repo_root, relative));
    }

    candidates.sort_by_key(|(_, relative)| {
        std::cmp::Reverse(relative.components().count())
    });
    candidates.into_iter().next()
}

pub(super) fn git_restore_tracked_path(
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

pub(super) fn safe_strip_prefix(
    path: &Path,
    prefix: &Path,
) -> Result<PathBuf, std::path::StripPrefixError> {
    let p_norm = PathBuf::from(crate::workspace::normalize_slashes(path));
    let s_norm = PathBuf::from(crate::workspace::normalize_slashes(prefix));
    p_norm.strip_prefix(&s_norm).map(|p| p.to_path_buf())
}

#[cfg(test)]
mod persist_journal_contract_tests {
    use super::*;

    fn journal_with_id(id: Uuid) -> MoveJournal {
        MoveJournal {
            id,
            entity_id: Uuid::new_v4(),
            source_store_root: PathBuf::from("/stores/source"),
            target_store_root: PathBuf::from("/stores/target"),
            source_entity_path: PathBuf::from("/stores/source/entity"),
            destination_entity_path: PathBuf::from("/stores/target/entity"),
            phase: MoveExecutionPhase::Planned,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            steps: vec!["created move journal".to_string()],
            rollback_steps: vec!["rename destination back to source".to_string()],
            lock_paths: Vec::new(),
            migrated_board_entries: Vec::new(),
            rewritten_path_files: Vec::new(),
            manual_followups: Vec::new(),
            phase_timings_ms: std::collections::BTreeMap::new(),
            failure: None,
            next_recovery_step: None,
        }
    }

    #[test]
    fn persist_journal_rejects_non_compliant_journal_and_writes_nothing() {
        let store = tempfile::tempdir().expect("temp store root");
        let id = Uuid::new_v4();
        let mut journal = journal_with_id(id);
        // Strip deterministic mutation payload ownership + operation identity.
        journal.entity_id = Uuid::nil();
        journal.source_store_root = PathBuf::new();

        let error = persist_journal(store.path(), &journal)
            .expect_err("non-compliant journal must be rejected at persistence");
        match error {
            MoveError::Domain(detail) => assert!(
                detail.contains(MoveJournal::INTEROP_CONTRACT_MARKER),
                "unexpected detail: {detail}"
            ),
            other => panic!("unexpected error variant: {other:?}"),
        }

        assert!(
            !journal_path(store.path(), id).exists(),
            "rejected journal must not be written to disk"
        );
    }

    #[test]
    fn persist_journal_writes_compliant_journal() {
        let store = tempfile::tempdir().expect("temp store root");
        let id = Uuid::new_v4();
        let journal = journal_with_id(id);

        persist_journal(store.path(), &journal)
            .expect("compliant journal must persist");

        assert!(
            journal_path(store.path(), id).exists(),
            "compliant journal must be written to disk"
        );
    }
}
