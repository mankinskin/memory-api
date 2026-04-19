use spec_api::default_schema::{specification_schema, SPECIFICATION_SCHEMA_TOML};
use spec_api::spec_schema_registry;

#[test]
fn test_specification_schema_toml_parses() {
    // Verify the embedded TOML deserialises without panic.
    let schema = specification_schema();
    assert_eq!(schema.type_id, "specification");
}

#[test]
fn test_specification_schema_states() {
    let schema = specification_schema();
    let states: Vec<&str> = schema.states.iter().map(String::as_str).collect();
    assert_eq!(
        states,
        &["draft", "reviewed", "approved", "implemented", "verified", "deprecated", "cancelled"]
    );
}

#[test]
fn test_specification_schema_required_states() {
    let schema = specification_schema();
    assert!(
        schema.required_states.contains(&"reviewed".to_string()),
        "required_states must include 'reviewed'"
    );
    assert!(
        schema.required_states.contains(&"approved".to_string()),
        "required_states must include 'approved'"
    );
    // required_states must be a subset of non-terminal states
    let terminal = &schema.terminal_states;
    for rs in &schema.required_states {
        assert!(
            !terminal.contains(rs),
            "required_state '{}' must not be a terminal state",
            rs
        );
    }
}

#[test]
fn test_specification_schema_terminal_states() {
    let schema = specification_schema();
    assert_eq!(
        schema.terminal_states,
        vec!["verified".to_string()],
        "terminal_states must be [verified]"
    );
}

#[test]
fn test_specification_schema_transitions_valid() {
    let schema = specification_schema();
    let state_set: std::collections::HashSet<&str> =
        schema.states.iter().map(String::as_str).collect();

    for t in &schema.transitions {
        assert!(
            state_set.contains(t.from.as_str()),
            "transition 'from' state '{}' not in states list",
            t.from
        );
        assert!(
            state_set.contains(t.to.as_str()),
            "transition 'to' state '{}' not in states list",
            t.to
        );
    }
}

#[test]
fn test_specification_schema_edge_rules() {
    let schema = specification_schema();
    assert!(
        schema.edge_rules.contains_key("depends_on"),
        "edge_rules must contain 'depends_on'"
    );
    assert!(
        schema.edge_rules.contains_key("linked"),
        "edge_rules must contain 'linked'"
    );
    assert!(
        schema.edge_rules.contains_key("parent_of"),
        "edge_rules must contain 'parent_of' (spec-specific hierarchical relationship)"
    );

    let parent_of = &schema.edge_rules["parent_of"];
    assert!(parent_of.directed, "parent_of must be directed");
    assert!(parent_of.acyclic_enforced, "parent_of must enforce acyclicity");
}

#[test]
fn test_spec_schema_registry_contains_specification() {
    let registry = spec_schema_registry();
    assert!(
        registry.get("specification").is_some(),
        "spec_schema_registry must contain the 'specification' schema"
    );
}

#[test]
fn test_specification_schema_required_fields() {
    let schema = specification_schema();

    for required_field in &["title", "slug", "type"] {
        let field = schema.fields.get(*required_field).unwrap_or_else(|| {
            panic!("field '{}' must be defined in schema", required_field)
        });
        assert!(
            field.required,
            "field '{}' must be marked required",
            required_field
        );
    }
}

#[test]
fn test_specification_schema_workflow_enforcement() {
    let schema = specification_schema();

    // Reaching "verified" without history should fail required_states check.
    let empty_history: Vec<String> = vec![];
    let result = schema.validate_workflow("verified", &empty_history);
    assert!(
        result.is_err(),
        "validate_workflow should reject 'verified' with no history"
    );

    // With reviewed + approved in history, it should pass.
    let full_history = vec!["reviewed".to_string(), "approved".to_string()];
    let result = schema.validate_workflow("verified", &full_history);
    assert!(
        result.is_ok(),
        "validate_workflow should accept 'verified' after reviewed + approved"
    );
}

#[test]
fn test_specification_toml_raw_parses() {
    // Verify the raw constant itself is valid TOML for EntityTypeSchema.
    use memory_api::model::schema::EntityTypeSchema;
    let schema: EntityTypeSchema = toml::from_str(SPECIFICATION_SCHEMA_TOML)
        .expect("SPECIFICATION_SCHEMA_TOML must parse as EntityTypeSchema");
    assert_eq!(schema.type_id, "specification");
}
