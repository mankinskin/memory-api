use super::*;

#[test]
fn create_rejects_off_schema_state() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let err = store
        .create(
            None,
            "tracker-improvement",
            Some("Invalid creation state"),
            Some("archived"),
            Default::default(),
            None,
            None,
        )
        .expect_err("off-schema state must be rejected before persistence");

    match err {
        crate::error::StorageError::Validation(
            crate::error::SchemaValidationError::OffSchemaState { state, allowed },
        ) => {
            assert_eq!(state, "archived");
            assert!(allowed.contains(&"open".to_string()));
        },
        other => panic!("expected OffSchemaState, got {other:?}"),
    }
    assert!(store.list(None, None, None).unwrap().is_empty());
}

#[test]
fn off_schema_state_recovers_only_to_entry_state_then_transitions_normally() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Legacy off-schema state"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let mut manifest = store.get(&id).unwrap();
    manifest.extra.insert(
        "state".to_string(),
        Value::String("archived".to_string()),
    );
    let ticket_path = store.get_indexed(&id).unwrap().unwrap().path;
    let toml_str = memory_api::model::manifest_format::format_manifest_toml(&manifest);
    std::fs::write(
        ticket_path.join(crate::model::filesystem::TICKET_MANIFEST_FILE),
        toml_str,
    )
    .unwrap();
    store.scan(true).unwrap();

    let err = store
        .update(&id, BTreeMap::new(), None, Some("planned"), None, None)
        .expect_err("recovery must not jump to a non-entry state");
    assert!(err.to_string().contains("'archived' -> 'planned'"));

    store
        .update(&id, BTreeMap::new(), None, Some("open"), None, None)
        .expect("off-schema state should recover to the entry state");
    store
        .update(&id, BTreeMap::new(), None, Some("planned"), None, None)
        .expect("recovered ticket should transition normally");

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("planned"));
}

#[test]
fn ticket_29a56eef_state_in_field_patch_errors_instead_of_silently_dropping() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // A `state` key sent through a plain field/field_map patch (no
    // `to_state`) must never be silently dropped: it must either apply or
    // return an explicit error naming the field.
    let mut patch = BTreeMap::new();
    patch.insert("state".to_string(), Value::String("planned".to_string()));

    let result = store.update(&id, patch, None, None, None, None);

    match result {
        Ok(_) => {
            let indexed = store.get_indexed(&id).unwrap().unwrap();
            assert_eq!(
                indexed.state.as_deref(),
                Some("planned"),
                "if the write is accepted, it must actually apply"
            );
        },
        Err(err) => {
            let message = err.to_string();
            assert!(
                message.contains("state"),
                "error must name the rejected field, got: {message}"
            );
        },
    }
}

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
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // Advance to ready
    store
        .update(&id, BTreeMap::new(), Some(&[]), Some("planned"), None, None)
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("planned"));

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
        Some("planned"),
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
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // Advance to ready
    store
        .update(&id, BTreeMap::new(), Some(&[]), Some("planned"), None, None)
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("planned"));

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
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // Multi-step transition: new -> ready
    let transition_states = vec!["planned".to_string()];
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
        Some("planned"),
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
            Some("open"),
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
            Some("open"),
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
            Some("open"),
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
            Some("open"),
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
            Some("open"),
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
            Some(DescriptionUpdateMode::Replace),
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
            assert_eq!(from, "open");
            assert_eq!(to, "in-implementation");
            assert!(
                allowed_next.contains(&"planned".to_string()),
                "allowed next states should list the legal single-hop targets: {allowed_next:?}"
            );
            assert!(
                intermediate.contains(&"planned".to_string()),
                "intermediate path should name the mandatory waypoint: {intermediate:?}"
            );
        },
        other => panic!("expected InvalidTransition, got {other:?}"),
    }

    // The blocked update must not have advanced the ticket.
    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("open"));
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
        .update(&id, BTreeMap::new(), None, Some("open"), None, None)
        .unwrap();

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("open"));
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
            Some("open"),
            None,
            Some(DescriptionUpdateMode::Replace),
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
            Some("open"),
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
            Some("open"),
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
            Some(DescriptionUpdateMode::Replace),
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
            Some("open"),
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
            Some(DescriptionUpdateMode::Append),
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
            Some("open"),
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
            Some(DescriptionUpdateMode::Replace),
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
            Some("open"),
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
            Some(DescriptionUpdateMode::Replace),
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

