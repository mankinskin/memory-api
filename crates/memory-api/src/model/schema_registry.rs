use std::{collections::BTreeMap, path::Path};

use crate::error::StorageError;
use super::schema::EntityTypeSchema;

/// Registry of entity type schemas.
///
/// Populated from built-in defaults and/or TOML schema files loaded from a
/// directory. A file whose `type_id` matches a built-in replaces the built-in,
/// allowing full workflow customisation per test environment or project.
#[derive(Debug, Clone, Default)]
pub struct SchemaRegistry {
    schemas: BTreeMap<String, EntityTypeSchema>,
}

impl SchemaRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a schema. Replaces any existing schema with the same `type_id`.
    pub fn register(&mut self, schema: EntityTypeSchema) {
        self.schemas.insert(schema.type_id.clone(), schema);
    }

    /// Load all `*.toml` schema files from `dir`, adding or replacing entries.
    ///
    /// Each file must deserialise into [`EntityTypeSchema`]. The `type_id` field
    /// inside the file determines the registry key (not the filename).
    pub fn load_dir(&mut self, dir: &Path) -> Result<(), StorageError> {
        for entry in std::fs::read_dir(dir)? {
            let path = entry?.path();
            if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                self.load_file(&path)?;
            }
        }
        Ok(())
    }

    /// Load a single TOML schema file into the registry.
    pub fn load_file(&mut self, path: &Path) -> Result<(), StorageError> {
        let content = std::fs::read_to_string(path)?;
        let schema: EntityTypeSchema = toml::from_str(&content).map_err(|e| {
            StorageError::SchemaFileParse {
                path: path.to_path_buf(),
                reason: e.to_string(),
            }
        })?;
        self.schemas.insert(schema.type_id.clone(), schema);
        Ok(())
    }

    /// Look up a schema by entity type ID.
    pub fn get(&self, type_id: &str) -> Option<&EntityTypeSchema> {
        self.schemas.get(type_id)
    }

    /// Returns an iterator over all registered type IDs.
    pub fn type_ids(&self) -> impl Iterator<Item = &str> {
        self.schemas.keys().map(String::as_str)
    }
}
