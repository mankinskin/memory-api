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
    patch.insert("custom_field".to_string(), Value::String("custom value".to_string()));

    store
        .update(&id, patch, None, None, None, None)
        .unwrap();

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
    patch.insert("custom_field".to_string(), Value::String("custom value".to_string()));

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
fn update_allows_reachable_multi_step_without_transition_states() {
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
fn update_allows_reachable_reverse_multi_step_without_transition_states() {
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
        .update(
            &id,
            BTreeMap::new(),
            None,
            Some("new"),
            None,
            None,
        )
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("new"));
}

