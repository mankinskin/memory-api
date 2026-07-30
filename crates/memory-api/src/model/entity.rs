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

/// A single addressable content part of a ticket, stored under
/// `parts/<file>` and indexed by the `parts` extra key (rendered as
/// `[[parts]]` in `ticket.toml`). See spec 24b3d22b.
///
/// `id` is the stable addressing key, assigned once at creation and never
/// reused; manifest order is display/creation order only and is never used
/// for addressing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TicketPart {
    /// Stable opaque addressing key, assigned at creation.
    pub id: Uuid,
    /// Part kind: one of the core, schema-validated kinds or a free-form
    /// opaque attachment kind.
    pub kind: String,
    /// Path to the part's markdown file, relative to the ticket directory
    /// (e.g. `"parts/<id>.md"`).
    pub path: String,
    /// `true` once the ticket has entered `planned` and this is a planning
    /// part; frozen parts reject direct writes (enforced by a follow-up
    /// ticket, not here).
    pub frozen: bool,
    pub created_at: DateTime<Utc>,
    /// Reserved from the start: the `id` of the frozen part an `amendment`
    /// part supersedes. Unused until the freeze ticket lands.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<Uuid>,
}

impl TicketPart {
    /// Construct a new, unfrozen part with a freshly assigned `id` and the
    /// current timestamp.
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

/// A single typed external reference from a ticket to a non-ticket entity
/// (spec, test execution, log, rule, file, or commit), stored under the
/// `refs` extra key (rendered as `[[refs]]` in `ticket.toml`). See spec
/// 24b3d22b, ticket 9d69e93d.
///
/// `kind` is a plain `String`, not a closed enum: reading never fails, so a
/// foreign or future kind already present in a manifest round-trips
/// unchanged. Kind and URN-shape validation is enforced only at write time
/// (`ticket_api::model::refs`), not here.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TicketRefEntry {
    /// Reference kind (e.g. "spec", "test_execution", "log", "rule",
    /// "file", "commit"), or a foreign/unknown kind preserved as-is.
    pub kind: String,
    /// Canonical URN or path identifying the target, shape depending on
    /// `kind` (e.g. `ce://default/spec/<uuid>` for `spec`, a repo-relative
    /// path for `file`).
    pub urn: String,
    /// Optional free-text note explaining why the reference is relevant.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
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

    /// Structured typed references for this entity (typed field backed by
    /// the `refs` extra key, rendered as `[[refs]]` in `ticket.toml`).
    /// Returns an empty vec (never errors) when the key is absent and there
    /// is no legacy data to bridge.
    ///
    /// When no explicit `refs` key is present, legacy `related_specs`
    /// entries are transparently synthesized as `kind = "spec"` refs (no
    /// loss of identity) so a ticket written before this ticket landed
    /// still exposes its spec references through the typed surface. Once
    /// an explicit `refs` key exists, it is authoritative and legacy
    /// `related_specs` is not merged in (avoids duplicate entries after the
    /// first `set_refs` write).
    pub fn refs(&self) -> Vec<TicketRefEntry> {
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

    /// Replace the structured typed references, storing them under the
    /// `refs` extra key in manifest (creation/display) order. Removes the
    /// key entirely when empty so serialized manifests stay minimal and
    /// legacy (no-`[[refs]]`) tickets round-trip unchanged. Never touches
    /// `related_specs` — the old field is read for compatibility only and
    /// is never written by this path.
    pub fn set_refs(
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

    /// Structured content parts for this entity (typed field backed by the
    /// `parts` extra key, rendered as `[[parts]]` in `ticket.toml`). Returns
    /// an empty vec (never errors) when the key is absent — the case for
    /// every legacy ticket with no `[[parts]]` table.
    pub fn parts(&self) -> Vec<TicketPart> {
        self.extra
            .get("parts")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }

    /// Replace the structured content parts, storing them under the `parts`
    /// extra key in manifest (creation/display) order. Removes the key
    /// entirely when empty so serialized manifests stay minimal and legacy
    /// (no-`[[parts]]`) tickets round-trip unchanged.
    pub fn set_parts(
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

    #[test]
    fn parts_round_trip_through_extra() {
        let mut manifest = make_manifest();
        let parts = vec![
            TicketPart::new("objective", "parts/aaaaaaaa.md"),
            TicketPart::new("notes", "parts/bbbbbbbb.md"),
        ];

        manifest.set_parts(parts.clone());
        assert_eq!(manifest.parts(), parts);

        let toml_str = toml::to_string(&manifest).unwrap();
        let parsed: EntityManifest = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.parts(), parts);
    }

    #[test]
    fn parts_supersedes_none_omitted_and_round_trips() {
        let mut manifest = make_manifest();
        let original = TicketPart::new("acceptance_criteria", "parts/orig.md");
        let amendment = TicketPart {
            supersedes: Some(original.id),
            ..TicketPart::new("amendment", "parts/amend.md")
        };
        manifest.set_parts(vec![original.clone(), amendment.clone()]);

        // `supersedes: None` must not survive as an explicit null — it must
        // be indistinguishable from a legacy part that never had the field.
        let value = manifest.extra.get("parts").unwrap();
        let first = &value.as_array().unwrap()[0];
        assert!(
            first.get("supersedes").is_none(),
            "supersedes should be omitted, not null, when absent"
        );

        let toml_str = toml::to_string(&manifest).unwrap();
        let parsed: EntityManifest = toml::from_str(&toml_str).unwrap();
        let round_tripped = parsed.parts();
        assert_eq!(round_tripped[0].supersedes, None);
        assert_eq!(round_tripped[1].supersedes, Some(original.id));
    }

    #[test]
    fn set_parts_empty_removes_key() {
        let mut manifest = make_manifest();
        manifest.set_parts(vec![TicketPart::new("objective", "parts/a.md")]);
        assert!(manifest.extra.contains_key("parts"));

        manifest.set_parts(Vec::new());
        assert!(manifest.extra.get("parts").is_none());
        assert!(manifest.parts().is_empty());
    }

    #[test]
    fn parts_absent_key_yields_empty_vec_for_legacy_manifests() {
        let manifest = make_manifest();
        assert!(manifest.parts().is_empty());
    }
}