#[test]
fn ticket_3d952036_omitted_description_mode_is_rejected() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("open"),
            Default::default(),
            None,
            Some("Original description"),
        )
        .unwrap();

    // AC1: `description` with no `description_mode` is a hard error, not a
    // silent default.
    let err = store
        .update_with_options(
            &id, BTreeMap::new(), None, None, Some("New description"), None,
            None, false,
        )
        .unwrap_err();

    // AC2: the error names both modes and states which preserves content.
    let message = err.to_string();
    assert!(message.contains("replace"), "error must name 'replace': {message}");
    assert!(message.contains("append"), "error must name 'append': {message}");
    assert!(
        message.to_lowercase().contains("preserv"),
        "error must state which mode preserves existing content: {message}"
    );

    // The rejected write must not have touched the description.
    let path = store.get_indexed(&id).unwrap().unwrap().path;
    assert_eq!(
        crate::storage::ticket_fs::TicketFs::read_description(&path).as_deref(),
        Some("Original description"),
        "a rejected write must leave the existing description untouched"
    );
}

/// Rework of ticket 3d952036 (AC5): a prior iteration validated the omitted
/// -mode case only at runtime inside `apply_manifest_update`. This test
/// proves the *boundary* type (`DescriptionUpdate`, threaded through
/// `UpdateTicketBody`/`UpdateArgs`/the MCP handler decode) makes "content
/// without a mode" unrepresentable: there is no `description_mode` field on
/// those types to omit, only a single `description_update: DescriptionUpdate`
/// field whose three variants (`Unchanged`/`Replace(String)`/`Append(String)`)
/// each already encode a complete, valid combination.
///
/// This is an exhaustiveness-dependent unit test, not a compile-fail
/// (trybuild) test — the repository has no trybuild dependency, and this
/// ticket's scope note says to check before introducing a new one. The
/// compile-time guarantee itself is structural: `DescriptionUpdate::as_parts`
/// matches all three variants with no catch-all arm, so adding a fourth
/// variant without updating every match site (including this test) is a
/// compiler error, and no Rust code can construct
/// `DescriptionUpdate::Replace`/`Append` without also supplying content, nor
/// omit a mode when constructing a variant that carries content.
#[test]
fn ticket_3d952036_description_update_makes_missing_mode_unrepresentable() {
    // Boundary decode still rejects a raw wire `description` with no
    // `description_mode` string (AC1/AC2), but the value produced by a
    // *successful* decode is always one of three fully-formed variants —
    // there is no fourth "content, no mode" shape to construct.
    let err = DescriptionUpdate::decode(
        Some("New description".to_string()),
        None,
    )
    .unwrap_err();
    assert_eq!(err, REQUIRED_DESCRIPTION_MODE_ERROR);

    // Every successful decode round-trips through `as_parts` into exactly
    // the `(content, mode)` pair the two are paired on — Replace/Append
    // always carry both, Unchanged always carries neither.
    let unchanged = DescriptionUpdate::decode(None, None).unwrap();
    assert_eq!(unchanged, DescriptionUpdate::Unchanged);
    assert_eq!(unchanged.as_parts(), (None, None));

    let replace =
        DescriptionUpdate::decode(Some("R".to_string()), Some("replace"))
            .unwrap();
    assert_eq!(replace, DescriptionUpdate::Replace("R".to_string()));
    assert_eq!(
        replace.as_parts(),
        (Some("R"), Some(DescriptionUpdateMode::Replace))
    );

    let append =
        DescriptionUpdate::decode(Some("A".to_string()), Some("append"))
            .unwrap();
    assert_eq!(append, DescriptionUpdate::Append("A".to_string()));
    assert_eq!(
        append.as_parts(),
        (Some("A"), Some(DescriptionUpdateMode::Append))
    );

    // An unrecognized mode string is also rejected at the decode boundary,
    // never silently coerced into a variant.
    let bad = DescriptionUpdate::decode(
        Some("x".to_string()),
        Some("overwrite"),
    )
    .unwrap_err();
    assert!(
        bad.contains("invalid description_mode"),
        "unrecognized mode must be named in the error: {bad}"
    );
}

