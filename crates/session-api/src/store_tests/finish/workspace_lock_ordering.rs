
#[test]
fn finished_workspace_plain_init_is_read_only_and_byte_stable() {
    let (config, workspace_id, _tempdir) = finished_workspace();
    let paths = config.runtime_paths_for_workspace(&workspace_id).unwrap();
    let active_path = config.active_workspace_session_path().unwrap();
    let context_before = std::fs::read(&paths.context_path).unwrap();
    let active_before = std::fs::read(&active_path).unwrap();

    let init = config
        .init_runtime_context(SessionRuntimeInitRequest {
            workspace_session_id: Some(workspace_id.clone()),
            predecessor_run_id: None,
            force_new_run: false,
        })
        .unwrap();

    assert!(!init.created_workspace);
    assert!(!init.created_run);
    assert_eq!(init.context.workspace_session_id, workspace_id);
    assert_eq!(std::fs::read(&paths.context_path).unwrap(), context_before);
    assert_eq!(std::fs::read(active_path).unwrap(), active_before);
}

/// High: the finished-workspace check runs *under* the mutation lock. When a
/// finished workspace also has a live lock held, the mutation must fail with a
/// lock conflict (lock acquired first) rather than the finished error — proving
/// the ordering that closes the finish-versus-mutation race.
#[test]
fn finished_check_runs_under_mutation_lock() {
    let (config, workspace_id, _tempdir) = finished_workspace();

    let _lock = config.acquire_runtime_lock(&workspace_id).unwrap();

    let err = config
        .workflow_add_node(
            &workspace_id,
            SessionWorkflowNodeDraft {
                node_id: Some("post-finish-locked".to_string()),
                kind: SessionWorkflowNodeKind::Task,
                requirement: SessionWorkflowNodeRequirement::Optional,
                title: "blocked".to_string(),
                ticket_urn: None,
                spec_urn: None,
                category: None,
                cached_ticket_title: None,
                validation_spec_id: None,
            },
        )
        .unwrap_err();
    assert!(matches!(err, SessionError::RuntimeMutationConflict { .. }));
}
