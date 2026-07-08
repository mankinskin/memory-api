use super::*;
#[test]
fn workflow_facts_set_became_actionable_at_when_blockers_resolve() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Blocking prerequisite"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let dependent = store
        .create(
            None,
            "tracker-improvement",
            Some("Blocked dependent"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    store
        .add_edge(EdgeRecord {
            from: dependent,
            to: blocker,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        })
        .unwrap();

    let initial = store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(initial.unresolved_dependency_count, 1);
    assert!(initial.became_actionable_at.is_none());

    store.close(&blocker, "done", None).unwrap();

    let updated = store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(updated.unresolved_dependency_count, 0);
    assert!(updated.became_actionable_at.is_some());
    assert!(updated.last_blocker_progress_at.is_none());
}

#[test]
fn workflow_facts_set_last_blocker_progress_at_while_ticket_remains_blocked() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let progressing_blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Progressing blocker"),
            Some("new"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let persistent_blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Persistent blocker"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let dependent = store
        .create(
            None,
            "tracker-improvement",
            Some("Still blocked dependent"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    for blocker in [progressing_blocker, persistent_blocker] {
        store
            .add_edge(EdgeRecord {
                from: dependent,
                to: blocker,
                kind: "depends_on".to_string(),
                created_at: Utc::now(),
            })
            .unwrap();
    }

    let initial = store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(initial.unresolved_dependency_count, 2);
    assert!(initial.last_blocker_progress_at.is_none());

    store
        .update(
            &progressing_blocker,
            BTreeMap::new(),
            Some(&[]),
            Some("ready"),
            None,
            None,
        )
        .unwrap();

    let updated = store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(updated.unresolved_dependency_count, 2);
    assert!(updated.last_blocker_progress_at.is_some());
    assert!(updated.became_actionable_at.is_none());
}

#[test]
fn update_allows_reverse_transitions_from_terminal_states() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let done_ticket = store
        .create(
            None,
            "tracker-improvement",
            Some("Reopen done ticket"),
            Some("done"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    store
        .update(
            &done_ticket,
            BTreeMap::new(),
            Some(&[]),
            Some("in-review"),
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        store.get_indexed(&done_ticket).unwrap().unwrap().state.as_deref(),
        Some("in-review")
    );

    let cancelled_ticket = store
        .create(
            None,
            "tracker-improvement",
            Some("Reopen cancelled ticket"),
            Some("cancelled"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    store
        .update(
            &cancelled_ticket,
            BTreeMap::new(),
            Some(&[]),
            Some("new"),
            None,
            None,
        )
        .unwrap();
    assert_eq!(
        store
            .get_indexed(&cancelled_ticket)
            .unwrap()
            .unwrap()
            .state
            .as_deref(),
        Some("new")
    );
}

#[test]
fn workflow_facts_follow_depends_on_edge_removal() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Transient blocker"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let dependent = store
        .create(
            None,
            "tracker-improvement",
            Some("Edge-driven dependent"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let edge = EdgeRecord {
        from: dependent,
        to: blocker,
        kind: "depends_on".to_string(),
        created_at: Utc::now(),
    };

    store.add_edge(edge.clone()).unwrap();
    assert_eq!(
        store
            .get_workflow_facts(&dependent)
            .unwrap()
            .unwrap()
            .unresolved_dependency_count,
        1
    );

    store.remove_edge(edge).unwrap();

    let updated = store.get_workflow_facts(&dependent).unwrap().unwrap();
    assert_eq!(updated.unresolved_dependency_count, 0);
    assert!(updated.became_actionable_at.is_some());
    assert!(updated.last_blocker_progress_at.is_none());
}

