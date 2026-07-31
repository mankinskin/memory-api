impl SessionStoreConfig {
    /// Find a handoff record by id across every session's handoff folder and
    /// return it alongside the path it was read from.
    fn find_handoff(
        &self,
        handoff_id: &str,
    ) -> Result<(SessionHandoffRecord, PathBuf), SessionError> {
        let sessions_root = self.sessions_root()?;
        if !sessions_root.exists() {
            return Err(SessionError::HandoffNotFound {
                handoff_id: handoff_id.to_string(),
            });
        }

        let entries = fs::read_dir(&sessions_root).map_err(|source| {
            SessionError::Io {
                path: sessions_root.clone(),
                source,
            }
        })?;

        for entry in entries {
            let entry = entry.map_err(|source| SessionError::Io {
                path: sessions_root.clone(),
                source,
            })?;
            let handoff_json_path = entry
                .path()
                .join("handoffs")
                .join(handoff_id)
                .join("handoff.json");
            if let Some(record) =
                read_json_if_exists::<SessionHandoffRecord>(&handoff_json_path)?
            {
                return Ok((record, handoff_json_path));
            }
        }

        Err(SessionError::HandoffNotFound {
            handoff_id: handoff_id.to_string(),
        })
    }

    /// Bind a target session to a target-less handoff (pickup). Updates the
    /// target session's `picked_up_handoff_ids` and rejects a handoff that is
    /// already claimed.
    pub fn pickup_handoff(
        &self,
        handoff_id: &str,
        target_session_id: &str,
    ) -> Result<SessionHandoffRecord, SessionError> {
        let (mut record, handoff_json_path) = self.find_handoff(handoff_id)?;

        if let Some(existing_target) = &record.target_session_id {
            return Err(SessionError::HandoffAlreadyClaimed {
                handoff_id: handoff_id.to_string(),
                target_session_id: existing_target.clone(),
            });
        }

        record.target_session_id = Some(target_session_id.to_string());
        write_json(&handoff_json_path, &record)?;

        let mut target_record = self.read_session(target_session_id)?;
        if !target_record
            .picked_up_handoff_ids
            .iter()
            .any(|id| id == handoff_id)
        {
            target_record
                .picked_up_handoff_ids
                .push(handoff_id.to_string());
            self.persist_record(target_record)?;
        }

        Ok(record)
    }

    /// List unclaimed (target-less) handoffs across every session, optionally
    /// narrowed by source session id or the source session's track id.
    pub fn list_unclaimed_handoffs(
        &self,
        filter: &HandoffBacklogFilter,
    ) -> Result<Vec<SessionHandoffRecord>, SessionError> {
        let sessions_root = self.sessions_root()?;
        if !sessions_root.exists() {
            return Ok(vec![]);
        }

        let mut backlog = Vec::new();
        let entries = fs::read_dir(&sessions_root).map_err(|source| {
            SessionError::Io {
                path: sessions_root.clone(),
                source,
            }
        })?;

        for entry in entries {
            let entry = entry.map_err(|source| SessionError::Io {
                path: sessions_root.clone(),
                source,
            })?;
            let session_dir = entry.path();
            if !session_dir.is_dir() {
                continue;
            }

            let handoffs_dir = session_dir.join("handoffs");
            if !handoffs_dir.exists() {
                continue;
            }
            let handoff_entries =
                fs::read_dir(&handoffs_dir).map_err(|source| SessionError::Io {
                    path: handoffs_dir.clone(),
                    source,
                })?;

            for handoff_entry in handoff_entries {
                let handoff_entry =
                    handoff_entry.map_err(|source| SessionError::Io {
                        path: handoffs_dir.clone(),
                        source,
                    })?;
                let handoff_json_path =
                    handoff_entry.path().join("handoff.json");
                let Some(record) = read_json_if_exists::<SessionHandoffRecord>(
                    &handoff_json_path,
                )?
                else {
                    continue;
                };

                if record.target_session_id.is_some() {
                    continue;
                }
                if let Some(source_session_id) = &filter.source_session_id {
                    if &record.workspace_session_id != source_session_id {
                        continue;
                    }
                }
                if let Some(track_id) = &filter.track_id {
                    let source_track_id = self
                        .read_session(&record.workspace_session_id)
                        .ok()
                        .and_then(|source_record| source_record.track_id);
                    if source_track_id.as_ref() != Some(track_id) {
                        continue;
                    }
                }

                backlog.push(record);
            }
        }

        Ok(backlog)
    }
}
