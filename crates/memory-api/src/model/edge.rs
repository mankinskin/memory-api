use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

use super::entity::EntityId;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeRecord {
    pub from: EntityId,
    pub to: EntityId,
    pub kind: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EdgeKindRule {
    pub directed: bool,
    pub acyclic_enforced: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct EdgeKey {
    pub from: EntityId,
    pub to: EntityId,
    pub kind: String,
}

impl EdgeRecord {
    pub fn key(&self) -> EdgeKey {
        EdgeKey {
            from: self.from,
            to: self.to,
            kind: self.kind.clone(),
        }
    }
}

#[derive(Debug, Default)]
pub struct EdgeRegistry {
    keys: BTreeSet<EdgeKey>,
}

impl EdgeRegistry {
    /// Inserts edge identity if it is not present.
    /// Returns `true` if inserted, `false` if it already existed.
    pub fn insert(&mut self, edge: &EdgeRecord) -> bool {
        self.keys.insert(edge.key())
    }

    pub fn contains(&self, edge: &EdgeRecord) -> bool {
        self.keys.contains(&edge.key())
    }
}
