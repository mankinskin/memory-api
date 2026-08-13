//! Tests for read projections (`view` profiles and explicit `parts` lists),
//! spec 24b3d22b, ticket 4c7b884e.

use std::collections::BTreeSet;

use tempfile::tempdir;
use uuid::Uuid;

use crate::{
    model::parts::{
        CORE_PART_KINDS,
        ViewProfile,
    },
    model::ticket::TicketManifestExt,
    storage::{
        ReadProjection,
        TicketStore,
    },
};

/// Populate a ticket with one part of every core kind plus one free-form
/// attachment kind, returning `(tempdir, store, id)`. The `TempDir` guard
/// must stay alive for the store's lifetime, or its backing directory is
/// deleted while the store still holds paths into it.
fn fixture_ticket_with_all_kinds() -> (tempfile::TempDir, TicketStore, Uuid) {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Projection fixture"),
            Some("open"),
            Default::default(),
            None,
            None,
        )
        .unwrap();

    for &kind in CORE_PART_KINDS {
        if kind == "amendment" {
            continue;
        }
        store
            .write_part(&id, Uuid::new_v4(), kind, &format!("{kind} content"), None)
            .unwrap();
    }
    // The requirements part is the amendment's supersedes target so
    // "amendment" (the 9th core kind) is present too.
    let manifest = store.get(&id).unwrap();
    let requirements_id = manifest
        .parts()
        .into_iter()
        .find(|p| p.kind == "requirements")
        .unwrap()
        .id;
    store
        .write_amendment_part(
            &id,
            Uuid::new_v4(),
            "amendment content",
            requirements_id,
            None,
        )
        .unwrap();

    // One free-form attachment kind.
    store
        .write_part(&id, Uuid::new_v4(), "handoff_notes", "attachment content", None)
        .unwrap();

    (dir, store, id)
}

fn top_level_kinds(projection: &crate::storage::TicketProjection) -> BTreeSet<String> {
    projection.parts.iter().map(|p| p.kind.clone()).collect()
}

#[test]
fn summary_profile_returns_only_objective() {
    let (_dir, store, id) = fixture_ticket_with_all_kinds();
    let projected = store
        .project(&id, &ReadProjection::Profile(ViewProfile::Summary))
        .unwrap();
    assert_eq!(top_level_kinds(&projected), BTreeSet::from(["objective".to_string()]));
    assert!(projected.refs.is_none(), "summary must not include refs");
}

#[test]
fn plan_profile_returns_exactly_the_five_planning_kinds_plus_refs() {
    let (_dir, store, id) = fixture_ticket_with_all_kinds();
    let projected = store
        .project(&id, &ReadProjection::Profile(ViewProfile::Plan))
        .unwrap();
    let expected: BTreeSet<String> = [
        "objective",
        "requirements",
        "design",
        "examples",
        "acceptance_criteria",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    assert_eq!(top_level_kinds(&projected), expected);
    assert!(projected.refs.is_some(), "plan must include refs");

    // The amendment is inlined under requirements, not a separate top-level
    // entry (design step 2).
    let requirements = projected
        .parts
        .iter()
        .find(|p| p.kind == "requirements")
        .unwrap();
    assert_eq!(requirements.amendments.len(), 1);
    assert_eq!(requirements.amendments[0].kind, "amendment");
}

#[test]
fn review_profile_returns_exactly_acceptance_criteria_review_validation() {
    let (_dir, store, id) = fixture_ticket_with_all_kinds();
    let projected = store
        .project(&id, &ReadProjection::Profile(ViewProfile::Review))
        .unwrap();
    let expected: BTreeSet<String> = ["acceptance_criteria", "review", "validation"]
        .into_iter()
        .map(String::from)
        .collect();
    assert_eq!(top_level_kinds(&projected), expected);
    assert!(projected.refs.is_none(), "review must not include refs");
}

#[test]
fn full_profile_returns_every_kind_including_free_form() {
    let (_dir, store, id) = fixture_ticket_with_all_kinds();
    let projected = store
        .project(&id, &ReadProjection::Profile(ViewProfile::Full))
        .unwrap();
    let mut expected: BTreeSet<String> = CORE_PART_KINDS
        .iter()
        .filter(|&&k| k != "amendment") // nested under requirements, not top-level
        .map(|&k| k.to_string())
        .collect();
    expected.insert("handoff_notes".to_string());
    assert_eq!(top_level_kinds(&projected), expected);
    assert!(projected.refs.is_some(), "full must include refs");

    let requirements = projected
        .parts
        .iter()
        .find(|p| p.kind == "requirements")
        .unwrap();
    assert_eq!(requirements.amendments.len(), 1);
}

#[test]
fn explicit_parts_list_returns_exactly_those_kinds_in_deterministic_order() {
    let (_dir, store, id) = fixture_ticket_with_all_kinds();
    let projection = ReadProjection::Kinds(vec![
        "objective".to_string(),
        "acceptance_criteria".to_string(),
    ]);
    let first = store.project(&id, &projection).unwrap();
    let second = store.project(&id, &projection).unwrap();

    let first_kinds: Vec<String> =
        first.parts.iter().map(|p| p.kind.clone()).collect();
    let second_kinds: Vec<String> =
        second.parts.iter().map(|p| p.kind.clone()).collect();
    assert_eq!(first_kinds, vec!["objective".to_string(), "acceptance_criteria".to_string()]);
    assert_eq!(first_kinds, second_kinds, "explicit-list order must be deterministic");
    assert!(first.refs.is_none(), "explicit part list never auto-includes refs");
    // No amendment inlining for explicit lists.
    assert!(first.parts.iter().all(|p| p.amendments.is_empty()));
}

#[test]
fn explicit_parts_list_absent_kind_yields_empty_not_error() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("No design part"),
            Some("open"),
            Default::default(),
            None,
            Some("objective text"),
        )
        .unwrap();
    let projection = ReadProjection::Kinds(vec!["design".to_string()]);
    let projected = store.project(&id, &projection).unwrap();
    assert!(projected.parts.is_empty(), "absent core kind yields no entry, not an error");
}