#[test]
fn ticket_3d952036_review_part_write_leaves_objective_byte_identical() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("open"),
            Default::default(),
            None,
            Some("Original objective content, unchanged."),
        )
        .unwrap();
    let path = store.get_indexed(&id).unwrap().unwrap().path;
    let objective_before =
        fs::read(path.join("description.md")).unwrap();

    // AC3/AC4: a part-addressed write targeting a fresh review part id
    // must never read or write the objective / description.md.
    let review_part_id = Uuid::new_v4();
    store
        .write_part(&id, review_part_id, "review", "Looks good.", None)
        .unwrap();

    let objective_after = fs::read(path.join("description.md")).unwrap();
    assert_eq!(
        objective_before, objective_after,
        "writing a review part must leave the objective bytes byte-identical"
    );

    let manifest = store.get(&id).unwrap();
    let parts = manifest.parts();
    let review = parts
        .iter()
        .find(|p| p.id == review_part_id)
        .expect("review part recorded in manifest");
    assert_eq!(review.kind, "review");
    let review_content =
        fs::read_to_string(path.join(&review.path)).unwrap();
    assert_eq!(review_content, "Looks good.");
}

#[test]
fn ticket_3d952036_per_part_history_and_undo_restores_only_that_part() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Test ticket"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let review_id = Uuid::new_v4();
    store
        .write_part(&id, review_id, "review", "first pass", None)
        .unwrap();
    let other_id = Uuid::new_v4();
    store
        .write_part(&id, other_id, "validation", "unrelated evidence", None)
        .unwrap();
    store
        .write_part(&id, review_id, "review", "second pass", None)
        .unwrap();

    // AC5: history carries the prior content of only the changed part.
    let revisions = store.get_history(&id).unwrap();
    let last = revisions.last().unwrap();
    assert_eq!(
        last.fields.get(crate::storage::store::PART_HISTORY_ID_KEY),
        Some(&Value::String(review_id.to_string()))
    );
    assert_eq!(
        last.fields.get(crate::storage::store::PART_HISTORY_CONTENT_KEY),
        Some(&Value::String("first pass".to_string()))
    );

    store.undo_part(&id, review_id, None).unwrap();

    let path = store.get_indexed(&id).unwrap().unwrap().path;
    let manifest = store.get(&id).unwrap();
    let parts = manifest.parts();
    let review = parts.iter().find(|p| p.id == review_id).unwrap();
    let other = parts.iter().find(|p| p.id == other_id).unwrap();
    assert_eq!(
        fs::read_to_string(path.join(&review.path)).unwrap(),
        "first pass",
        "undo must restore only the addressed part"
    );
    assert_eq!(
        fs::read_to_string(path.join(&other.path)).unwrap(),
        "unrelated evidence",
        "undo of one part must never touch another part's content"
    );
}

#[test]
fn ticket_3d952036_legacy_ticket_with_no_parts_table_still_updatable() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Legacy ticket"),
            Some("open"),
            Default::default(),
            None,
            Some("Legacy objective"),
        )
        .unwrap();

    // A legacy ticket carries no `[[parts]]` table.
    let manifest = store.get(&id).unwrap();
    assert!(manifest.parts().is_empty());

    // It must still be updatable through the plain field/description path.
    store
        .update_with_options(
            &id,
            BTreeMap::new(),
            None,
            None,
            Some("Updated legacy objective"),
            Some(DescriptionUpdateMode::Replace),
            None,
            false,
        )
        .unwrap();
    let path = store.get_indexed(&id).unwrap().unwrap().path;
    assert_eq!(
        crate::storage::ticket_fs::TicketFs::read_description(&path).as_deref(),
        Some("Updated legacy objective")
    );

    // And it must still be part-addressable via its implicit objective id.
    let report = crate::storage::ticket_fs::TicketFs::load_parts(
        &path,
        &store.get(&id).unwrap(),
    )
    .unwrap();
    assert_eq!(report.parts.len(), 1);
    let implicit_id = report.parts[0].id;
    store
        .write_part(&id, implicit_id, "objective", "Via part write", None)
        .unwrap();
    assert_eq!(
        crate::storage::ticket_fs::TicketFs::read_description(&path).as_deref(),
        Some("Via part write"),
        "writing the implicit objective part must go through description.md"
    );
}

