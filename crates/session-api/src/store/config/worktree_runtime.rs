impl SessionStoreConfig {
    pub fn check_in_worktree(
        &self,
        request: SessionWorktreeCheckInRequest,
    ) -> Result<SessionWorktreeCheckInReceipt, SessionError> {
        validate_worktree_request(&request)?;

        if let Ok(mut existing_record) = self.read_session(&request.session_id)
        {
            let existing_assignment =
                existing_record.metadata.worktree.clone().ok_or_else(|| {
                    SessionError::MissingWorktreeAssignment {
                        session_id: request.session_id.clone(),
                    }
                })?;

            let is_unclaimed = existing_record
                .metadata
                .ticket_id
                .as_deref()
                .is_none_or(|ticket_id| ticket_id.trim().is_empty());
            if !is_unclaimed
                && (existing_record.metadata.agent_id.as_deref()
                    != Some(request.owner_id.as_str())
                    || existing_record.metadata.ticket_id.as_deref()
                        != Some(request.ticket_id.as_str()))
            {
                return Err(SessionError::SessionOwnershipMismatch {
                    session_id: request.session_id,
                });
            }

            existing_record.metadata.agent_id = Some(request.owner_id.clone());
            existing_record.metadata.ticket_id = Some(request.ticket_id.clone());

            if can_reuse_assignment(&existing_assignment, &request) {
                existing_record.metadata.worktree =
                    Some(SessionWorktreeAssignment {
                        allocation_mode: SessionWorktreeAllocationMode::Reused,
                        ..existing_assignment
                    });
                existing_record.captured_at = chrono::Utc::now();
                self.persist_record(existing_record.clone())?;
                return receipt_from_record(&existing_record);
            }

            fs::create_dir_all(&request.worktree_path).map_err(|source| {
                SessionError::Io {
                    path: request.worktree_path.clone(),
                    source,
                }
            })?;
            self.ensure_no_active_worktree_conflict(
                &request.worktree_path,
                Some(request.session_id.as_str()),
            )?;

            existing_record.metadata.worktree =
                Some(SessionWorktreeAssignment {
                    path: request.worktree_path,
                    branch: request.branch,
                    allocation_mode: SessionWorktreeAllocationMode::Rotated,
                    status: SessionWorktreeStatus::Active,
                    predecessor_session_id: None,
                    predecessor_path: Some(existing_assignment.path),
                });
            existing_record.captured_at = chrono::Utc::now();
            self.persist_record(existing_record.clone())?;
            return receipt_from_record(&existing_record);
        }

        let mut predecessor_path = None;
        if let Some(predecessor_session_id) = &request.predecessor_session_id {
            let mut predecessor = self.read_session(predecessor_session_id)?;
            let predecessor_assignment =
                predecessor.metadata.worktree.clone().ok_or_else(|| {
                    SessionError::MissingWorktreeAssignment {
                        session_id: predecessor_session_id.clone(),
                    }
                })?;

            if predecessor_assignment.path == request.worktree_path {
                return Err(SessionError::CrossSessionReuseRequiresAdopt {
                    session_id: predecessor_session_id.clone(),
                    path: predecessor_assignment.path,
                });
            }

            predecessor_path = Some(predecessor_assignment.path.clone());
            predecessor.metadata.worktree = Some(SessionWorktreeAssignment {
                status: SessionWorktreeStatus::Superseded,
                ..predecessor_assignment
            });
            predecessor.captured_at = chrono::Utc::now();
            self.persist_record(predecessor)?;
        }

        fs::create_dir_all(&request.worktree_path).map_err(|source| {
            SessionError::Io {
                path: request.worktree_path.clone(),
                source,
            }
        })?;
        self.ensure_no_active_worktree_conflict(&request.worktree_path, None)?;

        let record = SessionRecord {
            schema_version: SESSION_SCHEMA_VERSION,
            session_id: request.session_id,
            source: "session-worktree-check-in".to_string(),
            started_at: chrono::Utc::now(),
            captured_at: chrono::Utc::now(),
            metadata: SessionMetadata {
                workspace_slug: self.workspace_slug.clone(),
                conversation_id: None,
                agent_id: Some(request.owner_id),
                ticket_id: Some(request.ticket_id),
                model: None,
                trigger: Some("session-check-in".to_string()),
                provisioning: None,
                producer: None,
                copilot_version: None,
                vscode_version: None,
                protocol_version: None,
                worktree: Some(SessionWorktreeAssignment {
                    path: request.worktree_path,
                    branch: request.branch,
                    allocation_mode: if request.predecessor_session_id.is_some()
                    {
                        SessionWorktreeAllocationMode::Rotated
                    } else {
                        SessionWorktreeAllocationMode::New
                    },
                    status: SessionWorktreeStatus::Active,
                    predecessor_session_id: request.predecessor_session_id,
                    predecessor_path,
                }),
            },
            turns: vec![],
            links: SessionLinks::default(),
            track_id: None,
            anchor_ticket_id: None,
            parent_session_id: None,
            spawned_session_id: None,
            emitted_handoff_ids: Vec::new(),
            picked_up_handoff_ids: Vec::new(),
        };
        self.persist_record(record.clone())?;
        receipt_from_record(&record)
    }

    pub fn lookup_worktree(
        &self,
        session_id: &str,
    ) -> Result<SessionWorktreeCheckInReceipt, SessionError> {
        let record = self.read_session(session_id)?;
        receipt_from_record(&record)
    }

    /// Return a bounded window of transcript turns for a persisted session.
    pub fn peek_range(
        &self,
        session_id: &str,
        start: usize,
        end: Option<usize>,
    ) -> Result<SessionTurnRange, SessionError> {
        let record = self.read_session(session_id)?;
        Ok(peek_turn_range(&record, start, end))
    }

    /// Return a body-stripped skeleton overview of a persisted session.
    pub fn peek_skeleton(
        &self,
        session_id: &str,
        preview_chars: usize,
    ) -> Result<SessionSkeleton, SessionError> {
        let record = self.read_session(session_id)?;
        Ok(peek_skeleton(&record, preview_chars))
    }

    /// Return a prompt-facing compact view of a persisted session transcript.
    pub fn peek_prompt_pack(
        &self,
        session_id: &str,
        options: PromptPackOptions,
    ) -> Result<SessionPromptPack, SessionError> {
        let record = self.read_session(session_id)?;
        Ok(peek_prompt_pack(&record, options))
    }

    pub fn init_runtime_context(
        &self,
        request: SessionRuntimeInitRequest,
    ) -> Result<SessionRuntimeInitResult, SessionError> {
        let now = chrono::Utc::now();
        let session_id =
            self.resolve_session_id(request.session_id)?;

        // Serialize lineage updates with every other runtime mutation and with
        // finish. Without the lock, a concurrent pin/workflow mutation (or a
        // second init/resume) could read the same context and clobber the run
        // lineage this call appends.
        let _lock = self.acquire_runtime_lock(&session_id)?;

        let mut created_workspace = false;
        let mut created_run = false;

        let mut context = match self.read_runtime_context(&session_id)
        {
            Ok(context) => context,
            Err(SessionError::RuntimeContextNotFound { .. }) => {
                created_workspace = true;
                created_run = true;
                let run = SessionRunLineage {
                    run_id: Uuid::new_v4().to_string(),
                    predecessor_run_id: request.predecessor_run_id.clone(),
                    captured_session_id: Some(session_id.clone()),
                    started_at: now,
                };

                SessionRuntimeContext {
                    session_id: session_id.clone(),
                    created_at: now,
                    updated_at: now,
                    active_run_id: run.run_id.clone(),
                    runs: vec![run],
                    pinned_entities: vec![],
                    workflow: Default::default(),
                }
            },
            Err(err) => return Err(err),
        };

        if !created_workspace
            && self
                .runtime_paths_for_workspace(&session_id)?
                .finish_path
                .exists()
        {
            if request.force_new_run || request.predecessor_run_id.is_some() {
                return Err(SessionError::WorkspaceFinished {
                    session_id,
                });
            }

            let run = context.active_run().cloned().ok_or_else(|| {
                SessionError::RuntimeContextNotFound {
                    session_id: session_id.clone(),
                }
            })?;
            return Ok(SessionRuntimeInitResult {
                context,
                run,
                created_workspace: false,
                created_run: false,
            });
        }

        if !created_workspace {
            let predecessor = request
                .predecessor_run_id
                .clone()
                .or_else(|| context.active_run().map(|run| run.run_id.clone()));

            if request.force_new_run || request.predecessor_run_id.is_some() {
                // Appending a new run is a lineage mutation; a finished workspace
                // is immutable, so reject it under the lock.
                self.ensure_workspace_not_finished(&session_id)?;
                let run = SessionRunLineage {
                    run_id: Uuid::new_v4().to_string(),
                    predecessor_run_id: predecessor,
                    captured_session_id: Some(context.canonical_session_id()),
                    started_at: now,
                };
                context.active_run_id = run.run_id.clone();
                context.runs.push(run);
                created_run = true;
            }

            context.updated_at = now;
        }

        self.persist_runtime_state(&context)?;

        let run = context.active_run().cloned().ok_or_else(|| {
            SessionError::RuntimeContextNotFound {
                session_id: session_id.clone(),
            }
        })?;

        Ok(SessionRuntimeInitResult {
            context,
            run,
            created_workspace,
            created_run,
        })
    }

    pub fn read_runtime_context(
        &self,
        session_id: &str,
    ) -> Result<SessionRuntimeContext, SessionError> {
        validate_session_id(session_id)?;
        let session_paths = self.paths_for_session_id(session_id)?;
        let legacy = self.read_legacy_runtime_context(session_id)?;
        let mut manifest = match read_json_if_exists(&session_paths.manifest_path)? {
            Some(manifest) => manifest,
            None if legacy.is_some() => PersistedSessionManifest {
                schema_version: SESSION_SCHEMA_VERSION,
                session_id: session_id.to_string(),
                source: "legacy-runtime-context".to_string(),
                started_at: chrono::Utc::now(),
                captured_at: chrono::Utc::now(),
                metadata: SessionMetadata {
                    workspace_slug: self.workspace_slug.clone(),
                    conversation_id: None,
                    agent_id: None,
                    ticket_id: None,
                    model: None,
                    trigger: None,
                    provisioning: None,
                    producer: None,
                    copilot_version: None,
                    vscode_version: None,
                    protocol_version: None,
                    worktree: None,
                },
                links: SessionLinks::default(),
                track_id: None,
                anchor_ticket_id: None,
                parent_session_id: None,
                spawned_session_id: None,
                emitted_handoff_ids: Vec::new(),
                picked_up_handoff_ids: Vec::new(),
                active_run_id: String::new(),
                runs: Vec::new(),
                pinned_entities: Vec::new(),
                workflow: SessionWorkflowGraph::default(),
            },
            None => {
                return Err(SessionError::RuntimeContextNotFound {
                    session_id: session_id.to_string(),
                });
            }
        };

        ensure_supported_schema_version(
            &session_paths.manifest_path,
            manifest.schema_version,
        )?;

        let mut created_at = manifest.started_at;
        let mut updated_at = manifest.captured_at;
        if let Some(legacy) = legacy {
            if manifest.active_run_id.is_empty() {
                manifest.active_run_id = legacy.active_run_id;
            }
            if manifest.runs.is_empty() {
                manifest.runs = legacy.runs;
            }
            if manifest.pinned_entities.is_empty() {
                manifest.pinned_entities = legacy.pinned_entities;
            }
            if manifest.workflow.is_empty() {
                manifest.workflow = legacy.workflow;
            }
            created_at = legacy.created_at.unwrap_or(created_at);
            updated_at = legacy.updated_at.unwrap_or(updated_at);
        }

        if manifest.active_run_id.is_empty() && manifest.runs.is_empty() {
            return Err(SessionError::RuntimeContextNotFound {
                session_id: session_id.to_string(),
            });
        }

        Ok(SessionRuntimeContext {
            session_id: session_id.to_string(),
            created_at,
            updated_at,
            active_run_id: manifest.active_run_id,
            runs: manifest.runs,
            pinned_entities: manifest.pinned_entities,
            workflow: manifest.workflow,
        })
    }

}
