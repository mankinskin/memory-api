use memory_kernel::model::{
    schema::EntityTypeSchema,
    schema_registry::SchemaRegistry,
};

pub const RULE_ENTRY_SCHEMA_TOML: &str =
    include_str!("../schemas/rule-entry.toml");
pub const GENERATED_TARGET_SCHEMA_TOML: &str =
    include_str!("../schemas/generated-target.toml");

pub fn rule_entry_schema() -> EntityTypeSchema {
    toml::from_str(RULE_ENTRY_SCHEMA_TOML)
        .expect("built-in rule-entry.toml is valid")
}

pub fn generated_target_schema() -> EntityTypeSchema {
    toml::from_str(GENERATED_TARGET_SCHEMA_TOML)
        .expect("built-in generated-target.toml is valid")
}

pub fn rule_schema_registry() -> SchemaRegistry {
    let mut registry = SchemaRegistry::new();
    registry.register(rule_entry_schema());
    registry.register(generated_target_schema());
    registry
}
