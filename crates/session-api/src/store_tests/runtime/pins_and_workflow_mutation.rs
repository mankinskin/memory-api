
#[test]
fn pinned_rule_render_contains_only_rule_pins_in_canonical_order() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;
    let mut rule_store =
        rule_api::RuleStore::open_or_init(&store_root.join(".rule")).unwrap();

    let mut later = rule_api::RuleManifest::new(
        "session/render/later",
        "Later",
        ".instructions",
        "later",
        "Later guidance.",
    );
    later.set_order_key(20);
    let later_id = rule_store.create(&later, None).unwrap();
    let mut earlier = rule_api::RuleManifest::new(
        "session/render/earlier",
        "Earlier",
        ".instructions",
        "earlier",
        "Earlier guidance.",
    );
    earlier.set_order_key(10);
    let earlier_id = rule_store.create(&earlier, None).unwrap();

    config
        .pin_runtime_entity(
            &workspace_id,
            &format!("ce://context-engine/rules/{later_id}"),
            None,
            None,
        )
        .unwrap();
    config
        .pin_runtime_entity(
            &workspace_id,
            "ce://context-engine/tickets/11111111-1111-4111-8111-111111111111",
            None,
            None,
        )
        .unwrap();
    config
        .pin_runtime_entity(
            &workspace_id,
            &format!("ce://context-engine/rules/{earlier_id}"),
            None,
            None,
        )
        .unwrap();

    let rendered = config
        .render_pinned_rule_instructions(&workspace_id)
        .unwrap();
    assert!(rendered.contains("Earlier guidance."));
    assert!(rendered.contains("Later guidance."));
    assert!(!rendered.contains("11111111-1111-4111-8111-111111111111"));
    assert!(
        rendered.find("Earlier guidance.").unwrap()
            < rendered.find("Later guidance.").unwrap()
    );
}

#[test]
fn pinned_rule_render_fails_for_missing_rule() {
    let tempdir = TempDir::new().unwrap();
    let store_root = tempdir.path().join("store");
    let config = SessionStoreConfig::new(store_root.clone(), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    rule_api::RuleStore::open_or_init(&store_root.join(".rule")).unwrap();
    config
        .pin_runtime_entity(
            &init.context.workspace_session_id,
            "ce://context-engine/rules/22222222-2222-4222-8222-222222222222",
            None,
            None,
        )
        .unwrap();

    let error = config
        .render_pinned_rule_instructions(&init.context.workspace_session_id)
        .unwrap_err();
    assert!(matches!(error, SessionError::InvalidHookInput(_)));
}

#[test]
fn context_capture_persistence_isolation_is_byte_stable() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");

    let capture = config
        .persist_capture(sample_request(
            "session-isolation",
            Some("conversation-isolation"),
            sample_time(),
            &["capture first"],
        ))
        .unwrap();
    let manifest_before = std::fs::read(&capture.paths.manifest_path).unwrap();
    let transcript_before =
        std::fs::read(&capture.paths.transcript_path).unwrap();

    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;
    config
        .pin_runtime_entity(
            &workspace_id,
            "ce://default/rules/084fd4e6-660b-4227-a13e-514edf44e393",
            Some("handoff".to_string()),
            None,
        )
        .unwrap();

    let manifest_after = std::fs::read(&capture.paths.manifest_path).unwrap();
    let transcript_after =
        std::fs::read(&capture.paths.transcript_path).unwrap();
    assert_eq!(manifest_before, manifest_after);
    assert_eq!(transcript_before, transcript_after);

    let runtime_paths =
        config.runtime_paths_for_workspace(&workspace_id).unwrap();
    let runtime_before = std::fs::read(&runtime_paths.context_path).unwrap();

    config
        .persist_capture(sample_request(
            "session-isolation",
            Some("conversation-isolation"),
            sample_time_later(),
            &["capture first", "capture second"],
        ))
        .unwrap();

    let runtime_after = std::fs::read(&runtime_paths.context_path).unwrap();
    assert_eq!(runtime_before, runtime_after);
}

struct MockTicketResolver {
    missing_urn: String,
}

