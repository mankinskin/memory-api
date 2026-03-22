use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub type TicketId = Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TicketManifest {
    pub id: TicketId,
    pub created_at: DateTime<Utc>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl TicketManifest {
    pub fn new(id: TicketId, created_at: DateTime<Utc>) -> Self {
        Self {
            id,
            created_at,
            extra: BTreeMap::new(),
        }
    }
}
