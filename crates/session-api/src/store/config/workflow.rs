impl SessionStoreConfig {
    pub fn workflow_update_node_status(
        &self,
        workspace_session_id: &str,
        node_id: &str,
        status: SessionWorkflowNodeStatus,
        deferred_reason: Option<String>,
    ) -> Result<SessionRuntimeContext, SessionError> {
        let _lock = self.begin_runtime_mutation(workspace_session_id)?;
        let mut context = self.read_runtime_context(workspace_session_id)?;
        let node = context
            .workflow
            .nodes
            .iter_mut()
            .find(|node| node.node_id == node_id)
            .ok_or_else(|| {
                SessionError::InvalidHookInput(format!(
                    "unknown workflow node id: {node_id}"
                ))
            })?;

        node.status = status;
        node.deferred_reason = if status == SessionWorkflowNodeStatus::Deferred
        {
            deferred_reason
        } else {
            None
        };
        node.updated_at = chrono::Utc::now();
        context.updated_at = node.updated_at;
        self.persist_runtime_context(&context)?;
        Ok(context)
    }

    pub fn workflow_add_edge(
        &self,
        workspace_session_id: &str,
        from: &str,
        to: &str,
        kind: SessionWorkflowEdgeKind,
    ) -> Result<SessionRuntimeContext, SessionError> {
        let _lock = self.begin_runtime_mutation(workspace_session_id)?;
        let mut context = self.read_runtime_context(workspace_session_id)?;
        let known = context
            .workflow
            .nodes
            .iter()
            .map(|node| node.node_id.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        if !known.contains(from) || !known.contains(to) {
            return Err(SessionError::InvalidHookInput(format!(
                "cannot link unknown workflow nodes: {from} -> {to}"
            )));
        }

        let edge = SessionWorkflowEdge {
            from: from.to_string(),
            to: to.to_string(),
            kind,
        };
        if !context
            .workflow
            .edges
            .iter()
            .any(|existing| existing == &edge)
        {
            context.workflow.edges.push(edge);
            sort_workflow_graph(&mut context.workflow);
            context.updated_at = chrono::Utc::now();
            self.persist_runtime_context(&context)?;
        }
        Ok(context)
    }

    pub fn workflow_promote_node_to_ticket(
        &self,
        workspace_session_id: &str,
        node_id: &str,
        ticket_urn: &str,
        cached_ticket_title: Option<String>,
    ) -> Result<SessionRuntimeContext, SessionError> {
        let parsed = parse_entity_urn(ticket_urn)?;
        if parsed.kind != SessionPinnedEntityKind::Ticket {
            return Err(SessionError::InvalidHookInput(format!(
                "promotion requires a ticket URN, got {ticket_urn}"
            )));
        }
        let _lock = self.begin_runtime_mutation(workspace_session_id)?;
        let mut context = self.read_runtime_context(workspace_session_id)?;
        let node = context
            .workflow
            .nodes
            .iter_mut()
            .find(|node| node.node_id == node_id)
            .ok_or_else(|| {
                SessionError::InvalidHookInput(format!(
                    "unknown workflow node id: {node_id}"
                ))
            })?;

        node.kind = crate::SessionWorkflowNodeKind::Ticket;
        node.ticket_urn = Some(ticket_urn.to_string());
        if cached_ticket_title.is_some() {
            node.cached_ticket_title = cached_ticket_title;
        }
        node.updated_at = chrono::Utc::now();
        context.updated_at = node.updated_at;
        self.persist_runtime_context(&context)?;
        Ok(context)
    }

    pub fn workflow_snapshot(
        &self,
        workspace_session_id: &str,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<SessionWorkflowSnapshot, SessionError> {
        let context = self.read_runtime_context(workspace_session_id)?;
        let mut resolutions = Vec::new();
        let mut diagnostics = Vec::new();
        let owned_resolver = if resolver.is_none() {
            Some(self.default_ticket_state_resolver()?)
        } else {
            None
        };
        let resolver = resolver.or(owned_resolver
            .as_ref()
            .map(|item| item as &dyn SessionTicketStateResolver));

        if let Some(resolver) = resolver {
            for node in &context.workflow.nodes {
                // Ticket-backed nodes resolve authoritative live ticket state.
                if let Some(ticket_urn) = node.ticket_urn.as_deref() {
                    match resolver.resolve_ticket_state(ticket_urn) {
                        Ok(state) =>
                            resolutions.push(SessionWorkflowNodeResolution {
                                node_id: node.node_id.clone(),
                                live_ticket_state: state,
                            }),
                        Err(message) =>
                            diagnostics.push(SessionWorkflowDiagnostic {
                                node_id: node.node_id.clone(),
                                code: "ticket-state-unavailable".to_string(),
                                message,
                            }),
                    }
                }

                // Spec-backed nodes resolve authoritative live spec state,
                // symmetric to ticket resolution. The `live_ticket_state` slot
                // carries the live entity state regardless of backing kind.
                if let Some(spec_urn) = node.spec_urn.as_deref() {
                    match resolver.resolve_spec_state(spec_urn) {
                        Ok(state) =>
                            resolutions.push(SessionWorkflowNodeResolution {
                                node_id: node.node_id.clone(),
                                live_ticket_state: state,
                            }),
                        Err(message) =>
                            diagnostics.push(SessionWorkflowDiagnostic {
                                node_id: node.node_id.clone(),
                                code: "spec-state-unavailable".to_string(),
                                message,
                            }),
                    }
                }
            }
        }

        Ok(SessionWorkflowSnapshot {
            workflow: context.workflow,
            resolutions,
            diagnostics,
        })
    }

    pub fn workflow_render_terminal(
        &self,
        workspace_session_id: &str,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<String, SessionError> {
        let snapshot =
            self.workflow_snapshot(workspace_session_id, resolver)?;
        let mut lines = Vec::new();
        lines.push(format!("workflow {}", workspace_session_id));

        let live_states = snapshot
            .resolutions
            .iter()
            .map(|item| (item.node_id.clone(), item.live_ticket_state.clone()))
            .collect::<BTreeMap<_, _>>();

        let mut blockers = BTreeMap::<String, Vec<String>>::new();
        for edge in &snapshot.workflow.edges {
            if edge.kind != SessionWorkflowEdgeKind::DependsOn {
                continue;
            }
            if let Some(dependency) = snapshot
                .workflow
                .nodes
                .iter()
                .find(|node| node.node_id == edge.to)
            {
                if !node_is_effectively_done(
                    dependency,
                    live_states.get(&dependency.node_id),
                ) {
                    blockers
                        .entry(edge.from.clone())
                        .or_default()
                        .push(edge.to.clone());
                }
            }
        }

        for node in &snapshot.workflow.nodes {
            let requirement = match node.requirement {
                crate::SessionWorkflowNodeRequirement::Required => "required",
                crate::SessionWorkflowNodeRequirement::Optional => "optional",
            };
            let live_state = live_states
                .get(&node.node_id)
                .and_then(|state| state.as_deref())
                .unwrap_or("-");
            let blockers_for_node = blockers
                .get(&node.node_id)
                .cloned()
                .unwrap_or_default()
                .join(",");
            let blocker_view = if blockers_for_node.is_empty() {
                "-".to_string()
            } else {
                blockers_for_node
            };

            lines.push(format!(
                "- {} [{} {}] ticket_state={} blockers={} {}",
                node.node_id,
                requirement,
                workflow_status_label(node.status),
                live_state,
                blocker_view,
                node.title
            ));
        }

        for diag in &snapshot.diagnostics {
            lines.push(format!(
                "! {} {} {}",
                diag.node_id, diag.code, diag.message
            ));
        }

        Ok(lines.join("\n"))
    }

    pub fn workflow_render_mermaid(
        &self,
        workspace_session_id: &str,
        resolver: Option<&dyn SessionTicketStateResolver>,
    ) -> Result<String, SessionError> {
        let snapshot =
            self.workflow_snapshot(workspace_session_id, resolver)?;
        let live_states = snapshot
            .resolutions
            .iter()
            .map(|item| (item.node_id.clone(), item.live_ticket_state.clone()))
            .collect::<BTreeMap<_, _>>();

        let mut lines = vec!["flowchart TD".to_string()];
        for node in &snapshot.workflow.nodes {
            let req = match node.requirement {
                crate::SessionWorkflowNodeRequirement::Required => "req",
                crate::SessionWorkflowNodeRequirement::Optional => "opt",
            };
            let live = live_states
                .get(&node.node_id)
                .and_then(|state| state.as_deref())
                .unwrap_or("-");
            let label = format!(
                "{} |{}| |{}| |ticket:{}|",
                node.title,
                req,
                workflow_status_label(node.status),
                live
            );
            lines.push(format!(
                "  {}[\"{}\"]",
                mermaid_node_id(&node.node_id),
                escape_mermaid_label(&label)
            ));
        }

        for edge in &snapshot.workflow.edges {
            let arrow = match edge.kind {
                SessionWorkflowEdgeKind::DependsOn => "-->|depends_on|",
                SessionWorkflowEdgeKind::Order => "-->|order|",
            };
            lines.push(format!(
                "  {} {} {}",
                mermaid_node_id(&edge.from),
                arrow,
                mermaid_node_id(&edge.to)
            ));
        }

        for diag in &snapshot.diagnostics {
            let diag_id = format!("diag_{}", mermaid_node_id(&diag.node_id));
            lines.push(format!(
                "  {}((\"{}\"))",
                diag_id,
                escape_mermaid_label(&format!(
                    "{}: {}",
                    diag.code, diag.message
                ))
            ));
            lines.push(format!(
                "  {} -.-> {}",
                diag_id,
                mermaid_node_id(&diag.node_id)
            ));
        }

        Ok(lines.join("\n"))
    }

}
