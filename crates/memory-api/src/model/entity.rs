use std::collections::BTreeMap;

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

pub type EntityId = Uuid;

/// A structured reference from a ticket (or other entity) to a spec,
/// carried on [`EntityManifest::related_specs`].
///
/// Mirrors `spec_api::TicketRef`'s shape so validators can treat both
/// directions symmetrically. Always carries an explicit workspace/store
/// identifier so link validation never has to guess which store a
/// reference resolves against — the direct fix for the nested-store bug,
/// where a path relative to the referencing file silently resolved
/// against the wrong `.spec` store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SpecRef {
    /// Spec UUID.
    pub spec_id: Uuid,
    /// Named workspace the spec store belongs to (e.g. "default").
    pub workspace: String,
    /// Store root the spec resolves against, repo-root-relative
    /// (e.g. ".spec", "memory-api/.spec"). Never a path relative to the
    /// referencing entity file.
    pub store_root: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EntityManifest {
    pub id: EntityId,
    pub created_at: DateTime<Utc>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl EntityManifest {
    pub fn new(
        id: EntityId,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            id,
            created_at,
            extra: BTreeMap::new(),
        }
    }

    /// Structured spec links for this entity (typed field backed by the
    /// `related_specs` extra key). Returns an empty vec (never errors) when
    /// the key is absent or holds legacy untyped entries — see
    /// [`Self::legacy_spec_link_entries`] for the migration-detection path.
    pub fn related_specs(&self) -> Vec<SpecRef> {
        self.extra
            .get("related_specs")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    /// Replace the structured spec links, storing them under the
    /// `related_specs` extra key. Removes the key entirely when empty so
    /// serialized manifests stay minimal.
    pub fn set_related_specs(
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

    /// Legacy untyped spec-link entries (bare UUID or path strings) found
    /// under the `related_specs`/`spec_ids` extra keys. Used by
    /// `validate-links` and migration tooling to detect entities that still
    /// need conversion to structured [`SpecRef`] entries; never an error on
    /// its own.
    pub fn legacy_spec_link_entries(&self) -> Vec<String> {
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_manifest() -> EntityManifest {
        EntityManifest::new(Uuid::nil(), Utc::now())
    }

    #[test]
    fn spec_ref_round_trips_through_toml() {
        let spec_ref = SpecRef {
            spec_id: Uuid::nil(),
            workspace: "default".to_string(),
            store_root: ".spec".to_string(),
        };

        let toml_str = toml::to_string(&spec_ref).unwrap();
        let parsed: SpecRef = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed, spec_ref);
    }

    #[test]
    fn related_specs_round_trips_through_extra() {
        let mut manifest = make_manifest();
        let refs = vec![SpecRef {
            spec_id: Uuid::new_v4(),
            workspace: "default".to_string(),
            store_root: ".spec".to_string(),
        }];

        manifest.set_related_specs(refs.clone());
        assert_eq!(manifest.related_specs(), refs);

        // Round trip through TOML text (as would happen via manifest_format).
        let toml_str = toml::to_string(&manifest).unwrap();
        let parsed: EntityManifest = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.related_specs(), refs);
    }

    #[test]
    fn set_related_specs_empty_removes_key() {
        let mut manifest = make_manifest();
        manifest.extra.insert(
            "related_specs".to_string(),
            serde_json::json!([{
                "spec_id": Uuid::nil(),
                "workspace": "default",
                "store_root": ".spec",
            }]),
        );

        manifest.set_related_specs(Vec::new());

        assert!(manifest.extra.get("related_specs").is_none());
        assert!(manifest.related_specs().is_empty());
    }

    #[test]
    fn legacy_spec_link_entries_detects_untyped_strings() {
        let mut manifest = make_manifest();
        manifest.extra.insert(
            "related_specs".to_string(),
            serde_json::json!(["0386c4d0-0000-0000-0000-000000000000"]),
        );
        manifest.extra.insert(
            "spec_ids".to_string(),
            serde_json::json!(["../../.spec/specs/deadbeef/spec.toml"]),
        );

        let legacy = manifest.legacy_spec_link_entries();

        assert_eq!(legacy.len(), 2);
        assert!(legacy.contains(&"0386c4d0-0000-0000-0000-000000000000".to_string()));

        // Typed entries replace the legacy `related_specs` signal; removing
        // the other legacy key clears the rest.
        manifest.set_related_specs(vec![SpecRef {
            spec_id: Uuid::nil(),
            workspace: "default".to_string(),
            store_root: ".spec".to_string(),
        }]);
        manifest.extra.remove("spec_ids");
        assert!(manifest.legacy_spec_link_entries().is_empty());
    }
}

