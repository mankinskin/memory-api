//! Canonical `IndexEntry` schema for memory-api store index artifacts.
//!
//! Every generated index artifact — folder-level README indexes, workspace-folder
//! `.index.toon` files, and `.agents/` hook entries — round-trips through these
//! types. There is no dependency on context-stack crates.
//!
//! # Serialization
//!
//! TOON is the primary serialization format (`toon-format`). JSON is opt-in only.
//! Both are supported transparently because these types carry standard serde derives.
//!
//! # Digest normalization contract
//!
//! [`IndexEntry::compute_digest`] hashes a stable, ordered subset of the entry's
//! fields using SHA-256 and returns the result as a lowercase hex string.
//!
//! **Input fields** (in this fixed order, separated by `\0`):
//! 1. `id` (UUID hyphenated)
//! 2. `kind` (serde snake_case variant name)
//! 3. `source_path` (as stored, using `/` separators)
//! 4. `title`
//! 5. `summary`
//! 6. `scope` (empty string when absent)
//! 7. `non_goals` (empty string when absent)
//! 8. `keywords` sorted and joined with `,`
//! 9. `tags` sorted and joined with `,`
//!
//! Fields excluded from the digest: `digest` itself, `generated_at`,
//! `source_modified_at`, and all `relations` links (relation topology is
//! re-derived on each generation pass).
//!
//! **Stability contract**: given identical values for all input fields, the
//! computed digest is identical across runs, platforms, and Rust toolchain
//! versions. The only external dependency is the SHA-256 algorithm.

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};
use sha2::{
    Digest as _,
    Sha256,
};
use uuid::Uuid;

/// The domain category of the source entity captured in an [`IndexEntry`].
///
/// This enum is stable: adding a variant is a non-breaking extension. Removing
/// or renaming a variant is a breaking change that requires a migration.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum ContentKind {
    /// A ticket (issue, task, bug, or epic).
    Ticket,
    /// A specification document.
    Spec,
    /// A rule or policy entry.
    Rule,
    /// A test case or test plan entry.
    Test,
    /// A finding produced by an audit pass.
    AuditFinding,
    /// A workspace-level summary entry (e.g. the workspace root index).
    WorkspaceSummary,
    /// A rule catalog entry aggregating multiple rules.
    RuleCatalog,
    /// An index entry that itself describes another index artifact.
    Index,
    /// An entry placed under `.agents/` for direct agent-client consumption (D1 third surface).
    ///
    /// Agent-hook entries share the same schema as all other entries; the
    /// `content_kind` discriminant allows generators to route them to the
    /// correct placement surface without a separate type.
    AgentHook,
}

/// The semantic relationship from one [`IndexRef`] (or [`IndexEntry`]) to another.
///
/// Direction: the relation is read as "this entry *relation_kind* the referenced entry".
/// For example, `RelationKind::DependsOn` means "this entry depends on the referenced entry".
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum RelationKind {
    /// The referenced entry is the hierarchical parent of this entry.
    Parent,
    /// The referenced entry is a direct child of this entry.
    Child,
    /// This entry depends on the referenced entry being complete/satisfied first.
    DependsOn,
    /// The referenced entry is related but has no strict ordering.
    Related,
    /// This entry blocks progress on the referenced entry.
    Blocks,
    /// This entry supersedes (replaces) the referenced entry.
    Supersedes,
}

/// A typed cross-reference link from one index entry to another.
///
/// `IndexRef` keeps references slim but dense. It carries enough information
/// to resolve the target without loading the full target entry: the canonical
/// path, the target's UUID, its content kind, and an optional in-document anchor.
///
/// The `digest` field mirrors the target entry's [`IndexEntry::digest`] at
/// generation time and can be used to detect stale references on the next
/// generation pass (a mismatch signals the target has changed).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexRef {
    /// Repository-relative path to the target index artifact, using `/` separators.
    ///
    /// Stability: paths are stable within a workspace; cross-workspace refs use
    /// an explicit workspace prefix agreed by the generator.
    pub canonical_path: String,

    /// UUID of the target [`IndexEntry`].
    ///
    /// When the target is an entity in a domain store (ticket, spec, etc.), this
    /// UUID matches the entity's store id.
    pub entry_id: Uuid,

    /// Semantic relationship from the containing entry to this target.
    pub relation_kind: RelationKind,

    /// Content domain of the target entry.
    ///
    /// Redundant with the target entry, but stored here so tooling can filter
    /// refs by kind without resolving every target.
    pub content_kind: ContentKind,

    /// SHA-256 hex digest of the target entry at generation time.
    ///
    /// Empty string when the digest is unknown or not yet computed.
    /// A non-empty value that no longer matches the current target signals
    /// a stale reference that should be re-generated.
    #[serde(default)]
    pub digest: String,

    /// Optional in-document anchor (e.g. a Markdown heading slug).
    ///
    /// `None` means the reference points to the entry as a whole.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
}

