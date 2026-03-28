use std::collections::BTreeMap;

use crate::model::edge::EdgeKindRule;
use crate::model::schema::{FieldSchema, FieldType, Transition, TicketTypeSchema};

pub const TYPE_ID: &str = "tracker-improvement";

/// Returns the hardcoded built-in `tracker-improvement` ticket type schema.
///
/// This is the only supported type in Phase 1. Additional types and a full
/// schema engine are deferred to post-dogfooding.
pub fn tracker_improvement_schema() -> TicketTypeSchema {
    let fields: BTreeMap<String, FieldSchema> = [
        ("title", FieldSchema { field_type: FieldType::String, required: true }),
        ("type", FieldSchema { field_type: FieldType::String, required: true }),
        ("state", FieldSchema { field_type: FieldType::String, required: false }),
        ("component", FieldSchema { field_type: FieldType::String, required: false }),
        ("risk_level", FieldSchema { field_type: FieldType::String, required: false }),
        ("acceptance_criteria", FieldSchema { field_type: FieldType::String, required: false }),
        ("validation_plan", FieldSchema { field_type: FieldType::String, required: false }),
        ("validation_status", FieldSchema { field_type: FieldType::String, required: false }),
        ("validator_id", FieldSchema { field_type: FieldType::String, required: false }),
        ("release_target", FieldSchema { field_type: FieldType::String, required: false }),
        ("release_version", FieldSchema { field_type: FieldType::String, required: false }),
        ("interview_file_type", FieldSchema { field_type: FieldType::String, required: false }),
        ("interview_files", FieldSchema { field_type: FieldType::Json, required: false }),
        ("bootstrap_blocker", FieldSchema { field_type: FieldType::Boolean, required: false }),
        ("rollout_stage", FieldSchema { field_type: FieldType::String, required: false }),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect();

    let states = vec![
        "new",
        "in-refinement",
        "ready",
        "in-implementation",
        "in-review",
        "in-validation",
        "done",
        "cancelled",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();

    let transitions = vec![
        // new ->
        ("new", "in-refinement"),
        ("new", "cancelled"),
        // in-refinement ->
        ("in-refinement", "ready"),
        ("in-refinement", "new"),
        ("in-refinement", "cancelled"),
        // ready ->
        ("ready", "in-implementation"),
        ("ready", "in-refinement"),
        ("ready", "cancelled"),
        // in-implementation ->
        ("in-implementation", "in-review"),
        ("in-implementation", "cancelled"),
        // in-review ->
        ("in-review", "in-validation"),
        ("in-review", "in-implementation"),
        ("in-review", "cancelled"),
        // in-validation ->
        ("in-validation", "done"),
        ("in-validation", "in-review"),
        ("in-validation", "cancelled"),
    ]
    .into_iter()
    .map(|(f, t)| Transition { from: f.to_string(), to: t.to_string() })
    .collect();

    let mut edge_rules: BTreeMap<String, EdgeKindRule> = BTreeMap::new();
    edge_rules.insert(
        "depends_on".to_string(),
        EdgeKindRule { directed: true, acyclic_enforced: true },
    );
    edge_rules.insert(
        "linked".to_string(),
        EdgeKindRule { directed: false, acyclic_enforced: false },
    );

    TicketTypeSchema {
        type_id: TYPE_ID.to_string(),
        fields,
        states,
        transitions,
        edge_rules,
        required_states: vec!["in-review".to_string()],
        terminal_states: vec!["done".to_string()],
    }
}

/// Returns `true` if the given type ID is a known built-in type.
pub fn is_builtin_type(type_id: &str) -> bool {
    type_id == TYPE_ID
}

/// Resolve a type schema by type ID. Returns `None` for unknown types.
pub fn schema_for_type(type_id: &str) -> Option<TicketTypeSchema> {
    match type_id {
        TYPE_ID => Some(tracker_improvement_schema()),
        _ => None,
    }
}