// ── ticket f9e70385: plan freezing at `planned` (spec 24b3d22b) ─────────────

const PLANNING_KINDS: &[&str] = &[
    "objective",
    "requirements",
    "design",
    "examples",
    "acceptance_criteria",
];

#[test]
fn f9e70385_planned_freezes_exactly_the_five_planning_parts() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Plan freeze"),
            Some("open"),
            Default::default(),
            None,
            Some("Legacy objective content"),
        )
        .unwrap();

    // Seed a non-planning, non-frozen kind before freezing, to prove it is
    // left alone.
    let notes_id = Uuid::new_v4();
    store
        .write_part(&id, notes_id, "notes", "working notes", None)
        .unwrap();

    store
        .update(&id, BTreeMap::new(), None, Some("planned"), None, None)
        .unwrap();

    let manifest = store.get(&id).unwrap();
    let parts = manifest.parts();

    // AC1: exactly the five planning parts are frozen, and no others.
    for &kind in PLANNING_KINDS {
        let part = parts
            .iter()
            .find(|p| p.kind == kind)
            .unwrap_or_else(|| panic!("planning part '{kind}' materialized"));
        assert!(part.frozen, "planning part '{kind}' must be frozen");
    }
    let notes = parts.iter().find(|p| p.id == notes_id).unwrap();
    assert!(!notes.frozen, "notes part must never be frozen");
    let frozen_count = parts.iter().filter(|p| p.frozen).count();
    assert_eq!(
        frozen_count, 5,
        "exactly the five planning parts should be frozen, got: {parts:?}"
    );
}

#[test]
fn f9e70385_write_to_frozen_part_is_rejected_and_file_byte_identical() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Plan freeze"),
            Some("open"),
            Default::default(),
            None,
            Some("Stable objective"),
        )
        .unwrap();

    store
        .update(&id, BTreeMap::new(), None, Some("planned"), None, None)
        .unwrap();

    let path = store.get_indexed(&id).unwrap().unwrap().path;
    let manifest = store.get(&id).unwrap();
    let objective = manifest
        .parts()
        .into_iter()
        .find(|p| p.kind == "objective")
        .unwrap();
    let before = fs::read(path.join(&objective.path)).unwrap();

    // AC2: the write is hard-rejected.
    let err = store
        .write_part(&id, objective.id, "objective", "sneaky overwrite", None)
        .unwrap_err();

    let after = fs::read(path.join(&objective.path)).unwrap();
    assert_eq!(
        before, after,
        "the frozen part file must be byte-identical after a rejected write"
    );

    // AC3: the error names the part (kind + id), the freezing state, and
    // both recovery paths (amendment w/ supersedes; transition back to a
    // pre-planned state).
    let message = err.to_string();
    assert!(message.contains("objective"), "must name the kind: {message}");
    assert!(
        message.contains(&objective.id.to_string()),
        "must name the part id: {message}"
    );
    assert!(message.contains("planned"), "must name the freezing state: {message}");
    assert!(
        message.contains("amendment") && message.contains("supersedes"),
        "must name the amendment recovery path: {message}"
    );
    assert!(
        message.contains("transition"),
        "must name the state-transition recovery path: {message}"
    );
}

#[test]
fn f9e70385_review_write_on_planned_ticket_succeeds() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Plan freeze"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    store
        .update(&id, BTreeMap::new(), None, Some("planned"), None, None)
        .unwrap();

    // AC4: a write to `review` on a `planned` ticket succeeds.
    let review_id = Uuid::new_v4();
    store
        .write_part(&id, review_id, "review", "Reviewed, looks good.", None)
        .unwrap();

    let path = store.get_indexed(&id).unwrap().unwrap().path;
    let manifest = store.get(&id).unwrap();
    let review = manifest.parts().into_iter().find(|p| p.id == review_id).unwrap();
    assert!(!review.frozen);
    assert_eq!(
        fs::read_to_string(path.join(&review.path)).unwrap(),
        "Reviewed, looks good."
    );
}

