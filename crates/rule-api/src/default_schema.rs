use memory_api::model::schema::EntityTypeSchema;
use memory_api::model::schema_registry::SchemaRegistry;

pub const RULE_ENTRY_SCHEMA_TOML: &str = include_str!("../schemas/rule-entry.toml");

pub fn rule_entry_schema() -> EntityTypeSchema {
    toml::from_str(RULE_ENTRY_SCHEMA_TOML).expect("built-in rule-entry.toml is valid")
}

pub fn rule_schema_registry() -> SchemaRegistry {
    let mut registry = SchemaRegistry::new();
    registry.register(rule_entry_schema());
    registry
}