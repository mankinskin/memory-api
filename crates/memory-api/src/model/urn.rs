//! Canonical cross-store reference model: the `ce://` URN.
//!
//! A URN names a single entity in any store of any workspace with one stable,
//! portable string:
//!
//! ```text
//! ce://<workspace>/<store>/<entity>
//! ```
//!
//! - `workspace` — workspace identifier (an opaque, non-empty slug).
//! - `store` — the store kind, mapped to [`ContentKind`] (`ticket`, `spec`, …).
//! - `entity` — the target entity UUID.
//!
//! Example: `ce://default/ticket/82d6ada4-ac35-45a7-9df6-7b7501d58e70`.
//!
//! This module owns parsing, formatting, and validation. The [`UrnResolver`]
//! trait defines the cross-store lookup interface store crates implement to
//! turn a [`Urn`] into a concrete entity.

use std::{
    fmt,
    str::FromStr,
};

use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;
use uuid::Uuid;

use crate::model::index_entry::ContentKind;

/// The fixed scheme prefix for every cross-store reference URN.
pub const URN_SCHEME: &str = "ce";

/// Errors produced when parsing or validating a [`Urn`].
#[derive(Debug, Error, PartialEq, Eq)]
pub enum UrnError {
    /// The string did not start with the `ce://` scheme.
    #[error("invalid scheme: expected '{URN_SCHEME}://', got '{0}'")]
    InvalidScheme(String),
    /// The authority/path did not have exactly three segments.
    #[error("expected 3 segments (workspace/store/entity), got {0}")]
    WrongSegmentCount(usize),
    /// The workspace segment was empty.
    #[error("workspace segment must not be empty")]
    EmptyWorkspace,
    /// The store segment did not map to a known [`ContentKind`].
    #[error("unknown store kind: '{0}'")]
    UnknownStore(String),
    /// The entity segment was not a valid UUID.
    #[error("invalid entity uuid: '{0}'")]
    InvalidEntity(String),
}

/// A parsed `ce://<workspace>/<store>/<entity>` cross-store reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Urn {
    /// Workspace identifier (non-empty).
    pub workspace: String,
    /// Store kind the entity lives in.
    pub store: ContentKind,
    /// Target entity UUID.
    pub entity: Uuid,
}

impl Urn {
    /// Construct a URN from its parts. The workspace must be non-empty.
    pub fn new(
        workspace: impl Into<String>,
        store: ContentKind,
        entity: Uuid,
    ) -> Result<Self, UrnError> {
        let workspace = workspace.into();
        if workspace.is_empty() {
            return Err(UrnError::EmptyWorkspace);
        }
        Ok(Self {
            workspace,
            store,
            entity,
        })
    }

    /// Parse a URN from its canonical string form.
    pub fn parse(input: &str) -> Result<Self, UrnError> {
        let rest = input
            .strip_prefix(&format!("{URN_SCHEME}://"))
            .ok_or_else(|| UrnError::InvalidScheme(input.to_string()))?;

        let segments: Vec<&str> = rest.split('/').collect();
        if segments.len() != 3 {
            return Err(UrnError::WrongSegmentCount(segments.len()));
        }

        let workspace = segments[0];
        if workspace.is_empty() {
            return Err(UrnError::EmptyWorkspace);
        }

        let store = store_kind_from_slug(segments[1])
            .ok_or_else(|| UrnError::UnknownStore(segments[1].to_string()))?;

        let entity = Uuid::parse_str(segments[2])
            .map_err(|_| UrnError::InvalidEntity(segments[2].to_string()))?;

        Ok(Self {
            workspace: workspace.to_string(),
            store,
            entity,
        })
    }

    /// The store kind's canonical slug used in the URN path.
    pub fn store_slug(&self) -> &'static str {
        store_kind_slug(self.store)
    }
}

impl fmt::Display for Urn {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        write!(
            f,
            "{URN_SCHEME}://{}/{}/{}",
            self.workspace,
            store_kind_slug(self.store),
            self.entity
        )
    }
}

impl FromStr for Urn {
    type Err = UrnError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Urn::parse(s)
    }
}

impl Serialize for Urn {
    fn serialize<S>(
        &self,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Urn {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Urn::parse(&s).map_err(serde::de::Error::custom)
    }
}

/// Canonical slug for a store kind in the URN path.
fn store_kind_slug(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Ticket => "ticket",
        ContentKind::Spec => "spec",
        ContentKind::Rule => "rule",
        ContentKind::Test => "test",
        ContentKind::AuditFinding => "audit_finding",
        ContentKind::WorkspaceSummary => "workspace_summary",
        ContentKind::RuleCatalog => "rule_catalog",
        ContentKind::Index => "index",
        ContentKind::AgentHook => "agent_hook",
    }
}

