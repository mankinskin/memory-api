use super::*;
use crate::{
    error::{
        SchemaValidationError,
        StorageError,
    },
    storage::ticket_fs::TicketFs,
};

/// Ticket 5a3d152c AC2, wired through the real store write API: a
/// near-miss core-kind typo must be rejected by
/// [`TicketStore::write_part`] itself, not just by the standalone
/// [`crate::model::parts::classify_part_kind`] classifier.
#[test]
fn ticket_5a3d152c_misspelled_core_kind_rejected_through_store_write_part() {
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

    let part_id = Uuid::new_v4();
    let err = store
        .write_part(&id, part_id, "objectve", "content", None)
        .unwrap_err();

    match err {
        StorageError::Validation(SchemaValidationError::InvalidCoreKind {
            kind,
            valid_kinds,
        }) => {
            assert_eq!(kind, "objectve");
            assert!(valid_kinds.contains(&"objective".to_string()));
        },
        other => panic!("expected InvalidCoreKind, got {other:?}"),
    }

    // The rejected write must not have created a part file or manifest
    // entry.
    let manifest = TicketFs::read(&canonical_existing_path(
        &store.get_indexed(&id).unwrap().unwrap().path,
    ))
    .unwrap();
    assert!(
        manifest.parts().iter().all(|p| p.id != part_id),
        "a rejected write must not persist a manifest entry"
    );
}

/// A well-formed core kind still writes successfully through the same
/// entry point.
#[test]
fn ticket_5a3d152c_valid_core_kind_still_writes_through_store_write_part() {
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

    let part_id = Uuid::new_v4();
    let manifest = store
        .write_part(&id, part_id, "review", "looks good", None)
        .unwrap();

    assert!(
        manifest
            .parts()
            .iter()
            .any(|p| p.id == part_id && p.kind == "review")
    );
}
