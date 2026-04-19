use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

pub type SpecId = Uuid;

/// A specification manifest — metadata about a spec stored in spec.toml.
///
/// Uses the same `extra: BTreeMap<String, Value>` storage pattern as
/// `EntityManifest` / `TicketManifest`. Spec-specific fields are stored in
/// the extra map and accessed via typed methods.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SpecManifest {
    pub id: SpecId,
    pub created_at: DateTime<Utc>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl SpecManifest {
    /// Create a new spec manifest with required fields.
    pub fn new(slug: &str, title: &str, component: &str) -> Self {
        let mut extra = BTreeMap::new();
        extra.insert("slug".to_string(), Value::String(slug.to_string()));
        extra.insert("title".to_string(), Value::String(title.to_string()));
        extra.insert(
            "component".to_string(),
            Value::String(component.to_string()),
        );
        extra.insert("type".to_string(), Value::String("specification".to_string()));
        extra.insert("state".to_string(), Value::String("draft".to_string()));

        Self {
            id: Uuid::new_v4(),
            created_at: Utc::now(),
            extra,
        }
    }

    // ── typed accessors ──

    pub fn id(&self) -> SpecId {
        self.id
    }

    pub fn slug(&self) -> Option<&str> {
        self.extra.get("slug").and_then(|v| v.as_str())
    }

    pub fn title(&self) -> Option<&str> {
        self.extra.get("title").and_then(|v| v.as_str())
    }

    pub fn state(&self) -> Option<&str> {
        self.extra.get("state").and_then(|v| v.as_str())
    }

    pub fn component(&self) -> Option<&str> {
        self.extra.get("component").and_then(|v| v.as_str())
    }

    pub fn scope(&self) -> Option<&str> {
        self.extra.get("scope").and_then(|v| v.as_str())
    }

    pub fn parent(&self) -> Option<&str> {
        self.extra.get("parent").and_then(|v| v.as_str())
    }

    // ── setters ──

    pub fn set_slug(&mut self, slug: &str) {
        self.extra
            .insert("slug".to_string(), Value::String(slug.to_string()));
    }

    pub fn set_title(&mut self, title: &str) {
        self.extra
            .insert("title".to_string(), Value::String(title.to_string()));
    }

    pub fn set_state(&mut self, state: &str) {
        self.extra
            .insert("state".to_string(), Value::String(state.to_string()));
    }

    pub fn set_component(&mut self, comp: &str) {
        self.extra
            .insert("component".to_string(), Value::String(comp.to_string()));
    }

    pub fn set_scope(&mut self, scope: &str) {
        self.extra
            .insert("scope".to_string(), Value::String(scope.to_string()));
    }

    pub fn set_parent(&mut self, parent: &str) {
        self.extra
            .insert("parent".to_string(), Value::String(parent.to_string()));
    }

    /// Access the underlying extra fields.
    pub fn as_entity(&self) -> &BTreeMap<String, Value> {
        &self.extra
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_spec_manifest() {
        let m = SpecManifest::new("ticket-api/store", "TicketStore", "ticket-api");
        assert_eq!(m.slug(), Some("ticket-api/store"));
        assert_eq!(m.title(), Some("TicketStore"));
        assert_eq!(m.component(), Some("ticket-api"));
        assert_eq!(m.state(), Some("draft"));
    }

    #[test]
    fn test_serde_round_trip() {
        let m = SpecManifest::new("ticket-api/store", "TicketStore", "ticket-api");
        let toml_str = toml::to_string_pretty(&m).unwrap();
        let m2: SpecManifest = toml::from_str(&toml_str).unwrap();
        assert_eq!(m2.slug(), Some("ticket-api/store"));
        assert_eq!(m2.title(), Some("TicketStore"));
        assert_eq!(m2.id(), m.id());
    }

    #[test]
    fn test_set_parent() {
        let mut m = SpecManifest::new("ticket-api/store/create", "create", "ticket-api");
        let parent_id = uuid::Uuid::new_v4().to_string();
        m.set_parent(&parent_id);
        assert_eq!(m.parent(), Some(parent_id.as_str()));
    }

    #[test]
    fn test_set_scope() {
        let mut m = SpecManifest::new("ticket-api/store", "TicketStore", "ticket-api");
        m.set_scope("public");
        assert_eq!(m.scope(), Some("public"));
    }
}