/// Parse a store-kind slug back to a [`ContentKind`].
fn store_kind_from_slug(slug: &str) -> Option<ContentKind> {
    Some(match slug {
        "ticket" => ContentKind::Ticket,
        "spec" => ContentKind::Spec,
        "rule" => ContentKind::Rule,
        "test" => ContentKind::Test,
        "audit_finding" => ContentKind::AuditFinding,
        "workspace_summary" => ContentKind::WorkspaceSummary,
        "rule_catalog" => ContentKind::RuleCatalog,
        "index" => ContentKind::Index,
        "agent_hook" => ContentKind::AgentHook,
        _ => return None,
    })
}

/// Cross-store lookup interface: turn a [`Urn`] into a concrete entity.
///
/// Store crates implement this so callers can resolve references that span
/// workspace and store boundaries without depending on every store type. The
/// associated `Entity` lets each implementer return its own resolved type.
pub trait UrnResolver {
    /// The resolved entity type returned on success.
    type Entity;

    /// Resolve a single URN to its entity, or `None` when the target store or
    /// entity is not present in this resolver's view.
    fn resolve(
        &self,
        urn: &Urn,
    ) -> Result<Option<Self::Entity>, UrnError>;

    /// Resolve many URNs, preserving input order.
    fn resolve_all(
        &self,
        urns: &[Urn],
    ) -> Result<Vec<Option<Self::Entity>>, UrnError> {
        urns.iter().map(|u| self.resolve(u)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_canonical_form() {
        let id = Uuid::new_v4();
        let urn = Urn::parse(&format!("ce://default/ticket/{id}")).unwrap();
        assert_eq!(urn.workspace, "default");
        assert_eq!(urn.store, ContentKind::Ticket);
        assert_eq!(urn.entity, id);
    }

    #[test]
    fn roundtrips_all_store_kinds() {
        let id = Uuid::new_v4();
        for kind in [
            ContentKind::Ticket,
            ContentKind::Spec,
            ContentKind::Rule,
            ContentKind::Test,
            ContentKind::AuditFinding,
            ContentKind::WorkspaceSummary,
            ContentKind::RuleCatalog,
            ContentKind::Index,
            ContentKind::AgentHook,
        ] {
            let urn = Urn::new("ws", kind, id).unwrap();
            let parsed = Urn::parse(&urn.to_string()).unwrap();
            assert_eq!(urn, parsed);
        }
    }

    #[test]
    fn rejects_bad_scheme() {
        let id = Uuid::new_v4();
        assert!(matches!(
            Urn::parse(&format!("http://default/ticket/{id}")),
            Err(UrnError::InvalidScheme(_))
        ));
    }

    #[test]
    fn rejects_wrong_segment_count() {
        assert!(matches!(
            Urn::parse("ce://default/ticket"),
            Err(UrnError::WrongSegmentCount(2))
        ));
        assert!(matches!(
            Urn::parse("ce://default/ticket/abc/extra"),
            Err(UrnError::WrongSegmentCount(4))
        ));
    }

    #[test]
    fn rejects_empty_workspace() {
        let id = Uuid::new_v4();
        assert_eq!(
            Urn::parse(&format!("ce:///ticket/{id}")),
            Err(UrnError::EmptyWorkspace)
        );
    }

    #[test]
    fn rejects_unknown_store() {
        let id = Uuid::new_v4();
        assert_eq!(
            Urn::parse(&format!("ce://default/bogus/{id}")),
            Err(UrnError::UnknownStore("bogus".to_string()))
        );
    }

    #[test]
    fn rejects_invalid_entity_uuid() {
        assert_eq!(
            Urn::parse("ce://default/ticket/not-a-uuid"),
            Err(UrnError::InvalidEntity("not-a-uuid".to_string()))
        );
    }

    #[test]
    fn new_rejects_empty_workspace() {
        assert_eq!(
            Urn::new("", ContentKind::Spec, Uuid::new_v4()),
            Err(UrnError::EmptyWorkspace)
        );
    }

    #[test]
    fn serde_roundtrip() {
        let urn =
            Urn::new("default", ContentKind::Spec, Uuid::new_v4()).unwrap();
        let json = serde_json::to_string(&urn).unwrap();
        let back: Urn = serde_json::from_str(&json).unwrap();
        assert_eq!(urn, back);
        assert_eq!(json, format!("\"{urn}\""));
    }
}
