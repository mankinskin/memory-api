//! Index invariants — the conditions that must hold before the index can serve
//! correct results for **any read operation**, plus the vocabulary used to
//! validate and proactively heal them.
//!
//! # Why this exists
//!
//! [`EntityStore`](crate::storage::entity_store::EntityStore) composes three
//! storage layers that must stay mutually consistent:
//!
//! 1. the filesystem source-of-truth ([`EntityFs`](crate::storage::entity_fs::EntityFs)),
//! 2. the SQLite metadata index ([`RedbIndexStore`](crate::storage::index::RedbIndexStore)),
//! 3. the Tantivy full-text search index ([`TantivySearchIndex`](crate::storage::search::TantivySearchIndex)).
//!
//! A read is only correct when every layer is structurally valid **and** the
//! three layers agree. Rather than catching errors after a read fails (or a
//! background Tantivy thread panics), the store *proactively* enforces these
//! invariants: every read and write entry point first brings the index into a
//! valid state via
//! [`EntityStore::ensure_ready`](crate::storage::entity_store::EntityStore::ensure_ready),
//! and a full reconciliation is available through
//! [`EntityStore::verify_and_heal`](crate::storage::entity_store::EntityStore::verify_and_heal).
//!
//! # The invariants
//!
//! Structural invariants (`I1`–`I6`) are cheap, local to one layer, and are
//! enforced on **every** API interaction. Consistency invariants (`I7`–`I10`)
//! span layers, require a scan to verify, and are enforced by a reconciling
//! heal (`scan(true)`).

/// A single condition that must hold for the index to serve correct reads.
///
/// Variants are ordered outermost-structural-layer first, then inward to
/// cross-layer consistency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IndexInvariant {
    /// `I1` — The index root directory exists.
    ///
    /// Heal: create the directory.
    IndexRootExists,

    /// `I2` — The SQLite metadata database is open-able and contains every
    /// required table.
    ///
    /// Heal: (re)create the missing tables (`CREATE TABLE IF NOT EXISTS`).
    MetadataTablesPresent,

    /// `I3` — The SQLite metadata schema version matches the current
    /// [`SCHEMA_VERSION`](crate::storage::schema::SCHEMA_VERSION).
    ///
    /// Heal: write the version row when absent. A *mismatched* (not merely
    /// missing) version is a migration concern and surfaces as a violation
    /// rather than being silently overwritten.
    MetadataSchemaVersionCurrent,

    /// `I4` — The Tantivy search index directory exists.
    ///
    /// Heal: create the directory.
    SearchDirExists,

    /// `I5` — The on-disk Tantivy index is open-able (not corrupt).
    ///
    /// Heal: reset the search directory and rebuild from the filesystem.
    SearchIndexOpenable,

    /// `I6` — The on-disk Tantivy schema matches the current
    /// [`build_schema`](crate::storage::search) layout.
    ///
    /// A stale schema (for example one created before a fast field was added)
    /// makes the fast-field writer index past the end of its field vector and
    /// panic on a background thread. Heal: reset the search directory and
    /// rebuild from the filesystem.
    SearchSchemaCurrent,

    /// `I7` — Every entity folder on disk under a scan root is represented in
    /// the metadata index.
    ///
    /// Heal: integrate the missing entity (`scan`).
    EveryEntityIndexed,

    /// `I8` — Every metadata index entry points to an entity folder that still
    /// exists on disk.
    ///
    /// Heal: prune the stale index entry (`scan(true)`).
    NoStaleIndexEntries,

    /// `I9` — Every metadata index entry has a matching full-text search
    /// document, so search can return it.
    ///
    /// Heal: re-index the entity body into the search index (`scan(true)`).
    EverySearchDocPresent,

    /// `I10` — The search index holds no documents for entities that no longer
    /// exist in the metadata index.
    ///
    /// Heal: remove the stale search document (`scan(true)`).
    NoStaleSearchDocs,
}

impl IndexInvariant {
    /// Every invariant, in enforcement order.
    pub const ALL: [IndexInvariant; 10] = [
        IndexInvariant::IndexRootExists,
        IndexInvariant::MetadataTablesPresent,
        IndexInvariant::MetadataSchemaVersionCurrent,
        IndexInvariant::SearchDirExists,
        IndexInvariant::SearchIndexOpenable,
        IndexInvariant::SearchSchemaCurrent,
        IndexInvariant::EveryEntityIndexed,
        IndexInvariant::NoStaleIndexEntries,
        IndexInvariant::EverySearchDocPresent,
        IndexInvariant::NoStaleSearchDocs,
    ];

