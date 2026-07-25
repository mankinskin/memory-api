use crate::model::schema::TicketTypeSchema;

pub const TYPE_ID: &str = "tracker-improvement";
pub const BUG_TYPE_ID: &str = "bug";
pub const TASK_TYPE_ID: &str = "task";

/// Raw TOML sources for the built-in ticket type schemas, embedded at compile
/// time from `crates/ticket-api/schemas/`.
///
/// Schemas are **data**, not code: every built-in type is defined by a TOML file
/// and parsed generically into a [`TicketTypeSchema`]. To add a new built-in
/// type, drop a `<type>.toml` file in that directory and add it to this list —
/// do not hand-build schema structs in Rust.
const BUILTIN_SCHEMA_TOML: &[(&str, &str)] = &[
    (
        TYPE_ID,
        include_str!("../../schemas/tracker-improvement.toml"),
    ),
    (BUG_TYPE_ID, include_str!("../../schemas/bug.toml")),
    (TASK_TYPE_ID, include_str!("../../schemas/task.toml")),
];

/// Parse a single embedded built-in schema TOML source.
///
/// Panics if the embedded TOML is malformed or its `type_id` does not match the
/// expected id — both are compile-time invariants verified by the parse tests in
/// this module.
fn parse_builtin(
    expected_type_id: &str,
    toml_src: &str,
) -> TicketTypeSchema {
    let schema: TicketTypeSchema =
        toml::from_str(toml_src).unwrap_or_else(|e| {
            panic!("built-in schema '{expected_type_id}.toml' is invalid: {e}")
        });
    assert_eq!(
        schema.type_id, expected_type_id,
        "built-in schema file for '{expected_type_id}' declares a mismatched type_id"
    );
    schema
}

/// Returns all built-in ticket type schemas, parsed from their embedded TOML
/// definitions.
pub fn builtin_schemas() -> Vec<TicketTypeSchema> {
    BUILTIN_SCHEMA_TOML
        .iter()
        .map(|(type_id, toml_src)| parse_builtin(type_id, toml_src))
        .collect()
}

/// Returns the built-in `tracker-improvement` ticket type schema.
pub fn tracker_improvement_schema() -> TicketTypeSchema {
    schema_for_type(TYPE_ID).expect("tracker-improvement is a built-in type")
}

/// Returns the built-in `bug` ticket type schema.
pub fn bug_schema() -> TicketTypeSchema {
    schema_for_type(BUG_TYPE_ID).expect("bug is a built-in type")
}

/// Returns the built-in `task` ticket type schema.
pub fn task_schema() -> TicketTypeSchema {
    schema_for_type(TASK_TYPE_ID).expect("task is a built-in type")
}

/// Returns `true` if the given type ID is a known built-in type.
pub fn is_builtin_type(type_id: &str) -> bool {
    BUILTIN_SCHEMA_TOML.iter().any(|(id, _)| *id == type_id)
}

/// Resolve a built-in type schema by type ID. Returns `None` for unknown types.
pub fn schema_for_type(type_id: &str) -> Option<TicketTypeSchema> {
    BUILTIN_SCHEMA_TOML
        .iter()
        .find(|(id, _)| *id == type_id)
        .map(|(id, toml_src)| parse_builtin(id, toml_src))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_builtin_schemas_parse() {
        let schemas = builtin_schemas();
        assert_eq!(schemas.len(), BUILTIN_SCHEMA_TOML.len());
        for schema in &schemas {
            assert!(
                !schema.states.is_empty(),
                "{} schema must define states",
                schema.type_id
            );
            assert!(
                schema.fields.contains_key("title"),
                "{} schema must define a title field",
                schema.type_id
            );
        }
    }

    #[test]
    fn tracker_improvement_schema_parses_from_toml() {
        let schema = tracker_improvement_schema();
        assert_eq!(schema.type_id, TYPE_ID);
        assert!(
            schema
                .required_states
                .iter()
                .any(|state| state == "in-review")
        );
        assert!(schema.terminal_states.iter().any(|state| state == "done"));
    }

    #[test]
    fn bug_schema_uses_bug_type_id() {
        let schema = bug_schema();
        assert_eq!(schema.type_id, BUG_TYPE_ID);
        assert!(
            schema
                .required_states
                .iter()
                .any(|state| state == "in-review")
        );
    }

    #[test]
    fn task_schema_uses_task_type_id() {
        let schema = task_schema();
        assert_eq!(schema.type_id, TASK_TYPE_ID);
        assert!(
            schema
                .required_states
                .iter()
                .any(|state| state == "in-review")
        );
    }

    #[test]
    fn builtin_type_checks_cover_all_types() {
        assert!(is_builtin_type(TYPE_ID));
        assert!(is_builtin_type(BUG_TYPE_ID));
        assert!(is_builtin_type(TASK_TYPE_ID));
        assert!(!is_builtin_type("made-up-type"));
        assert!(schema_for_type(BUG_TYPE_ID).is_some());
        assert!(schema_for_type(TASK_TYPE_ID).is_some());
        assert!(schema_for_type("made-up-type").is_none());
    }
}