#[test]
fn f9e70385_unfreeze_refreeze_cycle_appends_plan_revision() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Plan freeze"),
            Some("open"),
            Default::default(),
            None,
            Some("Objective v1"),
        )
        .unwrap();

    store
        .update(&id, BTreeMap::new(), None, Some("planned"), None, None)
        .unwrap();
    let manifest = store.get(&id).unwrap();
    assert!(manifest.parts().iter().all(|p| p.frozen == (PLANNING_KINDS.contains(&p.kind.as_str()))));
    let revision_1 = manifest
        .extra
        .get("plan_revision")
        .and_then(Value::as_u64)
        .unwrap();
    assert_eq!(revision_1, 1);

    // AC5: transitioning back to a pre-`planned` state clears every frozen
    // flag.
    store
        .update(&id, BTreeMap::new(), None, Some("open"), None, None)
        .unwrap();
    let manifest = store.get(&id).unwrap();
    assert!(
        manifest.parts().iter().all(|p| !p.frozen),
        "every frozen flag must be cleared after unfreezing"
    );

    // The now-unfrozen objective part must accept a direct write.
    let objective = manifest
        .parts()
        .into_iter()
        .find(|p| p.kind == "objective")
        .unwrap();
    store
        .write_part(&id, objective.id, "objective", "Objective v2", None)
        .unwrap();

    // Re-entering `planned` re-freezes and appends a plan revision.
    store
        .update(&id, BTreeMap::new(), None, Some("planned"), None, None)
        .unwrap();
    let manifest = store.get(&id).unwrap();
    assert!(manifest.parts().iter().all(|p| p.frozen == (PLANNING_KINDS.contains(&p.kind.as_str()))));
    let revision_2 = manifest
        .extra
        .get("plan_revision")
        .and_then(Value::as_u64)
        .unwrap();
    assert_eq!(revision_2, 2, "re-entering planned must append a plan revision");
}

#[test]
fn f9e70385_amendment_records_supersedes_and_is_retrievable_alongside_frozen_part() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Plan freeze"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let requirements_id = Uuid::new_v4();
    store
        .write_part(&id, requirements_id, "requirements", "Original requirements", None)
        .unwrap();

    store
        .update(&id, BTreeMap::new(), None, Some("planned"), None, None)
        .unwrap();

    let manifest = store.get(&id).unwrap();
    let requirements = manifest
        .parts()
        .into_iter()
        .find(|p| p.kind == "requirements")
        .unwrap();
    assert!(requirements.frozen);

    // AC6: an `amendment` part records `supersedes` and is retrievable
    // alongside the part it supersedes.
    let amendment_id = Uuid::new_v4();
    store
        .write_amendment_part(
            &id,
            amendment_id,
            "Corrected requirement text",
            requirements.id,
            None,
        )
        .unwrap();

    let path = store.get_indexed(&id).unwrap().unwrap().path;
    let manifest = store.get(&id).unwrap();
    let report =
        crate::storage::ticket_fs::TicketFs::load_parts(&path, &manifest)
            .unwrap();
    let amendment = report
        .parts
        .iter()
        .find(|p| p.id == amendment_id)
        .expect("amendment part retrievable");
    assert_eq!(amendment.kind, "amendment");
    assert_eq!(amendment.supersedes, Some(requirements.id));
    assert_eq!(amendment.content, "Corrected requirement text");

    // The superseded part remains present and byte-identical.
    let still_frozen = report
        .parts
        .iter()
        .find(|p| p.id == requirements.id)
        .unwrap();
    assert!(still_frozen.frozen);
    assert_eq!(still_frozen.content, "Original requirements");
}

