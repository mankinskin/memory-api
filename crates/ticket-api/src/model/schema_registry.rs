use std::{collections::BTreeMap, path::Path};

use crate::error::StorageError;
use crate::model::default_schema::tracker_improvement_schema;
use super::schema::TicketTypeSchema;

/// Registry of ticket type schemas.
///
/// Populated from built-in defaults and/or TOML schema files loaded from a
/// directory. A file whose `type_id` matches a built-in replaces the built-in,
/// allowing full workflow customisation per test environment or project.
#[derive(Debug, Clone, Default)]
pub struct SchemaRegistry {
    schemas: BTreeMap<String, TicketTypeSchema>,
}

impl SchemaRegistry {
    /// Create a registry pre-loaded with all built-in schemas.
    ///
    /// Currently registers: `tracker-improvement`.
    pub fn with_builtins() -> Self {
        let mut reg = Self::default();
        let s = tracker_improvement_schema();
        reg.schemas.insert(s.type_id.clone(), s);
        reg
    }

    /// Load all `*.toml` schema files from `dir`, adding or replacing entries.
    ///
    /// Each file must deserialise into [`TicketTypeSchema`]. The `type_id` field
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
        let schema: TicketTypeSchema = toml::from_str(&content).map_err(|e| {
            StorageError::SchemaFileParse {
                path: path.to_path_buf(),
                reason: e.to_string(),
            }
        })?;
        self.schemas.insert(schema.type_id.clone(), schema);
        Ok(())
    }

    /// Look up a schema by ticket type ID.
    pub fn get(&self, type_id: &str) -> Option<&TicketTypeSchema> {
        self.schemas.get(type_id)
    }

    /// Returns an iterator over all registered type IDs.
    pub fn type_ids(&self) -> impl Iterator<Item = &str> {
        self.schemas.keys().map(String::as_str)
    }
}