#[test]
fn legacy_ticket_with_no_parts_table_projects_sanely_under_every_profile() {
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
            Some("Legacy description-only objective"),
        )
        .unwrap();

    for profile in [
        ViewProfile::Summary,
        ViewProfile::Plan,
        ViewProfile::Review,
        ViewProfile::Full,
    ] {
        let projected = store
            .project(&id, &ReadProjection::Profile(profile))
            .unwrap();
        if matches!(profile, ViewProfile::Review) {
            // Legacy ticket has no acceptance_criteria/review/validation parts.
            assert!(projected.parts.is_empty());
            continue;
        }
        assert_eq!(projected.parts.len(), 1);
        let part = &projected.parts[0];
        assert_eq!(part.kind, "objective");
        assert!(part.implicit, "legacy objective part must be marked implicit");
        assert_eq!(part.content, "Legacy description-only objective");
    }
}

#[test]
fn unknown_view_profile_is_rejected_naming_valid_vocabulary() {
    let err = ReadProjection::decode(Some("bogus"), None).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("bogus"));
    for name in crate::model::parts::VIEW_PROFILE_NAMES {
        assert!(message.contains(name), "error must name '{name}': {message}");
    }
}

#[test]
fn unknown_part_kind_in_explicit_list_is_rejected() {
    let (_dir, store, id) = fixture_ticket_with_all_kinds();
    let err = store
        .project(&id, &ReadProjection::Kinds(vec!["not_a_real_kind".to_string()]))
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("not_a_real_kind"));
    assert!(message.contains("objective"), "error should name known vocabulary: {message}");
}

#[test]
fn both_view_and_parts_is_rejected() {
    let err = ReadProjection::decode(Some("summary"), Some("objective")).unwrap_err();
    assert!(err.to_string().contains("both"));
}

#[test]
fn no_view_and_no_parts_decodes_to_none_for_default_summary() {
    assert!(ReadProjection::decode(None, None).unwrap().is_none());
}

#[test]
fn plan_profile_inlines_frozen_requirements_amendments_oldest_first_newest_last() {
    let dir = tempdir().unwrap();
    let store = TicketStore::init(dir.path()).unwrap();
    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("Amendment ordering"),
            Some("open"),
            Default::default(),
            None,
            Some("objective text"),
        )
        .unwrap();

    let requirements_id = Uuid::new_v4();
    store
        .write_part(&id, requirements_id, "requirements", "frozen requirements text", None)
        .unwrap();

    // Freeze planning parts by entering `planned`.
    store
        .update(&id, Default::default(), None, Some("planned"), None, None)
        .unwrap();

    store
        .write_amendment_part(&id, Uuid::new_v4(), "amendment one", requirements_id, None)
        .unwrap();
    store
        .write_amendment_part(&id, Uuid::new_v4(), "amendment two", requirements_id, None)
        .unwrap();

    let projected = store
        .project(&id, &ReadProjection::Profile(ViewProfile::Plan))
        .unwrap();
    let requirements = projected
        .parts
        .iter()
        .find(|p| p.kind == "requirements")
        .unwrap();
    assert!(requirements.frozen, "requirements part must be frozen");
    assert_eq!(requirements.amendments.len(), 2);
    assert_eq!(requirements.amendments[0].content, "amendment one");
    assert_eq!(requirements.amendments[1].content, "amendment two");
}
