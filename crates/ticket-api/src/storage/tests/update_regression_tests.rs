use super::*;
#[test]
fn bug_7f4aaa05_state_preserved_on_field_patch_without_to_state() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    // Create ticket
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // Advance to ready
    store
        .update(&id, BTreeMap::new(), Some(&[]), Some("ready"), None, None)
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("ready"));

    // BUG: Update description WITHOUT to_state - state should be preserved
    let mut patch = BTreeMap::new();
    patch.insert(
        "custom_field".to_string(),
        Value::String("custom value".to_string()),
    );

    store.update(&id, patch, None, None, None, None).unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(
        indexed.state.as_deref(),
        Some("ready"),
        "State should be preserved when patching fields without to_state"
    );
}

#[test]
fn bug_7f4aaa05_description_patch_with_to_state_transition() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    // Create ticket
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // Advance to ready
    store
        .update(&id, BTreeMap::new(), Some(&[]), Some("ready"), None, None)
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("ready"));

    // Combined: patch fields AND transition in one call
    let mut patch = BTreeMap::new();
    patch.insert(
        "custom_field".to_string(),
        Value::String("custom value".to_string()),
    );

    store
        .update(&id, patch, None, Some("in-implementation"), None, None)
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(
        indexed.state.as_deref(),
        Some("in-implementation"),
        "State should transition to in-implementation"
    );
    assert_eq!(
        indexed.title.as_deref(),
        Some("Test ticket"),
        "Title should be preserved"
    );
}

#[test]
fn bug_7f4aaa05_transition_states_multi_step_path() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    // Create ticket
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // Multi-step transition: new -> ready
    let transition_states = vec!["ready".to_string()];
    store
        .update(
            &id,
            BTreeMap::new(),
            Some(transition_states.as_slice()),
            None, // NO to_state
            None,
            None,
        )
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(
        indexed.state.as_deref(),
        Some("ready"),
        "transition_states should apply the final state from the path"
    );
}

#[test]
fn update_routes_depends_on_patch_to_canonical_edge_ops() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let source = store
        .create(
            None,
            "tracker-improvement",
            Some("Source"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let target_a = store
        .create(
            None,
            "tracker-improvement",
            Some("Target A"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let target_b = store
        .create(
            None,
            "tracker-improvement",
            Some("Target B"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    store
        .add_edge(EdgeRecord {
            from: source,
            to: target_a,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        })
        .unwrap();

    let mut patch = BTreeMap::new();
    patch.insert(
        "depends_on".to_string(),
        Value::Array(vec![Value::String(target_b.to_string())]),
    );
    store
        .update(&source, patch, None, None, None, None)
        .unwrap();

    let manifest = store.get(&source).unwrap();
    let items = manifest
        .extra
        .get("depends_on")
        .and_then(Value::as_array)
        .unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].as_str(), Some(target_b.to_string().as_str()));

    let edges = store.edges_from(&source).unwrap();
    assert_eq!(edges.len(), 1);
    assert_eq!(edges[0].to, target_b);
    assert_eq!(edges[0].kind, "depends_on");
}

#[test]
fn update_auto_walks_reachable_multi_step_by_default() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Reachable multi-step forward"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // `new -> in-implementation` is reachable only by traversing `ready`.
    // Without an explicit opt-out, the update auto-walks the path and lands
    // on the requested target state.
    store
        .update(
            &id,
            BTreeMap::new(),
            None,
            Some("in-implementation"),
            None,
            None,
        )
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("in-implementation"));
}

#[test]
fn update_blocks_reachable_multi_step_under_single_hop_flag() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Reachable multi-step forward"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // Under the `single_hop` opt-out, `new -> in-implementation` is rejected
    // with a recovery-oriented error rather than silently walking the path.
    let err = store
        .update_with_options(
            &id,
            BTreeMap::new(),
            None,
            Some("in-implementation"),
            None,
            DescriptionUpdateMode::default(),
            None,
            true,
        )
        .unwrap_err();

    match err {
        crate::error::StorageError::Validation(
            crate::error::SchemaValidationError::InvalidTransition {
                from,
                to,
                allowed_next,
                intermediate,
            },
        ) => {
            assert_eq!(from, "new");
            assert_eq!(to, "in-implementation");
            assert!(
                allowed_next.contains(&"ready".to_string()),
                "allowed next states should list the legal single-hop targets: {allowed_next:?}"
            );
            assert!(
                intermediate.contains(&"ready".to_string()),
                "intermediate path should name the mandatory waypoint: {intermediate:?}"
            );
        },
        other => panic!("expected InvalidTransition, got {other:?}"),
    }

    // The blocked update must not have advanced the ticket.
    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("new"));
}

