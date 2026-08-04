impl SessionStoreConfig {
    /// Best-effort worktree/ticket inference at Copilot capture time
    /// (ticket bba9b313, root cause of e5f8a2c1's empty `sessions_for_ticket`
    /// results): the capture hook runs passively and never calls
    /// `check_in_worktree`, so a session otherwise carries no `branch`,
    /// `worktree_path`, or `ticket_id` at all.
    ///
    /// Resolves both from the current git environment using ONLY the branch
    /// name shape (never transcript text — spec e5f8a2c1 forbids transcript
    /// scanning for linkage at every tier) and reuses the backfill's
    /// short-id parser and ticket-store resolver so the two paths never
    /// diverge into separate parsing logic.
    ///
    /// A no-op whenever a worktree assignment already exists on the session:
    /// an explicit `check_in_worktree` (or a prior run of this same
    /// inference) always outranks a fresh guess. Never writes an
    /// unresolved ticket id — a branch shape that resolves to no real
    /// ticket leaves `ticket_id` untouched.
    pub fn infer_worktree_from_environment(
        &self,
        session_id: &str,
        working_dir: &Path,
    ) -> Result<(), SessionError> {
        let mut record = self.read_session(session_id)?;
        if record.metadata.worktree.is_some() {
            return Ok(());
        }

        let Some(branch) = current_git_branch(working_dir) else {
            return Ok(());
        };
        let worktree_path = current_git_toplevel(working_dir)
            .unwrap_or_else(|| working_dir.to_path_buf());

        let ticket_store_root = self.ticket_store_root();
        let ticket_store = if ticket_store_root.exists() {
            TicketStore::open(&ticket_store_root).ok()
        } else {
            None
        };
        let ticket_id = parse_agent_branch_short_id(&branch)
            .and_then(|short_id| {
                resolve_ticket_prefix(ticket_store.as_ref(), &short_id)
            });

        record.metadata.worktree = Some(SessionWorktreeAssignment {
            path: worktree_path,
            branch,
            allocation_mode: SessionWorktreeAllocationMode::New,
            status: SessionWorktreeStatus::Active,
            predecessor_session_id: None,
            predecessor_path: None,
        });
        if let Some(ticket_id) = ticket_id {
            record.metadata.ticket_id = Some(ticket_id);
        }
        record.captured_at = chrono::Utc::now();

        self.persist_record(record)?;
        Ok(())
    }
}

/// Resolves the current branch via `git rev-parse --abbrev-ref HEAD`.
/// Returns `None` (quietly) for a non-git directory, a missing `git`
/// binary, or any other resolution failure. Returns `Some("HEAD")` for a
/// detached HEAD, which never matches the `agent/<short-id>-<slug>` shape
/// and so yields no ticket id downstream.
fn current_git_branch(working_dir: &Path) -> Option<String> {
    let output = std::process::Command::new("git")
        .current_dir(working_dir)
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let branch = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!branch.is_empty()).then_some(branch)
}

/// Resolves the working tree root via `git rev-parse --show-toplevel`.
/// Returns `None` when git is unavailable or the directory is not inside a
/// work tree; callers fall back to the raw working directory.
fn current_git_toplevel(working_dir: &Path) -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .current_dir(working_dir)
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let path = String::from_utf8(output.stdout).ok()?.trim().to_string();
    (!path.is_empty()).then_some(PathBuf::from(path))
}
