use memory_api::model::schema::EntityTypeSchema;
use memory_api::model::schema_registry::SchemaRegistry;

/// The raw TOML content of the built-in `specification` schema.
pub const SPECIFICATION_SCHEMA_TOML: &str =
    include_str!("../schemas/specification.toml");

/// Parse and return the built-in `specification` entity type schema.
///
/// Panics if the embedded TOML is malformed — this is a compile-time invariant
/// verified by the schema parse test in this crate.
pub fn specification_schema() -> EntityTypeSchema {
    toml::from_str(SPECIFICATION_SCHEMA_TOML)
        .expect("built-in specification.toml is valid")
}

/// Create a [`SchemaRegistry`] pre-loaded with the built-in `specification` schema.
pub fn spec_schema_registry() -> SchemaRegistry {
    let mut registry = SchemaRegistry::new();
    registry.register(specification_schema());
    registry
}