#[test]
fn update_auto_walks_reachable_reverse_multi_step_by_default() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Reachable multi-step reverse"),
            Some("in-implementation"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    store
        .update(&id, BTreeMap::new(), None, Some("new"), None, None)
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("new"));
}

#[test]
fn update_blocks_reachable_reverse_multi_step_under_single_hop_flag() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Reachable multi-step reverse"),
            Some("in-implementation"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let err = store
        .update_with_options(
            &id,
            BTreeMap::new(),
            None,
            Some("new"),
            None,
            DescriptionUpdateMode::default(),
            None,
            true,
        )
        .unwrap_err();

    let message = err.to_string();
    assert!(
        message.contains("allows next states"),
        "reverse multi-step block should surface allowed next states: {message}"
    );

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("in-implementation"));
}

#[test]
fn update_without_description_preserves_existing_description() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("new"),
            Default::default(),
            None,
            Some("Original description"),
        )
        .unwrap();

    // Regression: a field-only update that never intended to touch the
    // description must not clobber the existing description.md content.
    let mut patch = BTreeMap::new();
    patch.insert(
        "custom_field".to_string(),
        Value::String("custom value".to_string()),
    );
    store.update(&id, patch, None, None, None, None).unwrap();

    let path = store.get_indexed(&id).unwrap().unwrap().path;
    let description = crate::storage::ticket_fs::TicketFs::read_description(&path);
    assert_eq!(
        description.as_deref(),
        Some("Original description"),
        "an update that omits description must preserve the existing description"
    );
}

#[test]
fn update_with_replace_mode_overwrites_description() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("new"),
            Default::default(),
            None,
            Some("Original description"),
        )
        .unwrap();

    store
        .update_with_options(
            &id,
            BTreeMap::new(),
            None,
            None,
            Some("New description"),
            DescriptionUpdateMode::Replace,
            None,
            false,
        )
        .unwrap();

    let path = store.get_indexed(&id).unwrap().unwrap().path;
    let description = crate::storage::ticket_fs::TicketFs::read_description(&path);
    assert_eq!(description.as_deref(), Some("New description"));
}

#[test]
fn update_with_append_mode_concatenates_description() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("new"),
            Default::default(),
            None,
            Some("Original description"),
        )
        .unwrap();

    store
        .update_with_options(
            &id,
            BTreeMap::new(),
            None,
            None,
            Some("Extra note"),
            DescriptionUpdateMode::Append,
            None,
            false,
        )
        .unwrap();

    let path = store.get_indexed(&id).unwrap().unwrap().path;
    let description = crate::storage::ticket_fs::TicketFs::read_description(&path);
    assert_eq!(
        description.as_deref(),
        Some("Original description\nExtra note"),
        "append mode should concatenate onto the existing description"
    );
}

#[test]
fn update_captures_previous_description_in_history_regardless_of_mode() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("new"),
            Default::default(),
            None,
            Some("Original description"),
        )
        .unwrap();

    store
        .update_with_options(
            &id,
            BTreeMap::new(),
            None,
            None,
            Some("New description"),
            DescriptionUpdateMode::Replace,
            None,
            false,
        )
        .unwrap();

    let revisions = store.get_history(&id).unwrap();
    let last = revisions.last().expect("history revision recorded");
    assert_eq!(
        last.fields.get(crate::storage::store::DESCRIPTION_HISTORY_KEY),
        Some(&Value::String("Original description".to_string())),
        "the pre-update description must be captured in history on every description change"
    );
}

#[test]
fn undo_restores_previous_description() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("new"),
            Default::default(),
            None,
            Some("Original description"),
        )
        .unwrap();

    store
        .update_with_options(
            &id,
            BTreeMap::new(),
            None,
            None,
            Some("New description"),
            DescriptionUpdateMode::Replace,
            None,
            false,
        )
        .unwrap();

    let path = store.get_indexed(&id).unwrap().unwrap().path;
    assert_eq!(
        crate::storage::ticket_fs::TicketFs::read_description(&path).as_deref(),
        Some("New description")
    );

    let revisions = store.get_history(&id).unwrap();
    let previous = &revisions[revisions.len() - 2];
    let mut revert_fields = previous.fields.clone();
    if let Some(desc_val) = revisions[revisions.len() - 1]
        .fields
        .get(crate::storage::store::DESCRIPTION_HISTORY_KEY)
    {
        revert_fields.insert(
            crate::storage::store::DESCRIPTION_HISTORY_KEY.to_string(),
            desc_val.clone(),
        );
    }
    store.apply_revert(&id, revert_fields, None).unwrap();

    assert_eq!(
        crate::storage::ticket_fs::TicketFs::read_description(&path).as_deref(),
        Some("Original description"),
        "undo must restore the pre-overwrite description, making it recoverable"
    );
}

