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
        store
            .get_indexed(&done_ticket)
            .unwrap()
            .unwrap()
            .state
            .as_deref(),
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

#[test]
fn update_guards_transition_ahead_of_dependency_state() {
    use crate::error::StorageError;

    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let blocker = store
        .create(
            None,
            "tracker-improvement",
            Some("Guard blocker"),
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
            Some("Guard dependent"),
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

    // ready -> ready (equal rank) is allowed.
    store
        .update(&dependent, BTreeMap::new(), Some(&[]), Some("ready"), None, None)
        .unwrap();

    // Advancing the dependent past the blocker (still 'ready') is rejected.
    let err = store
        .update(
            &dependent,
            BTreeMap::new(),
            Some(&[]),
            Some("in-implementation"),
            None,
            None,
        )
        .unwrap_err();
    assert!(
        matches!(err, StorageError::DependencyNotProgressed { .. }),
        "expected DependencyNotProgressed, got {err:?}"
    );

    // Once the blocker advances, the dependent may match its progress.
    store
        .update(
            &blocker,
            BTreeMap::new(),
            Some(&[]),
            Some("in-implementation"),
            None,
            None,
        )
        .unwrap();
    store
        .update(
            &dependent,
            BTreeMap::new(),
            Some(&[]),
            Some("in-implementation"),
            None,
            None,
        )
        .unwrap();

    // A terminal (done) dependency no longer caps the dependent's progress.
    store.close(&blocker, "done", None).unwrap();
    store
        .update(
            &dependent,
            BTreeMap::new(),
            Some(&[]),
            Some("in-review"),
            None,
            None,
        )
        .unwrap();

    // Cancelling is always permitted regardless of dependency progress.
    let gate = store
        .create(
            None,
            "tracker-improvement",
            Some("Cancel gate"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    let abandoned = store
        .create(
            None,
            "tracker-improvement",
            Some("Abandoned dependent"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();
    store
        .add_edge(EdgeRecord {
            from: abandoned,
            to: gate,
            kind: "depends_on".to_string(),
            created_at: Utc::now(),
        })
        .unwrap();
    store
        .update(
            &abandoned,
            BTreeMap::new(),
            Some(&[]),
            Some("cancelled"),
            None,
            None,
        )
        .unwrap();
}

#[test]
fn release_lease_enforces_owner_and_stale_rules() {
    use crate::error::StorageError;

    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let ticket = store
        .create(
            None,
            "tracker-improvement",
            Some("Leased ticket"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // The holder may release its own live lease.
    store.claim(&ticket, "agent-a", 3600, Some("work")).unwrap();
    store.release_lease(&ticket, "agent-a").unwrap();
    assert!(store.list_leases().unwrap().is_empty());

    // A different agent may not release a live lease held by someone else.
    store.claim(&ticket, "agent-a", 3600, Some("work")).unwrap();
    let err = store.release_lease(&ticket, "agent-b").unwrap_err();
    assert!(
        matches!(err, StorageError::LeaseConflict { .. }),
        "expected LeaseConflict, got {err:?}"
    );
    store.release_lease(&ticket, "agent-a").unwrap();

    // Any agent may release a stale (expired) lease.
    store.claim(&ticket, "agent-a", 0, Some("work")).unwrap();
    store.release_lease(&ticket, "agent-b").unwrap();
    assert!(store.list_leases().unwrap().is_empty());

    // Releasing a ticket with no active lease is a no-op.
    store.release_lease(&ticket, "agent-b").unwrap();
}

#[test]
fn board_check_out_releases_orphaned_lease_when_entry_is_missing() {
    use crate::storage::board::BoardError;

    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let ticket = store
        .create(
            None,
            "tracker-improvement",
            Some("Orphaned lease ticket"),
            Some("ready"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    store
        .board_check_in(&ticket, "agent-a", 0, "work", vec![])
        .unwrap();
    assert_eq!(store.list_leases().unwrap().len(), 1);

    let preview = store.board_clean_preview(true).unwrap();
    assert_eq!(preview.entry_ids.len(), 1);
    store.board_clean_apply(&preview.token, true).unwrap();

    let err = store
        .board_check_out(&ticket, "agent-a", Some("cleanup orphan"))
        .unwrap_err();
    assert!(
        matches!(err, BoardError::NotCheckedIn { .. }),
        "expected NotCheckedIn, got {err:?}"
    );
    assert!(store.list_leases().unwrap().is_empty());
}
