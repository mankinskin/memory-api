use chrono::Utc;
use ticket_api::model::edge::{EdgeRecord, EdgeRegistry};
use uuid::Uuid;

#[test]
fn edge_registry_is_idempotent_for_same_identity() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    let edge = EdgeRecord {
        from: a,
        to: b,
        kind: "depends_on".to_string(),
        created_at: Utc::now(),
    };

    let mut registry = EdgeRegistry::default();

    assert!(registry.insert(&edge));
    assert!(registry.contains(&edge));
    assert!(!registry.insert(&edge));
}

#[test]
fn edge_registry_treats_kind_as_part_of_uniqueness_key() {
    let a = Uuid::new_v4();
    let b = Uuid::new_v4();

    let edge_depends = EdgeRecord {
        from: a,
        to: b,
        kind: "depends_on".to_string(),
        created_at: Utc::now(),
    };

    let edge_related = EdgeRecord {
        from: a,
        to: b,
        kind: "related".to_string(),
        created_at: Utc::now(),
    };

    let mut registry = EdgeRegistry::default();

    assert!(registry.insert(&edge_depends));
    assert!(registry.insert(&edge_related));
}
