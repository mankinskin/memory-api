impl SessionStoreConfig {
    fn ensure_no_active_worktree_conflict(
        &self,
        requested_path: &Path,
        ignored_session_id: Option<&str>,
    ) -> Result<(), SessionError> {
        for record in self.query_sessions(&SessionQuery::default())? {
            if ignored_session_id == Some(record.session_id.as_str()) {
                continue;
            }

            let Some(worktree) = record.metadata.worktree.as_ref() else {
                continue;
            };

            if worktree.status == SessionWorktreeStatus::Active
                && worktree.path == requested_path
            {
                return Err(SessionError::WorktreeConflict {
                    path: requested_path.to_path_buf(),
                    session_id: record.session_id,
                });
            }
        }

        Ok(())
    }
}
