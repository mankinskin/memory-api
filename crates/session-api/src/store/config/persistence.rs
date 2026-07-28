impl SessionStoreConfig {
    fn resolve_validation_gates(
        &self,
        context: &SessionRuntimeContext,
        validation: Vec<SessionValidationGate>,
        strict_required: bool,
    ) -> Result<Vec<SessionValidationGate>, SessionError> {
        let mut by_id = BTreeMap::<String, SessionValidationGate>::new();
        for gate in validation {
            by_id.insert(gate.validation_spec_id.clone(), gate);
        }

        let required_specs = context
            .workflow
            .nodes
            .iter()
            .filter(|node| {
                node.kind == crate::SessionWorkflowNodeKind::Validation
                    && node.requirement
                        == crate::SessionWorkflowNodeRequirement::Required
            })
            .map(|node| {
                node.validation_spec_id.clone().ok_or_else(|| {
                    SessionError::FinishBlocked {
                        reason: format!(
                            "required validation node {} is missing validation_spec_id; \
                             repair it with workflow_update_node (MCP: session_workflow_update_node) \
                             to set validation_spec_id, or remove the node with \
                             workflow_remove_node (MCP: session_workflow_remove_node)",
                            node.node_id
                        ),
                    }
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        if required_specs.is_empty() {
            return Ok(by_id.into_values().collect());
        }

        let test_store = self.test_store_config();
        for spec_id in required_specs {
            // Fail closed when a required guard references an unknown validation spec.
            test_store.get_spec(&spec_id).map_err(|error| {
                SessionError::FinishBlocked {
                    reason: format!(
                        "required validation {} is unavailable: {}",
                        spec_id, error
                    ),
                }
            })?;

            // Required outcomes are ALWAYS derived from the authoritative test-api
            // execution record. Caller-provided outcomes are never accepted as
            // completion authority; they may only identify or display a gate.
            let latest = test_store
                .list_executions(&ExecutionQuery {
                    validation_spec_id: Some(spec_id.clone()),
                    limit: Some(1),
                    ..ExecutionQuery::default()
                })
                .map_err(|error| SessionError::FinishBlocked {
                    reason: format!(
                        "required validation {} could not be queried: {}",
                        spec_id, error
                    ),
                })?;
            let outcome = latest
                .into_iter()
                .next()
                .map(|execution| validation_outcome_label(execution.outcome));

            // Fail closed for absent executions, failed executions, and blocked
            // executions when finish requires strict authoritative evidence.
            if strict_required && outcome.as_deref() != Some("passed") {
                return Err(SessionError::FinishBlocked {
                    reason: format!(
                        "required validation {} is not passed (authoritative outcome: {})",
                        spec_id,
                        outcome.as_deref().unwrap_or("no execution record")
                    ),
                });
            }

            by_id.insert(
                spec_id.clone(),
                SessionValidationGate {
                    validation_spec_id: spec_id,
                    required: true,
                    outcome,
                    command: None,
                },
            );
        }

        Ok(by_id.into_values().collect())
    }

    pub(crate) fn paths_for_session_id(
        &self,
        session_id: &str,
    ) -> Result<SessionStorePaths, SessionError> {
        if self.root.as_os_str().is_empty() {
            return Err(SessionError::EmptyStoreRoot);
        }
        validate_segment(session_id, false)?;

        let session_dir = self.root.join("sessions").join(session_id);
        let manifest_path = session_dir.join("session.json");
        let transcript_path = session_dir.join("transcript.json");
        let events_path = session_dir.join("events.json");

        if manifest_path.parent().is_none()
            || transcript_path.parent().is_none()
            || events_path.parent().is_none()
        {
            return Err(SessionError::InvalidStorePath(session_dir));
        }

        Ok(SessionStorePaths {
            session_dir,
            manifest_path,
            transcript_path,
            events_path,
        })
    }

    fn sessions_root(&self) -> Result<PathBuf, SessionError> {
        if self.root.as_os_str().is_empty() {
            return Err(SessionError::EmptyStoreRoot);
        }
        Ok(self.root.join("sessions"))
    }

    fn local_root(&self) -> Result<PathBuf, SessionError> {
        if self.root.as_os_str().is_empty() {
            return Err(SessionError::EmptyStoreRoot);
        }
        Ok(self.root.join("local"))
    }

    pub(super) fn active_workspace_session_path(&self) -> Result<PathBuf, SessionError> {
        Ok(self.local_root()?.join("active_workspace_session.json"))
    }

    pub(super) fn runtime_paths_for_workspace(
        &self,
        workspace_session_id: &str,
    ) -> Result<SessionRuntimePaths, SessionError> {
        validate_runtime_workspace_id(workspace_session_id)?;
        let workspace_dir = self
            .sessions_root()?
            .join(workspace_session_id);
        let context_path = workspace_dir.join("context.json");
        let handoffs_dir = workspace_dir.join("handoffs");
        let finish_path = workspace_dir.join("finish.json");
        Ok(SessionRuntimePaths {
            workspace_dir,
            context_path,
            handoffs_dir,
            finish_path,
        })
    }

    fn persist_runtime_context(
        &self,
        context: &SessionRuntimeContext,
    ) -> Result<(), SessionError> {
        let paths =
            self.runtime_paths_for_workspace(&context.workspace_session_id)?;
        fs::create_dir_all(&paths.workspace_dir).map_err(|source| {
            SessionError::Io {
                path: paths.workspace_dir.clone(),
                source,
            }
        })?;

        write_json(
            &paths.context_path,
            &PersistedRuntimeContext::from(context.clone()),
        )
    }

    fn ticket_store_root(&self) -> PathBuf {
        sibling_store_root(&self.root, ".ticket")
    }

    fn spec_store_root(&self) -> PathBuf {
        sibling_store_root(&self.root, ".spec")
    }

    fn test_store_root(&self) -> PathBuf {
        sibling_store_root(&self.root, ".test")
    }

    fn test_store_config(&self) -> TestStoreConfig {
        TestStoreConfig::new(
            self.test_store_root(),
            self.workspace_slug.clone(),
        )
    }

    fn default_ticket_state_resolver(
        &self
    ) -> Result<DefaultTicketStateResolver, SessionError> {
        let store = TicketStore::open_or_init(&self.ticket_store_root())
            .map_err(|error| {
                SessionError::InvalidHookInput(format!(
                    "ticket store resolver unavailable: {error}"
                ))
            })?;
        Ok(DefaultTicketStateResolver {
            store,
            spec_store_root: self.spec_store_root(),
            workspace_slug: self.workspace_slug.clone(),
        })
    }

    fn resolve_workspace_session_id(
        &self,
        requested: Option<String>,
    ) -> Result<String, SessionError> {
        if let Some(id) = requested {
            validate_runtime_workspace_id(&id)?;
            return Ok(id);
        }

        // Try new path first (.session/local/)
        let active_path = self.active_workspace_session_path()?;
        if let Some(active) = read_json_if_exists::<
            PersistedActiveWorkspaceSession,
        >(&active_path)?
        {
            validate_runtime_workspace_id(&active.workspace_session_id)?;
            return Ok(active.workspace_session_id);
        }

        Ok(Uuid::new_v4().to_string())
    }

    fn persist_active_workspace_session(
        &self,
        workspace_session_id: &str,
    ) -> Result<(), SessionError> {
        validate_runtime_workspace_id(workspace_session_id)?;
        let path = self.active_workspace_session_path()?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| SessionError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        write_json(
            &path,
            &PersistedActiveWorkspaceSession {
                workspace_session_id: workspace_session_id.to_string(),
                updated_at: chrono::Utc::now(),
            },
        )
    }

    pub fn plan_capture(
        &self,
        request: SessionCaptureRequest,
    ) -> Result<SessionStorePlan, SessionError> {
        let (mut record, events) = request.into_record_and_events()?;
        
        // Compute cost_usd for turns with token attribution (ticket 6549b6a7)
        let price_table = crate::price_loader::load_price_table(&self.root).ok();
        if let Some(table) = &price_table {
            for turn in &mut record.turns {
                if let Some(meta) = &mut turn.event_meta {
                    if let (Some(model_id), Some(input), Some(output)) =
                        (&meta.model_id, meta.input_tokens, meta.output_tokens)
                    {
                        meta.cost_usd = crate::price_loader::compute_cost_usd(
                            model_id,
                            input,
                            output,
                            meta.cache_read_tokens.unwrap_or(0),
                            meta.cache_write_tokens.unwrap_or(0),
                            table,
                        );
                    }
                }
            }
        }
        
        let paths = self.paths_for(&record)?;
        let events = if events.is_empty() {
            None
        } else {
            Some(PersistedSessionEvents {
                schema_version: record.schema_version,
                session_id: record.session_id.clone(),
                captured_at: record.captured_at,
                events,
            })
        };
        Ok(SessionStorePlan {
            record,
            paths,
            events,
        })
    }

    pub fn persist_capture(
        &self,
        request: SessionCaptureRequest,
    ) -> Result<SessionStorePlan, SessionError> {
        let plan = self.plan_capture(request)?;
        plan.persist()?;
        Ok(plan)
    }

    fn persist_record(
        &self,
        record: SessionRecord,
    ) -> Result<SessionStorePlan, SessionError> {
        let paths = self.paths_for(&record)?;
        let plan = SessionStorePlan {
            record,
            paths,
            events: None,
        };
        plan.persist()?;
        Ok(plan)
    }

}