/// All outbound cross-references from an [`IndexEntry`], grouped by [`RelationKind`].
///
/// Fields are `None` when the corresponding relation set is empty, which keeps
/// serialized artifacts compact.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexRelations {
    /// The single hierarchical parent of this entry (if any).
    ///
    /// An entry has at most one parent; if a generator produces multiple parents
    /// the first one wins and the rest are demoted to `related`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<IndexRef>,

    /// Direct children of this entry in the hierarchy.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<IndexRef>,

    /// Entries this entry depends on (must be satisfied before this one).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<IndexRef>,

    /// Loosely related entries with no strict ordering.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub related: Vec<IndexRef>,
}

impl IndexRelations {
    /// Returns `true` when all relation sets are empty.
    pub fn is_empty(&self) -> bool {
        self.parent.is_none()
            && self.children.is_empty()
            && self.depends_on.is_empty()
            && self.related.is_empty()
    }
}

/// A single entity captured in a domain index artifact.
///
/// `IndexEntry` is the unit of information in every generated index. One entry
/// corresponds to one source entity (ticket, spec, rule, test, etc.) at a
/// specific point in time. Multiple entries form an index artifact; multiple
/// index artifacts form the full memory-api store index.
///
/// # Field stability
///
/// Fields marked *stable* must not be removed or renamed without a migration.
/// Fields marked *informational* may be omitted or recomputed on each generation
/// pass without breaking the digest contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexEntry {
    /// Unique identifier of this entry.
    ///
    /// For domain-store entities (tickets, specs, rules …) this UUID matches
    /// the entity's store id. For synthetic entries (workspace summaries, agent
    /// hooks) it is a freshly generated UUID that is stable for the lifetime of
    /// the artifact.
    ///
    /// *Stable*. Changing this value changes the digest.
    pub id: Uuid,

    /// Content domain of this entry.
    ///
    /// Determines which generator produced this entry and which placement
    /// surface it targets.
    ///
    /// *Stable*. Changing this value changes the digest.
    pub kind: ContentKind,

    /// Repository-relative path to the primary source artifact for this entry,
    /// using `/` separators.
    ///
    /// For a ticket entry this is the path to `ticket.toml`. For a spec entry
    /// it is the path to the spec TOML file. For an agent-hook entry it is the
    /// path to the generated hook file under `.agents/`.
    ///
    /// *Stable*. Changing this value changes the digest.
    pub source_path: String,

    /// Human-readable title of the entry.
    ///
    /// Derived from the source entity's title field. Truncated at 200 characters
    /// if the source title is longer.
    ///
    /// *Stable*. Changing this value changes the digest.
    pub title: String,

    /// One-paragraph plain-text summary of the entry's purpose and state.
    ///
    /// Written by the generator from the source entity's description or body.
    /// Should be 1–3 sentences. May be empty if the source has no description.
    ///
    /// *Stable*. Changing this value changes the digest.
    pub summary: String,

    /// Keyword terms extracted from the source entity for full-text indexing.
    ///
    /// De-duplicated, lower-cased, and sorted before storage. Order in the
    /// serialized artifact is always ascending lexicographic.
    ///
    /// *Stable*. Sorted and joined value changes the digest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub keywords: Vec<String>,

    /// Optional free-text description of what this entry covers.
    ///
    /// Populated from the source entity's `scope` field when present.
    ///
    /// *Stable*. Changing this value (or absence) changes the digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// Optional free-text description of what this entry explicitly does NOT cover.
    ///
    /// Populated from the source entity's `non_goals` field when present.
    ///
    /// *Stable*. Changing this value (or absence) changes the digest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub non_goals: Option<String>,

    /// Cross-references to related entries, grouped by relation kind.
    ///
    /// Relation topology is re-derived on each generation pass and is NOT
    /// included in the digest computation.
    ///
    /// *Informational*.
    #[serde(default, skip_serializing_if = "IndexRelations::is_empty")]
    pub relations: IndexRelations,

    /// SHA-256 hex digest of the stable input fields (see module-level docs).
    ///
    /// An empty string signals that the digest has not yet been computed.
    /// Populate via [`IndexEntry::compute_digest`] before writing an artifact.
    ///
    /// *Excluded from digest computation* (would be circular).
    #[serde(default)]
    pub digest: String,

    /// Comma-separated or multi-value tags for filtering and grouping.
    ///
    /// De-duplicated, lower-cased, and sorted before storage.
    ///
    /// *Stable*. Sorted and joined value changes the digest.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,

    /// UTC timestamp at which this entry was generated.
    ///
    /// Updated on every generation pass. *Excluded from digest computation*.
    ///
    /// *Informational*.
    pub generated_at: DateTime<Utc>,

    /// UTC timestamp of the most recent modification of the source artifact.
    ///
    /// Derived from the source file's `mtime` or the entity's `updated_at` field.
    /// *Excluded from digest computation*.
    ///
    /// *Informational*.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_modified_at: Option<DateTime<Utc>>,
}