    /// Stable short identifier (`"I1"`..`"I10"`).
    pub fn id(self) -> &'static str {
        match self {
            IndexInvariant::IndexRootExists => "I1",
            IndexInvariant::MetadataTablesPresent => "I2",
            IndexInvariant::MetadataSchemaVersionCurrent => "I3",
            IndexInvariant::SearchDirExists => "I4",
            IndexInvariant::SearchIndexOpenable => "I5",
            IndexInvariant::SearchSchemaCurrent => "I6",
            IndexInvariant::EveryEntityIndexed => "I7",
            IndexInvariant::NoStaleIndexEntries => "I8",
            IndexInvariant::EverySearchDocPresent => "I9",
            IndexInvariant::NoStaleSearchDocs => "I10",
        }
    }

    /// Human-readable description of the condition.
    pub fn description(self) -> &'static str {
        match self {
            IndexInvariant::IndexRootExists =>
                "index root directory exists",
            IndexInvariant::MetadataTablesPresent =>
                "metadata database is open-able with all required tables",
            IndexInvariant::MetadataSchemaVersionCurrent =>
                "metadata schema version matches the current version",
            IndexInvariant::SearchDirExists =>
                "search index directory exists",
            IndexInvariant::SearchIndexOpenable =>
                "search index is open-able (not corrupt)",
            IndexInvariant::SearchSchemaCurrent =>
                "search index schema matches the current schema",
            IndexInvariant::EveryEntityIndexed =>
                "every on-disk entity is present in the metadata index",
            IndexInvariant::NoStaleIndexEntries =>
                "every metadata index entry exists on disk",
            IndexInvariant::EverySearchDocPresent =>
                "every indexed entity has a search document",
            IndexInvariant::NoStaleSearchDocs =>
                "search index holds no documents for deleted entities",
        }
    }

    /// Whether this invariant is *structural* (cheap, single-layer) and is
    /// therefore enforced on every API interaction, as opposed to a
    /// cross-layer *consistency* invariant enforced by a reconciling scan.
    pub fn is_structural(self) -> bool {
        matches!(
            self,
            IndexInvariant::IndexRootExists
                | IndexInvariant::MetadataTablesPresent
                | IndexInvariant::MetadataSchemaVersionCurrent
                | IndexInvariant::SearchDirExists
                | IndexInvariant::SearchIndexOpenable
                | IndexInvariant::SearchSchemaCurrent
        )
    }
}

/// The evaluated state of a single [`IndexInvariant`].
#[derive(Debug, Clone)]
pub struct IndexInvariantStatus {
    pub invariant: IndexInvariant,
    pub satisfied: bool,
    /// Optional context: the violation reason or healing action taken.
    pub detail: Option<String>,
}

impl IndexInvariantStatus {
    pub fn satisfied(invariant: IndexInvariant) -> Self {
        Self {
            invariant,
            satisfied: true,
            detail: None,
        }
    }

    pub fn violated(
        invariant: IndexInvariant,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            invariant,
            satisfied: false,
            detail: Some(detail.into()),
        }
    }
}

/// A full evaluation of all index invariants.
#[derive(Debug, Clone, Default)]
pub struct IndexInvariantReport {
    pub statuses: Vec<IndexInvariantStatus>,
}

impl IndexInvariantReport {
    pub fn push(
        &mut self,
        status: IndexInvariantStatus,
    ) {
        self.statuses.push(status);
    }

    /// `true` when every evaluated invariant is satisfied.
    pub fn all_satisfied(&self) -> bool {
        self.statuses.iter().all(|status| status.satisfied)
    }

    /// Iterator over the violated invariants.
    pub fn violations(
        &self
    ) -> impl Iterator<Item = &IndexInvariantStatus> {
        self.statuses.iter().filter(|status| !status.satisfied)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_invariants_have_unique_ids() {
        let mut ids: Vec<&str> =
            IndexInvariant::ALL.iter().map(|i| i.id()).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), IndexInvariant::ALL.len());
    }

    #[test]
    fn structural_invariants_are_the_first_six() {
        let structural: Vec<_> = IndexInvariant::ALL
            .iter()
            .copied()
            .filter(|i| i.is_structural())
            .collect();
        assert_eq!(structural.len(), 6);
        assert!(structural.contains(&IndexInvariant::SearchSchemaCurrent));
        assert!(!IndexInvariant::EveryEntityIndexed.is_structural());
    }

    #[test]
    fn report_tracks_violations() {
        let mut report = IndexInvariantReport::default();
        report.push(IndexInvariantStatus::satisfied(
            IndexInvariant::IndexRootExists,
        ));
        report.push(IndexInvariantStatus::violated(
            IndexInvariant::SearchSchemaCurrent,
            "stale 5-field schema",
        ));
        assert!(!report.all_satisfied());
        assert_eq!(report.violations().count(), 1);
    }
}
