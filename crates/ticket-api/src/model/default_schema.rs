use crate::model::schema::TicketTypeSchema;

/// Raw TOML sources for the ticket type schemas delivered with ticket-api,
/// embedded at compile time from `crates/ticket-api/schemas/`.
///
/// Schemas are **data**, not code. ticket-api makes no assumptions about which
/// ticket types exist, which fields they carry, or which type is "default" —
/// every type is whatever the shipped TOML files declare, and the `type_id` is
/// read from inside each file. To ship an additional type, drop a `<type>.toml`
/// file in that directory and add it to this list; deployments can add or
/// override types at runtime with `SchemaRegistry::load_dir`.
const DELIVERED_SCHEMA_TOML: &[&str] = &[
    include_str!("../../schemas/tracker-improvement.toml"),
    include_str!("../../schemas/bug.toml"),
    include_str!("../../schemas/task.toml"),
    include_str!("../../schemas/epic.toml"),
    include_str!("../../schemas/feature.toml"),
];

/// Parse every ticket type schema delivered with ticket-api from its embedded
/// TOML definition.
///
/// Panics if any embedded schema is malformed or omits a `type_id` — a
/// compile-time invariant verified by the parse test in this module.
pub fn builtin_schemas() -> Vec<TicketTypeSchema> {
    DELIVERED_SCHEMA_TOML
        .iter()
        .map(|toml_src| {
            let schema: TicketTypeSchema = toml::from_str(toml_src)
                .unwrap_or_else(|e| {
                    panic!("delivered ticket schema is invalid TOML: {e}")
                });
            assert!(
                !schema.type_id.is_empty(),
                "delivered ticket schema is missing a type_id"
            );
            schema
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_delivered_schemas_parse() {
        let schemas = builtin_schemas();
        assert_eq!(schemas.len(), DELIVERED_SCHEMA_TOML.len());
        for schema in &schemas {
            assert!(
                !schema.type_id.is_empty(),
                "schema must declare a type_id"
            );
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
    fn delivered_schema_type_ids_are_unique() {
        let schemas = builtin_schemas();
        let mut ids: Vec<&str> =
            schemas.iter().map(|s| s.type_id.as_str()).collect();
        ids.sort_unstable();
        let total = ids.len();
        ids.dedup();
        assert_eq!(
            total,
            ids.len(),
            "delivered schema type_ids must be unique"
        );
    }
}
