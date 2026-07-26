
#[test]
fn workflow_snapshot_resolves_live_state_and_emits_missing_diagnostics() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-live".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "Existing ticket".to_string(),
                ticket_urn: Some(
                    "ce://default/tickets/412964a3-e1c3-47da-94ad-268ff20441c0"
                        .to_string(),
                ),
                spec_urn: None,
                anchor_urn: None,
                category: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    let missing_urn =
        "ce://default/tickets/deadbeef-dead-beef-dead-beefdeadbeef";
    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-missing".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "Missing ticket".to_string(),
                ticket_urn: Some(missing_urn.to_string()),
                spec_urn: None,
                anchor_urn: None,
                category: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();

    let snapshot = config
        .workflow_snapshot(
            &workspace_id,
            Some(&MockTicketResolver {
                missing_urn: missing_urn.to_string(),
            }),
        )
        .unwrap();

    assert!(
        snapshot
            .resolutions
            .iter()
            .any(|item| item.node_id == "node-live"
                && item.live_ticket_state.as_deref() == Some("in-review"))
    );
    assert!(
        snapshot
            .diagnostics
            .iter()
            .any(|diag| diag.node_id == "node-missing"
                && diag.code == "ticket-state-unavailable")
    );
}

#[test]
fn workflow_render_outputs_are_deterministic_and_escaped() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-a".to_string()),
                kind: SessionWorkflowNodeKind::Task,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "Run \"workflow\" check".to_string(),
                ticket_urn: None,
                spec_urn: None,
                anchor_urn: None,
                category: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-b".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Optional,
                title: "Ticket fallback".to_string(),
                ticket_urn: Some(
                    "ce://default/tickets/deadbeef-dead-beef-dead-beefdeadbeef"
                        .to_string(),
                ),
                spec_urn: None,
                anchor_urn: None,
                category: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    config
        .workflow_add_edge(
            &workspace_id,
            "node-a",
            "node-b",
            SessionWorkflowEdgeKind::DependsOn,
        )
        .unwrap();

    let resolver = MockTicketResolver {
        missing_urn:
            "ce://default/tickets/deadbeef-dead-beef-dead-beefdeadbeef"
                .to_string(),
    };

    let terminal_first = config
        .workflow_render_terminal(&workspace_id, Some(&resolver))
        .unwrap();
    let terminal_second = config
        .workflow_render_terminal(&workspace_id, Some(&resolver))
        .unwrap();
    assert_eq!(terminal_first, terminal_second);
    assert!(terminal_first.contains("ticket-state-unavailable"));
    assert!(terminal_first.contains("node-a"));
    assert!(terminal_first.contains("blockers=node-b"));

    let mermaid_first = config
        .workflow_render_mermaid(&workspace_id, Some(&resolver))
        .unwrap();
    let mermaid_second = config
        .workflow_render_mermaid(&workspace_id, Some(&resolver))
        .unwrap();
    assert_eq!(mermaid_first, mermaid_second);
    assert!(mermaid_first.starts_with("flowchart TD\n"));
    assert!(mermaid_first.contains("Run \\\"workflow\\\" check"));
    assert!(mermaid_first.contains("-->|depends_on|"));
}

#[test]
fn workflow_render_is_read_only_for_runtime_persistence() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-read-only".to_string()),
                kind: SessionWorkflowNodeKind::Task,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "render check".to_string(),
                ticket_urn: None,
                spec_urn: None,
                anchor_urn: None,
                category: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();

    let runtime_paths =
        config.runtime_paths_for_workspace(&workspace_id).unwrap();
    let before = std::fs::read(&runtime_paths.context_path).unwrap();

    let _ = config
        .workflow_render_terminal(&workspace_id, None)
        .unwrap();
    let _ = config.workflow_render_mermaid(&workspace_id, None).unwrap();

    let after = std::fs::read(&runtime_paths.context_path).unwrap();
    assert_eq!(before, after);
}