impl IndexEntry {
    /// Compute the SHA-256 digest of this entry's stable fields and return it
    /// as a lowercase hex string.
    ///
    /// The digest is deterministic: identical stable field values produce an
    /// identical digest regardless of run order, platform, or Rust toolchain
    /// version. See the [module-level docs](self) for the full normalization
    /// contract.
    ///
    /// This method does NOT mutate `self.digest`. Call
    /// [`IndexEntry::seal`] to both compute and store the digest.
    pub fn compute_digest(&self) -> String {
        let mut hasher = Sha256::new();

        // 1. id
        hasher.update(self.id.hyphenated().to_string().as_bytes());
        hasher.update(b"\0");

        // 2. kind
        let kind_str = serde_json::to_string(&self.kind)
            .unwrap_or_default()
            .trim_matches('"')
            .to_owned();
        hasher.update(kind_str.as_bytes());
        hasher.update(b"\0");

        // 3. source_path
        hasher.update(self.source_path.as_bytes());
        hasher.update(b"\0");

        // 4. title
        hasher.update(self.title.as_bytes());
        hasher.update(b"\0");

        // 5. summary
        hasher.update(self.summary.as_bytes());
        hasher.update(b"\0");

        // 6. scope (empty string when absent)
        hasher.update(self.scope.as_deref().unwrap_or("").as_bytes());
        hasher.update(b"\0");

        // 7. non_goals (empty string when absent)
        hasher.update(self.non_goals.as_deref().unwrap_or("").as_bytes());
        hasher.update(b"\0");

        // 8. keywords: sorted and joined with ','
        let mut sorted_keywords = self.keywords.clone();
        sorted_keywords.sort_unstable();
        hasher.update(sorted_keywords.join(",").as_bytes());
        hasher.update(b"\0");

        // 9. tags: sorted and joined with ','
        let mut sorted_tags = self.tags.clone();
        sorted_tags.sort_unstable();
        hasher.update(sorted_tags.join(",").as_bytes());

        format!("{:x}", hasher.finalize())
    }

    /// Compute the digest and store it in `self.digest`, then return a
    /// reference to `self` for chaining.
    ///
    /// Call this once all stable fields have been populated, immediately before
    /// writing the artifact to disk or into an index collection.
    pub fn seal(&mut self) -> &mut Self {
        self.digest = self.compute_digest();
        self
    }

