impl SessionStoreConfig {
    pub fn create_handoff_record(
        &self,
        workspace_session_id: &str,
        package: Option<SessionHandoffPackage>,
        validation: Vec<SessionValidationGate>,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<SessionHandoffRecord, SessionError> {
        // Validate package completeness when a package is supplied.
        // Missing `objective` is a hard error; missing list fields are a soft warning.
        if let Some(ref pkg) = package {
            let missing = pkg.missing_fields();
            if missing.contains(&"objective") {
                return Err(SessionError::HandoffPackageIncomplete {
                    fields: "objective".to_string(),
                });
            }
            if !missing.is_empty() {
                eprintln!(
                    "[session-api] handoff package is missing required list \
                     fields ({fields}); the handoff persists but is not \
                     implementation-ready",
                    fields = missing.join(", ")
                );
            }
        }

        let context = self.read_runtime_context(workspace_session_id)?;
        let workflow =
            self.workflow_snapshot(workspace_session_id, resolver)?;
        let validation =
            self.resolve_validation_gates(&context, validation, false)?;
        let view = self.view_runtime_context(workspace_session_id)?;
        let handoff_id = Uuid::new_v4().to_string();
        let resume_command = format!(
            "session-cli resume --workspace-session-id {} --predecessor-run-id {}",
            context.workspace_session_id, context.active_run_id
        );

        let (
            objective,
            target_tickets,
            target_files,
            decisions,
            non_goals,
            context_anchors,
            open_escalations,
            risk_notes,
            predecessor_handoff,
        ) = package
            .map(|pkg| {
                (
                    pkg.objective,
                    pkg.target_tickets,
                    pkg.target_files,
                    pkg.decisions,
                    pkg.non_goals,
                    pkg.context_anchors,
                    pkg.open_escalations,
                    pkg.risk_notes,
                    pkg.predecessor_handoff,
                )
            })
            .unwrap_or_default();

        let record = SessionHandoffRecord {
            handoff_id: handoff_id.clone(),
            workspace_session_id: context.workspace_session_id.clone(),
            outgoing_run_id: context.active_run_id,
            created_at: chrono::Utc::now(),
            resume_command,
            pinned_entities: view.pinned_headers,
            workflow,
            validation,
            objective,
            target_tickets: target_tickets.clone(),
            target_files,
            decisions,
            non_goals,
            context_anchors,
            open_escalations,
            risk_notes,
            predecessor_handoff,
        };

        let paths = self.runtime_paths_for_workspace(workspace_session_id)?;
        fs::create_dir_all(&paths.handoffs_dir).map_err(|source| {
            SessionError::Io {
                path: paths.handoffs_dir.clone(),
                source,
            }
        })?;

        // Create handoff folder and write both JSON and Markdown
        let handoff_folder = paths.handoffs_dir.join(&handoff_id);
        fs::create_dir_all(&handoff_folder).map_err(|source| {
            SessionError::Io {
                path: handoff_folder.clone(),
                source,
            }
        })?;

        let handoff_json_path = handoff_folder.join("handoff.json");
        write_json(&handoff_json_path, &record)?;

        let handoff_md_path = handoff_folder.join("handoff.md");
        let markdown_content = render_handoff_record_markdown(&record);
        fs::write(&handoff_md_path, markdown_content).map_err(|source| {
            SessionError::Io {
                path: handoff_md_path.clone(),
                source,
            }
        })?;

        // Mirror the handoff onto each target ticket (best-effort; the handoff
        // record is the authoritative source of truth).
        if !target_tickets.is_empty() {
            let _ = self.mirror_handoff_to_tickets(&record, &target_tickets);
        }

        Ok(record)
    }

    fn mirror_handoff_to_tickets(
        &self,
        record: &SessionHandoffRecord,
        target_tickets: &[String],
    ) -> Result<(), SessionError> {
        let store =
            TicketStore::open_or_init(&self.ticket_store_root()).map_err(
                |error| {
                    SessionError::InvalidHookInput(format!(
                        "ticket store unavailable for handoff mirror: {error}"
                    ))
                },
            )?;

        let mirror_value = serde_json::json!({
            "handoff_id": record.handoff_id,
            "objective": record.objective,
            "target_tickets": record.target_tickets,
            "target_files": record.target_files,
            "validation": record.validation,
            "open_escalations": record.open_escalations,
        });

        for ticket_id_str in target_tickets {
            let ticket_id = match Uuid::parse_str(ticket_id_str) {
                Ok(id) => id,
                Err(_) => continue,
            };
            let mut patch = std::collections::BTreeMap::new();
            patch.insert(
                "handoff_package".to_string(),
                mirror_value.clone(),
            );
            // Best-effort: ignore individual ticket update errors.
            let _ = store.update(&ticket_id, patch, None, None, None, None);
        }
        Ok(())
    }

    pub fn create_handoff_result(
        &self,
        workspace_session_id: &str,
        package: Option<SessionHandoffPackage>,
        validation: Vec<SessionValidationGate>,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<SessionHandoffResult, SessionError> {
        let record = self.create_handoff_record(
            workspace_session_id,
            package,
            validation,
            resolver,
        )?;
        let paths = self.runtime_paths_for_workspace(workspace_session_id)?;
        let record_path = paths
            .handoffs_dir
            .join(&record.handoff_id);
        Ok(SessionHandoffResult {
            render: render_handoff_record_terminal(&record),
            record,
            record_path: record_path.to_string_lossy().into_owned(),
        })
    }

    pub fn render_handoff_terminal(
        &self,
        workspace_session_id: &str,
        package: Option<SessionHandoffPackage>,
        validation: Vec<SessionValidationGate>,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<String, SessionError> {
        let result = self.create_handoff_result(
            workspace_session_id,
            package,
            validation,
            resolver,
        )?;
        Ok(result.render)
    }

    pub fn resume_workspace_context(
        &self,
        workspace_session_id: &str,
        predecessor_run_id: &str,
    ) -> Result<SessionRuntimeInitResult, SessionError> {
        let init = self.init_runtime_context(SessionRuntimeInitRequest {
            workspace_session_id: Some(workspace_session_id.to_string()),
            predecessor_run_id: Some(predecessor_run_id.to_string()),
            force_new_run: true,
        })?;

        if init.run.run_id == predecessor_run_id {
            return Err(SessionError::InvalidHookInput(
                "resume must produce a new run id".to_string(),
            ));
        }
        Ok(init)
    }

    pub fn finish_workflow(
        &self,
        workspace_session_id: &str,
        validation: Vec<SessionValidationGate>,
        deferred_optional_node_ids: Vec<String>,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<SessionFinishResult, SessionError> {
        let paths = self.runtime_paths_for_workspace(workspace_session_id)?;
        if let Some(result) = Self::existing_finish_result(&paths.finish_path)?
        {
            return Ok(result);
        }

        // Hold the mutation lock across evaluation and finish-record write so a
        // concurrent workflow mutation cannot interleave with finish.
        let _lock = self.acquire_runtime_lock(workspace_session_id)?;
        // Re-check under the lock: another finish may have won the race.
        if let Some(result) = Self::existing_finish_result(&paths.finish_path)?
        {
            return Ok(result);
        }

        let context = self.read_runtime_context(workspace_session_id)?;
        let snapshot =
            self.workflow_snapshot(workspace_session_id, resolver)?;
        let deferred = deferred_optional_node_ids
            .into_iter()
            .collect::<BTreeSet<_>>();

        Self::evaluate_workflow_completion(&context, &snapshot, &deferred)?;
        let validation =
            self.evaluate_required_validation(&context, validation)?;

        let record = SessionFinishRecord {
            workspace_session_id: workspace_session_id.to_string(),
            run_id: context.active_run_id,
            finished_at: chrono::Utc::now(),
            deferred_optional_node_ids: deferred.into_iter().collect(),
            validation,
        };

        if let Some(parent) = paths.finish_path.parent() {
            fs::create_dir_all(parent).map_err(|source| SessionError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        write_json(&paths.finish_path, &record)?;
        Ok(SessionFinishResult {
            record,
            already_finished: false,
        })
    }

    /// Load a persisted finish record (if any) as an idempotent finish result.
    ///
    /// Extracted so `finish_workflow` can perform the pre-lock fast path and the
    /// under-lock re-check with a single branch each instead of duplicating the
    /// read-and-wrap logic inline.
    fn existing_finish_result(
        finish_path: &Path
    ) -> Result<Option<SessionFinishResult>, SessionError> {
        Ok(
            read_json_if_exists::<SessionFinishRecord>(finish_path)?.map(
                |record| SessionFinishResult {
                    record,
                    already_finished: true,
                },
            ),
        )
    }

    /// Pure completion predicate for finish: verify every required workflow node
    /// is done and every optional node is done or explicitly deferred with a
    /// reason. Ticket nodes whose live state could not be resolved fail closed.
    ///
    /// Extracted from `finish_workflow` so the completion invariant can be unit
    /// tested in isolation and so the locked finish path reads as a linear
    /// sequence rather than an oversized branching function.
    fn evaluate_workflow_completion(
        context: &SessionRuntimeContext,
        snapshot: &SessionWorkflowSnapshot,
        deferred: &BTreeSet<String>,
    ) -> Result<(), SessionError> {
        let live_states = snapshot
            .resolutions
            .iter()
            .map(|item| (item.node_id.clone(), item.live_ticket_state.clone()))
            .collect::<BTreeMap<_, _>>();
        // Ticket-state diagnostics (unavailable / misrouted / not-found) must be
        // able to block finish; they cannot be silently ignored.
        let diagnostics_by_node = snapshot
            .diagnostics
            .iter()
            .map(|diag| (diag.node_id.clone(), diag.message.clone()))
            .collect::<BTreeMap<_, _>>();

        for node in &context.workflow.nodes {
            // A required ticket- or spec-backed node whose live state could not
            // be resolved (missing, misrouted, or otherwise unavailable) fails
            // closed with an explicit unavailable reason instead of a generic
            // "not done". Spec gating is symmetric to ticket gating.
            if node.requirement
                == crate::SessionWorkflowNodeRequirement::Required
                && matches!(
                    node.kind,
                    crate::SessionWorkflowNodeKind::Ticket
                        | crate::SessionWorkflowNodeKind::Spec
                )
            {
                if let Some(message) = diagnostics_by_node.get(&node.node_id) {
                    let backing = match node.kind {
                        crate::SessionWorkflowNodeKind::Spec => "spec",
                        _ => "ticket",
                    };
                    return Err(SessionError::FinishBlocked {
                        reason: format!(
                            "required {} node {} has unavailable live state: {}",
                            backing, node.node_id, message
                        ),
                    });
                }
            }

            let is_done =
                node_is_effectively_done(node, live_states.get(&node.node_id));
            if node.requirement
                == crate::SessionWorkflowNodeRequirement::Required
                && !is_done
            {
                return Err(SessionError::FinishBlocked {
                    reason: format!(
                        "required node {} is not done",
                        node.node_id
                    ),
                });
            }

            if node.requirement
                == crate::SessionWorkflowNodeRequirement::Optional
                && !is_done
            {
                let valid_defer = node.status
                    == SessionWorkflowNodeStatus::Deferred
                    && node.deferred_reason.is_some()
                    && deferred.contains(&node.node_id);
                if !valid_defer {
                    return Err(SessionError::FinishBlocked {
                        reason: format!(
                            "optional node {} must be deferred with a reason",
                            node.node_id
                        ),
                    });
                }
            }
        }

        Ok(())
    }

    /// Resolve required validation gates from the authoritative test store and
    /// verify each required gate passed, returning the merged gate list to
    /// persist. Extracted from `finish_workflow` to keep authoritative-gate
    /// evaluation testable independently of the locked finish sequence.
    fn evaluate_required_validation(
        &self,
        context: &SessionRuntimeContext,
        validation: Vec<SessionValidationGate>,
    ) -> Result<Vec<SessionValidationGate>, SessionError> {
        let validation =
            self.resolve_validation_gates(context, validation, true)?;
        for gate in &validation {
            if gate.required && gate.outcome.as_deref() != Some("passed") {
                return Err(SessionError::FinishBlocked {
                    reason: format!(
                        "required validation {} is not passed",
                        gate.validation_spec_id
                    ),
                });
            }
        }
        Ok(validation)
    }

}