#[test]
fn handoff_persists_before_render_and_resume_links_new_run() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    let _rendered = config
        .render_handoff_terminal(
            &workspace_id,
            None,
            vec![crate::SessionValidationGate {
                validation_spec_id: "val-session-handoff-continuity"
                    .to_string(),
                required: true,
                outcome: Some("passed".to_string()),
            }],
            None,
        )
        .unwrap();

    let paths = config.runtime_paths_for_workspace(&workspace_id).unwrap();
    let handoff_files = std::fs::read_dir(&paths.handoffs_dir)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(handoff_files.len(), 1);

    let handoff_path = handoff_files[0].path();
    let handoff: crate::SessionHandoffRecord =
        serde_json::from_slice(&std::fs::read(handoff_path).unwrap()).unwrap();
    assert_eq!(handoff.workspace_session_id, workspace_id);
    assert_eq!(handoff.outgoing_run_id, init.context.active_run_id);
    assert!(handoff.resume_command.contains(&workspace_id));
    assert!(handoff.resume_command.contains(&handoff.outgoing_run_id));

    let resumed = config
        .resume_workspace_context(&workspace_id, &handoff.outgoing_run_id)
        .unwrap();
    assert_eq!(resumed.context.workspace_session_id, workspace_id);
    assert_ne!(resumed.run.run_id, handoff.outgoing_run_id);
    assert_eq!(
        resumed.run.predecessor_run_id.as_deref(),
        Some(handoff.outgoing_run_id.as_str())
    );
}

#[test]
fn handoff_package_missing_objective_is_rejected() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(crate::SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    let result = config.create_handoff_record(
        &workspace_id,
        Some(crate::SessionHandoffPackage {
            objective: "".to_string(), // missing
            target_tickets: vec![],
            ..Default::default()
        }),
        vec![],
        None,
    );

    assert!(
        result.is_err(),
        "expected Err for incomplete package but got Ok"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("incomplete") || err.contains("missing"),
        "unexpected error message: {err}"
    );
}

#[test]
fn handoff_package_round_trip_persists_schema_fields() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(crate::SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    let package = crate::SessionHandoffPackage {
        objective: "Implement required-field enforcement".to_string(),
        target_tickets: vec!["d3af78d7-9486-43c0-aae7-ddd5681d9807"
            .to_string()],
        target_files: vec![
            "memory-api/crates/session-api/src/model/handoff.rs".to_string(),
        ],
        decisions: vec!["Use Option<SessionHandoffPackage> for backward compat"
            .to_string()],
        non_goals: vec!["UI/viewer representation".to_string()],
        context_anchors: vec!["spec:5e52039d".to_string()],
        open_escalations: vec![],
        risk_notes: Some("none".to_string()),
        predecessor_handoff: None,
    };

    let result = config
        .create_handoff_result(&workspace_id, Some(package.clone()), vec![], None)
        .expect("handoff with complete package");

    assert_eq!(result.record.objective, package.objective);
    assert_eq!(result.record.target_tickets, package.target_tickets);
    assert_eq!(result.record.target_files, package.target_files);
    assert_eq!(result.record.decisions, package.decisions);
    assert_eq!(result.record.non_goals, package.non_goals);
    assert_eq!(result.record.context_anchors, package.context_anchors);
    assert!(result.record.open_escalations.is_empty());
    assert!(result.render.contains("implementation_ready: true"));
    assert!(result.render.contains("objective:"));

    // Verify the record is persisted and can be re-read from disk.
    let paths = config.runtime_paths_for_workspace(&workspace_id).unwrap();
    let handoff_path =
        paths.handoffs_dir.join(format!("{}.json", result.record.handoff_id));
    let on_disk: crate::SessionHandoffRecord =
        serde_json::from_slice(&std::fs::read(&handoff_path).unwrap()).unwrap();
    assert_eq!(on_disk.objective, package.objective);
    assert_eq!(on_disk.target_tickets, package.target_tickets);
}

#[test]
fn handoff_package_with_open_escalations_persists_but_not_ready() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(crate::SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    let package = crate::SessionHandoffPackage {
        objective: "Implement something".to_string(),
        open_escalations: vec!["Must resolve cross-crate dep question first"
            .to_string()],
        ..Default::default()
    };

    let result = config
        .create_handoff_result(&workspace_id, Some(package), vec![], None)
        .expect("handoff with escalations should persist");

    assert!(!result.record.open_escalations.is_empty());
    assert!(result.render.contains("implementation_ready: false"));
    assert!(result.render.contains("open_escalations:"));
}
