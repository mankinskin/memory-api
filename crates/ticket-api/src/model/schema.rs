use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::SchemaValidationError;

use super::edge::EdgeKindRule;
use super::ticket::TicketManifest;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FieldType {
    String,
    Integer,
    Float,
    Boolean,
    DateTime,
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldSchema {
    pub field_type: FieldType,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transition {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TicketTypeSchema {
    pub type_id: String,
    pub fields: BTreeMap<String, FieldSchema>,
    pub states: Vec<String>,
    pub transitions: Vec<Transition>,
    #[serde(default)]
    pub edge_rules: BTreeMap<String, EdgeKindRule>,
}

impl TicketTypeSchema {
    pub fn validate_manifest(&self, manifest: &TicketManifest) -> Result<(), SchemaValidationError> {
        for (name, def) in &self.fields {
            if def.required && !manifest.extra.contains_key(name) {
                return Err(SchemaValidationError::MissingRequiredField(name.clone()));
            }
        }
        Ok(())
    }

    pub fn allows_transition(&self, from: &str, to: &str) -> bool {
        self.transitions.iter().any(|t| t.from == from && t.to == to)
    }

    pub fn ensure_transition(&self, from: &str, to: &str) -> Result<(), SchemaValidationError> {
        if self.allows_transition(from, to) {
            Ok(())
        } else {
            Err(SchemaValidationError::InvalidTransition {
                from: from.to_owned(),
                to: to.to_owned(),
            })
        }
    }

    pub fn ensure_edge_kind(&self, kind: &str) -> Result<(), SchemaValidationError> {
        if self.edge_rules.contains_key(kind) {
            Ok(())
        } else {
            Err(SchemaValidationError::InvalidEdgeKind(kind.to_owned()))
        }
    }
}