#[test]
fn f9e70385_undo_part_on_frozen_part_is_also_rejected() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Plan freeze"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    // Write `design` while still unfrozen so there is history to undo.
    let design_id = Uuid::new_v4();
    store
        .write_part(&id, design_id, "design", "design v1", None)
        .unwrap();
    store
        .write_part(&id, design_id, "design", "design v2", None)
        .unwrap();

    store
        .update(&id, BTreeMap::new(), None, Some("planned"), None, None)
        .unwrap();

    // AC7: undo is a write like any other and must not bypass the gate.
    let err = store.undo_part(&id, design_id, None).unwrap_err();
    assert!(matches!(err, crate::error::StorageError::FrozenPartWrite { .. }));

    let path = store.get_indexed(&id).unwrap().unwrap().path;
    let manifest = store.get(&id).unwrap();
    let design = manifest.parts().into_iter().find(|p| p.id == design_id).unwrap();
    assert_eq!(
        fs::read_to_string(path.join(&design.path)).unwrap(),
        "design v2",
        "a rejected undo must leave the frozen part untouched"
    );
}

#[test]
fn f9e70385_legacy_description_write_rejected_when_objective_frozen() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Plan freeze"),
            Some("open"),
            Default::default(),
            None,
            Some("Frozen objective content"),
        )
        .unwrap();

    store
        .update(&id, BTreeMap::new(), None, Some("planned"), None, None)
        .unwrap();

    let path = store.get_indexed(&id).unwrap().unwrap().path;
    let manifest = store.get(&id).unwrap();
    let objective = manifest
        .parts()
        .into_iter()
        .find(|p| p.kind == "objective")
        .unwrap();
    assert!(objective.frozen);
    let before_part = fs::read(path.join(&objective.path)).unwrap();
    let before_description = fs::read(path.join("description.md")).unwrap();

    // AC7 (review-reproduced bypass): the legacy `description`/
    // `description_mode` write path in `update_with_options` must reject
    // exactly like a part-addressed write when the ticket's `objective`
    // part is frozen — it is not an alternate, ungated entry point.
    let err = store
        .update_with_options(
            &id,
            BTreeMap::new(),
            None,
            None,
            Some("orphaned legacy write"),
            Some(DescriptionUpdateMode::Replace),
            None,
            false,
        )
        .unwrap_err();
    assert!(matches!(err, crate::error::StorageError::FrozenPartWrite { .. }));

    let after_part = fs::read(path.join(&objective.path)).unwrap();
    let after_description = fs::read(path.join("description.md")).unwrap();
    assert_eq!(
        before_part, after_part,
        "a rejected legacy write must leave the frozen objective part file untouched"
    );
    assert_eq!(
        before_description, after_description,
        "a rejected legacy write must leave description.md untouched"
    );
}

#[test]
fn bc74e91f_combined_freeze_and_description_write_materializes_matching_objective() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Combined freeze + description write"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    let written = "combined write content";

    // AC1/AC2: a single call that both transitions to `planned` and writes
    // the description must materialize the `objective` part from the new
    // description text, not from the pre-call (empty) description.
    store
        .update_with_options(
            &id,
            BTreeMap::new(),
            None,
            Some("planned"),
            Some(written),
            Some(DescriptionUpdateMode::Replace),
            None,
            false,
        )
        .unwrap();

    let path = store.get_indexed(&id).unwrap().unwrap().path;
    let manifest = store.get(&id).unwrap();
    let objective = manifest
        .parts()
        .into_iter()
        .find(|p| p.kind == "objective")
        .expect("objective part should be materialized by plan freeze");
    assert!(
        objective.frozen,
        "objective should be frozen after entering planned"
    );

    let objective_content = fs::read_to_string(path.join(&objective.path)).unwrap();
    let description_content = fs::read_to_string(path.join("description.md")).unwrap();

    assert_eq!(
        objective_content, written,
        "objective part must contain the newly written description"
    );
    assert_eq!(
        description_content, written,
        "description.md must contain the newly written description"
    );
    assert_eq!(
        objective_content, description_content,
        "objective part and description.md must be byte-identical"
    );

    let indexed = store.get_indexed(&id).unwrap().unwrap();
    assert_eq!(indexed.state.as_deref(), Some("planned"));
}