impl SessionTicketStateResolver for MockTicketResolver {
    fn resolve_ticket_state(
        &self,
        ticket_urn: &str,
    ) -> Result<Option<String>, String> {
        if ticket_urn == self.missing_urn {
            Err("ticket not found".to_string())
        } else {
            Ok(Some("in-review".to_string()))
        }
    }
}

#[test]
fn workflow_persists_mutation_and_reload() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();
    let workspace_id = init.context.workspace_session_id;

    let after_ticket = config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-ticket".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "Implement runtime model".to_string(),
                ticket_urn: Some(
                    "ce://default/tickets/412964a3-e1c3-47da-94ad-268ff20441c0"
                        .to_string(),
                ),
                spec_urn: None,
                category: None,
                cached_ticket_title: Some(
                    "Runtime session context".to_string(),
                ),
                validation_spec_id: None,
            },
        )
        .unwrap();
    assert_eq!(after_ticket.workflow.nodes.len(), 1);

    let after_action = config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-action".to_string()),
                kind: SessionWorkflowNodeKind::Task,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "Write workflow tests".to_string(),
                ticket_urn: None,
                spec_urn: None,
                category: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();
    assert_eq!(after_action.workflow.nodes.len(), 2);

    let linked = config
        .workflow_add_edge(
            &workspace_id,
            "node-action",
            "node-ticket",
            SessionWorkflowEdgeKind::DependsOn,
        )
        .unwrap();
    assert_eq!(linked.workflow.edges.len(), 1);

    let updated = config
        .workflow_update_node_status(
            &workspace_id,
            "node-action",
            SessionWorkflowNodeStatus::Done,
            None,
        )
        .unwrap();
    assert_eq!(
        updated
            .workflow
            .nodes
            .iter()
            .find(|node| node.node_id == "node-action")
            .unwrap()
            .status,
        SessionWorkflowNodeStatus::Done
    );

    let reloaded = config.read_runtime_context(&workspace_id).unwrap();
    assert_eq!(reloaded.workflow.nodes.len(), 2);
    assert_eq!(reloaded.workflow.edges.len(), 1);
}

#[test]
fn workflow_promotion_preserves_node_identity() {
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
                node_id: Some("node-temp".to_string()),
                kind: SessionWorkflowNodeKind::Task,
                requirement: SessionWorkflowNodeRequirement::Optional,
                title: "Investigate follow-up".to_string(),
                ticket_urn: None,
                spec_urn: None,
                category: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap();

    let promoted = config
        .workflow_promote_node_to_ticket(
            &workspace_id,
            "node-temp",
            "ce://default/tickets/70cd7056-c342-4433-ad60-5bc798f61aa6",
            Some("Workflow persistence".to_string()),
        )
        .unwrap();

    let node = promoted
        .workflow
        .nodes
        .iter()
        .find(|node| node.node_id == "node-temp")
        .unwrap();
    assert_eq!(node.kind, SessionWorkflowNodeKind::Ticket);
    assert_eq!(
        node.ticket_urn.as_deref(),
        Some("ce://default/tickets/70cd7056-c342-4433-ad60-5bc798f61aa6")
    );
}

#[test]
fn workflow_ticket_node_rejects_non_ticket_urn() {
    let tempdir = TempDir::new().unwrap();
    let config =
        SessionStoreConfig::new(tempdir.path().join("store"), "context-engine");
    let init = config
        .init_runtime_context(SessionRuntimeInitRequest::default())
        .unwrap();

    let error = config
        .workflow_add_node(
            &init.context.workspace_session_id,
            SessionWorkflowNodeDraft {
                node_id: Some("node-ticket".to_string()),
                kind: SessionWorkflowNodeKind::Ticket,
                requirement: SessionWorkflowNodeRequirement::Required,
                title: "bad type".to_string(),
                ticket_urn: Some(
                    "ce://default/specs/709f067a-21b6-41b6-8879-3cacef4bacaf"
                        .to_string(),
                ),
                spec_urn: None,
                category: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap_err();

    assert!(matches!(error, SessionError::InvalidHookInput(_)));
}
