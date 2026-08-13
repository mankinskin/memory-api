use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use uuid::Uuid;

pub use memory_kernel::model::entity::{
    EntityId as TicketId,
    EntityManifest as TicketManifest,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpecRef {
    pub spec_id: Uuid,
    pub workspace: String,
    pub store_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TicketPart {
    pub id: Uuid,
    pub kind: String,
    pub path: String,
    pub frozen: bool,
    pub created_at: DateTime<Utc>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<Uuid>,
}

impl TicketPart {
    pub fn new(
        kind: impl Into<String>,
        path: impl Into<String>,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind: kind.into(),
            path: path.into(),
            frozen: false,
            created_at: Utc::now(),
            supersedes: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TicketRefEntry {
    pub kind: String,
    pub urn: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

pub trait TicketManifestExt {
    fn related_specs(&self) -> Vec<SpecRef>;

    fn set_related_specs(
        &mut self,
        related_specs: Vec<SpecRef>,
    );

    fn refs(&self) -> Vec<TicketRefEntry>;

    fn set_refs(
        &mut self,
        refs: Vec<TicketRefEntry>,
    );

    fn legacy_spec_link_entries(&self) -> Vec<String>;

    fn parts(&self) -> Vec<TicketPart>;

    fn set_parts(
        &mut self,
        parts: Vec<TicketPart>,
    );
}

impl TicketManifestExt for TicketManifest {
    fn related_specs(&self) -> Vec<SpecRef> {
        self.extra
            .get("related_specs")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    fn set_related_specs(
        &mut self,
        related_specs: Vec<SpecRef>,
    ) {
        if related_specs.is_empty() {
            self.extra.remove("related_specs");
            return;
        }
        match serde_json::to_value(related_specs) {
            Ok(value) => {
                self.extra.insert("related_specs".to_string(), value);
            },
            Err(_) => {
                self.extra.remove("related_specs");
            },
        }
    }

    fn refs(&self) -> Vec<TicketRefEntry> {
        if let Some(value) = self.extra.get("refs").cloned() {
            return serde_json::from_value(value).unwrap_or_default();
        }
        self.related_specs()
            .into_iter()
            .map(|spec_ref| TicketRefEntry {
                kind: "spec".to_string(),
                urn: format!(
                    "ce://{}/spec/{}",
                    spec_ref.workspace, spec_ref.spec_id
                ),
                note: None,
            })
            .collect()
    }

    fn set_refs(
        &mut self,
        refs: Vec<TicketRefEntry>,
    ) {
        if refs.is_empty() {
            self.extra.remove("refs");
            return;
        }
        match serde_json::to_value(refs) {
            Ok(value) => {
                self.extra.insert("refs".to_string(), value);
            },
            Err(_) => {
                self.extra.remove("refs");
            },
        }
    }

    fn legacy_spec_link_entries(&self) -> Vec<String> {
        let mut entries = Vec::new();
        for key in ["related_specs", "spec_ids"] {
            if let Some(Value::Array(items)) = self.extra.get(key) {
                for item in items {
                    if let Value::String(s) = item {
                        entries.push(s.clone());
                    }
                }
            }
        }
        entries
    }

    fn parts(&self) -> Vec<TicketPart> {
        self.extra
            .get("parts")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    fn set_parts(
        &mut self,
        parts: Vec<TicketPart>,
    ) {
        if parts.is_empty() {
            self.extra.remove("parts");
            return;
        }
        match serde_json::to_value(parts) {
            Ok(value) => {
                self.extra.insert("parts".to_string(), value);
            },
            Err(_) => {
                self.extra.remove("parts");
            },
        }
    }
}
