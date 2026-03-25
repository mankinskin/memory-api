use std::collections::BTreeMap;

use chrono::Utc;
use ticket_api::model::edge::EdgeKindRule;
use ticket_api::model::schema::{FieldSchema, FieldType, TicketTypeSchema, Transition};
use ticket_api::model::ticket::TicketManifest;
use serde_json::json;
use uuid::Uuid;

fn make_schema() -> TicketTypeSchema {
    let mut fields = BTreeMap::new();
    fields.insert(
        "title".to_string(),
        FieldSchema {
            field_type: FieldType::String,
            required: true,
        },
    );

    let mut edge_rules = BTreeMap::new();
    edge_rules.insert(
        "depends_on".to_string(),
        EdgeKindRule {
            directed: true,
            acyclic_enforced: true,
        },
    );

    TicketTypeSchema {
        type_id: "feature".to_string(),
        fields,
        states: vec!["open".to_string(), "in_progress".to_string(), "done".to_string()],
        transitions: vec![
            Transition {
                from: "open".to_string(),
                to: "in_progress".to_string(),
            },
            Transition {
                from: "in_progress".to_string(),
                to: "done".to_string(),
            },
        ],
        edge_rules,
    }
}

#[test]
fn required_fields_must_be_present() {
    let schema = make_schema();
    let manifest = TicketManifest::new(Uuid::new_v4(), Utc::now());

    let err = schema
        .validate_manifest(&manifest)
        .expect_err("title is required");
    assert!(err.to_string().contains("required field missing"));
}

#[test]
fn transition_and_edge_kind_validation() {
    let schema = make_schema();

    schema
        .ensure_transition("open", "in_progress")
        .expect("valid transition");
    schema.ensure_edge_kind("depends_on").expect("valid edge kind");

    assert!(schema.ensure_transition("open", "done").is_err());
    assert!(schema.ensure_edge_kind("relates_to").is_err());
}

#[test]
fn manifest_with_required_fields_passes() {
    let schema = make_schema();
    let mut manifest = TicketManifest::new(Uuid::new_v4(), Utc::now());
    manifest.extra.insert("title".to_string(), json!("Implement parser"));

    schema
        .validate_manifest(&manifest)
        .expect("valid manifest passes");
}
