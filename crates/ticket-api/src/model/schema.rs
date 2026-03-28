use std::collections::{BTreeMap, VecDeque};

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
    /// States that must appear in a ticket's history before it can transition
    /// to a terminal state (e.g. `done`). Empty means no mandatory waypoints.
    #[serde(default)]
    pub required_states: Vec<String>,
    /// Terminal states that trigger the required_states check (default: `["done"]`).
    #[serde(default = "default_terminal_states")]
    pub terminal_states: Vec<String>,
}

fn default_terminal_states() -> Vec<String> {
    vec!["done".to_string()]
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

    /// Check that all `required_states` have been visited in `history_states`
    /// before allowing a transition to `target`. Only enforced when `target` is
    /// a terminal state.
    pub fn validate_workflow(
        &self,
        target: &str,
        history_states: &[String],
    ) -> Result<(), SchemaValidationError> {
        if self.required_states.is_empty() || !self.terminal_states.contains(&target.to_string()) {
            return Ok(());
        }
        let visited: std::collections::HashSet<&str> =
            history_states.iter().map(|s| s.as_str()).collect();
        let missing: Vec<String> = self
            .required_states
            .iter()
            .filter(|s| !visited.contains(s.as_str()))
            .cloned()
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(SchemaValidationError::RequiredStatesNotVisited {
                target: target.to_string(),
                missing,
            })
        }
    }

    /// Find the shortest path of intermediate states from `from` to `to` using BFS.
    /// Returns the sequence of states to transition through (excluding `from`, including `to`).
    /// Returns `None` if no path exists.
    pub fn find_path(&self, from: &str, to: &str) -> Option<Vec<String>> {
        if from == to {
            return Some(vec![]);
        }
        if self.allows_transition(from, to) {
            return Some(vec![to.to_string()]);
        }

        let mut visited = std::collections::HashSet::new();
        visited.insert(from.to_string());
        let mut queue = VecDeque::new();
        queue.push_back((from.to_string(), vec![]));

        while let Some((current, path)) = queue.pop_front() {
            for t in &self.transitions {
                if t.from == current && !visited.contains(&t.to) {
                    let mut new_path = path.clone();
                    new_path.push(t.to.clone());
                    if t.to == to {
                        return Some(new_path);
                    }
                    visited.insert(t.to.clone());
                    queue.push_back((t.to.clone(), new_path));
                }
            }
        }

        None
    }
}
