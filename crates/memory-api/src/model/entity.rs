use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub type EntityId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntityManifest {
    pub id: EntityId,
    pub created_at: DateTime<Utc>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl EntityManifest {
    pub fn new(id: EntityId, created_at: DateTime<Utc>) -> Self {
        Self {
            id,
            created_at,
            extra: BTreeMap::new(),
        }
    }
}