    /// Returns `true` when the stored `digest` matches the digest computed
    /// from the current stable field values.
    ///
    /// A `false` result means the entry's fields have changed since it was
    /// last sealed, or the digest was never computed.
    pub fn is_digest_valid(&self) -> bool {
        if self.digest.is_empty() {
            return false;
        }
        self.digest == self.compute_digest()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry() -> IndexEntry {
        IndexEntry {
            id: Uuid::nil(),
            kind: ContentKind::Ticket,
            source_path: "some/ticket.toml".to_string(),
            title: "Example ticket".to_string(),
            summary: "A test entry.".to_string(),
            keywords: vec!["alpha".to_string(), "beta".to_string()],
            scope: None,
            non_goals: None,
            relations: IndexRelations::default(),
            digest: String::new(),
            tags: vec!["tag-a".to_string()],
            generated_at: DateTime::from_timestamp(0, 0).unwrap(),
            source_modified_at: None,
        }
    }

    #[test]
    fn digest_is_stable_across_calls() {
        let entry = make_entry();
        let d1 = entry.compute_digest();
        let d2 = entry.compute_digest();
        assert_eq!(d1, d2, "digest must be deterministic");
    }

    #[test]
    fn digest_changes_when_title_changes() {
        let mut a = make_entry();
        let mut b = make_entry();
        b.title = "Different title".to_string();
        assert_ne!(
            a.compute_digest(),
            b.compute_digest(),
            "digest must change when title changes"
        );
        // generated_at does NOT affect digest
        a.generated_at = DateTime::from_timestamp(9999, 0).unwrap();
        assert_eq!(
            a.compute_digest(),
            make_entry().compute_digest(),
            "generated_at must not affect digest"
        );
    }

    #[test]
    fn digest_not_affected_by_generated_at() {
        let mut a = make_entry();
        let mut b = make_entry();
        b.generated_at = DateTime::from_timestamp(99999, 0).unwrap();
        assert_eq!(
            a.compute_digest(),
            b.compute_digest(),
            "generated_at must be excluded from digest"
        );
        // source_modified_at also excluded
        a.source_modified_at = Some(DateTime::from_timestamp(1, 0).unwrap());
        assert_eq!(
            a.compute_digest(),
            make_entry().compute_digest(),
            "source_modified_at must be excluded from digest"
        );
    }

    #[test]
    fn seal_sets_digest_field() {
        let mut entry = make_entry();
        assert!(entry.digest.is_empty());
        entry.seal();
        assert!(!entry.digest.is_empty());
        assert!(entry.is_digest_valid());
    }

    #[test]
    fn is_digest_valid_detects_mutation() {
        let mut entry = make_entry();
        entry.seal();
        assert!(entry.is_digest_valid());
        entry.title = "mutated".to_string();
        assert!(!entry.is_digest_valid());
    }

    #[test]
    fn roundtrip_json() {
        let mut entry = make_entry();
        entry.seal();
        let json = serde_json::to_string(&entry).unwrap();
        let decoded: IndexEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(entry, decoded);
    }

    #[test]
    fn keywords_sorted_before_digest() {
        let mut a = make_entry();
        let mut b = make_entry();
        // same keywords, different insertion order
        a.keywords = vec!["beta".to_string(), "alpha".to_string()];
        b.keywords = vec!["alpha".to_string(), "beta".to_string()];
        assert_eq!(
            a.compute_digest(),
            b.compute_digest(),
            "keyword order must not affect digest"
        );
    }

    #[test]
    fn content_kind_serde_roundtrip() {
        let kinds = [
            ContentKind::Ticket,
            ContentKind::Spec,
            ContentKind::Rule,
            ContentKind::Test,
            ContentKind::AuditFinding,
            ContentKind::WorkspaceSummary,
            ContentKind::RuleCatalog,
            ContentKind::Index,
            ContentKind::AgentHook,
        ];
        for kind in kinds {
            let s = serde_json::to_string(&kind).unwrap();
            let decoded: ContentKind = serde_json::from_str(&s).unwrap();
            assert_eq!(kind, decoded);
        }
    }

    #[test]
    fn relation_kind_serde_roundtrip() {
        let kinds = [
            RelationKind::Parent,
            RelationKind::Child,
            RelationKind::DependsOn,
            RelationKind::Related,
            RelationKind::Blocks,
            RelationKind::Supersedes,
        ];
        for kind in kinds {
            let s = serde_json::to_string(&kind).unwrap();
            let decoded: RelationKind = serde_json::from_str(&s).unwrap();
            assert_eq!(kind, decoded);
        }
    }
}
