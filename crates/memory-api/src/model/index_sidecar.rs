//! TOON sidecar format for memory-api store index artifacts.
//!
//! An [`IndexSidecar`] is the compact machine-readable companion emitted
//! alongside every human-readable `README.md` index. It lives at a fixed
//! path inside the store folder it describes, e.g.:
//!
//! - `.ticket/index.toon`
//! - `.spec/index.toon`
//! - `.rule/index.toon`
//! - `.audit/index.toon`
//!
//! All three D1 placement surfaces share the same format: workspace-folder
//! sidecars, folder-level README index entries, and `.agents/` agent-hook
//! entries all serialize as [`IndexEntry`] payloads inside an
//! [`IndexSidecar`].
//!
//! # Wire format
//!
//! The sidecar is serialized to JSON (via `serde_json`) and then encoded with
//! `toon_format::encode_default` for storage. Consumers decode with
//! `toon_format::decode_default` and then deserialize from JSON. This keeps
//! TOON as the primary on-disk encoding while retaining full serde
//! compatibility.
//!
//! # Validation
//!
//! [`IndexSidecar::validate`] checks:
//! - every `source_path` referenced in an entry exists on disk
//! - every entry's stored `digest` matches a fresh `compute_digest()` call
//! - no duplicate `id` values within the sidecar

use std::{
    collections::HashSet,
    path::Path,
};

use chrono::{
    DateTime,
    Utc,
};
use serde::{
    Deserialize,
    Serialize,
};

use super::index_entry::{
    ContentKind,
    IndexEntry,
};

/// Version token stored in every sidecar. Bump when the schema changes in a
/// backward-incompatible way. Readers should warn (not fail) on unknown
/// versions so tooling degrades gracefully.
pub const SIDECAR_VERSION: u32 = 1;

/// A validation issue found by [`IndexSidecar::validate`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SidecarValidationIssue {
    /// An entry's `source_path` does not exist on disk.
    ///
    /// Contains the entry index (0-based) and the missing path string.
    BrokenSourcePath {
        entry_index: usize,
        source_path: String,
    },

    /// An entry's stored `digest` does not match a freshly computed digest.
    ///
    /// Contains the entry index, the stored digest, and the computed digest.
    StaleDigest {
        entry_index: usize,
        stored: String,
        computed: String,
    },

    /// An entry has an empty `digest` (was never sealed before writing).
    ///
    /// Contains the entry index.
    MissingDigest { entry_index: usize },

    /// Two entries share the same `id` UUID.
    ///
    /// Contains the duplicate UUID string and the two entry indices.
    DuplicateId {
        id: String,
        first: usize,
        second: usize,
    },
}

impl std::fmt::Display for SidecarValidationIssue {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::BrokenSourcePath {
                entry_index,
                source_path,
            } => write!(
                f,
                "entry[{entry_index}]: source_path not found on disk: {source_path}"
            ),
            Self::StaleDigest {
                entry_index,
                stored,
                computed,
            } => write!(
                f,
                "entry[{entry_index}]: stale digest (stored={stored}, computed={computed})"
            ),
            Self::MissingDigest { entry_index } => {
                write!(
                    f,
                    "entry[{entry_index}]: digest is empty (entry was never sealed)"
                )
            },
            Self::DuplicateId { id, first, second } => {
                write!(
                    f,
                    "duplicate entry id {id} at indices {first} and {second}"
                )
            },
        }
    }
}

/// A compact, machine-readable index artifact co-located with a store folder.
///
/// Consumers can parse and query entries (by id, keyword, or digest) without
/// reading the full markdown README.
///
/// # Serialization
///
/// Write path:
/// ```no_run
/// # use memory_api::model::index_sidecar::IndexSidecar;
/// # let sidecar = IndexSidecar::default();
/// let json = serde_json::to_string(&sidecar).unwrap();
/// let toon = toon_format::encode_default(&serde_json::from_str::<serde_json::Value>(&json).unwrap()).unwrap();
/// std::fs::write(".ticket/index.toon", toon).unwrap();
/// ```
///
/// Read path:
/// ```no_run
/// # use memory_api::model::index_sidecar::IndexSidecar;
/// let toon = std::fs::read_to_string(".ticket/index.toon").unwrap();
/// let json_value = toon_format::decode_default(&toon).unwrap();
/// let sidecar: IndexSidecar = serde_json::from_value(json_value).unwrap();
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexSidecar {
    /// Schema version. Currently `1`. Readers should warn on unknown versions.
    pub version: u32,

    /// The content domain this sidecar indexes (e.g. `ticket`, `spec`, `rule`).
    pub domain: ContentKind,

    /// Repository-relative path to the store folder this sidecar indexes,
    /// using `/` separators (e.g. `.ticket`).
    pub store_path: String,

    /// UTC timestamp at which this sidecar was generated.
    pub generated_at: DateTime<Utc>,

    /// All [`IndexEntry`] records captured in this sidecar.
    ///
    /// Ordering is stable across generation passes (deterministic sort by `id`
    /// ascending) so diff tooling produces minimal output.
    pub entries: Vec<IndexEntry>,
}

impl IndexSidecar {
    /// Create a new sidecar with the given domain and store path.
    ///
    /// `entries` should be sorted by `id` (use [`IndexSidecar::sort`]) and
    /// every entry should be sealed (via [`IndexEntry::seal`]) before writing.
    pub fn new(
        domain: ContentKind,
        store_path: impl Into<String>,
        entries: Vec<IndexEntry>,
    ) -> Self {
        Self {
            version: SIDECAR_VERSION,
            domain,
            store_path: store_path.into(),
            generated_at: Utc::now(),
            entries,
        }
    }

    /// Sort entries by `id` ascending for stable diff output.
    pub fn sort(&mut self) {
        self.entries.sort_by_key(|e| e.id);
    }

    /// Encode this sidecar to a TOON string ready for writing to disk.
    ///
    /// Equivalent to serializing to JSON and then calling
    /// `toon_format::encode_default`. Returns the TOON string.
    pub fn encode_toon(&self) -> Result<String, SidecarError> {
        let json =
            serde_json::to_value(self).map_err(SidecarError::Serialize)?;
        toon_format::encode_default(&json)
            .map_err(|e| SidecarError::Toon(e.to_string()))
    }

    /// Decode a sidecar from a TOON string (as read from disk).
    ///
    /// Equivalent to calling `toon_format::decode_default` and then
    /// deserializing from JSON.
    pub fn decode_toon(toon: &str) -> Result<Self, SidecarError> {
        let json = toon_format::decode_default(toon)
            .map_err(|e| SidecarError::Toon(e.to_string()))?;
        serde_json::from_value(json).map_err(SidecarError::Deserialize)
    }

    /// Validate the sidecar against the filesystem rooted at `workspace_root`.
    ///
    /// Returns a (possibly empty) list of [`SidecarValidationIssue`]s.
    /// An empty list means the sidecar is fully consistent.
    ///
    /// Checks performed (in order):
    /// 1. No duplicate `id` values.
    /// 2. No empty `digest` fields.
    /// 3. No stale digests (stored digest ≠ `compute_digest()`).
    /// 4. Every `source_path` resolves to an existing file under `workspace_root`.
    pub fn validate(
        &self,
        workspace_root: &Path,
    ) -> Vec<SidecarValidationIssue> {
        let mut issues = Vec::new();
        let mut seen_ids: HashSet<String> = HashSet::new();

        for (i, entry) in self.entries.iter().enumerate() {
            // 1. Duplicate id check
            let id_str = entry.id.to_string();
            if let Some(first) = seen_ids.get(&id_str).map(|_| {
                self.entries[..i]
                    .iter()
                    .position(|e| e.id == entry.id)
                    .unwrap_or(0)
            }) {
                issues.push(SidecarValidationIssue::DuplicateId {
                    id: id_str.clone(),
                    first,
                    second: i,
                });
            } else {
                seen_ids.insert(id_str);
            }

            // 2. Missing digest
            if entry.digest.is_empty() {
                issues.push(SidecarValidationIssue::MissingDigest {
                    entry_index: i,
                });
            } else {
                // 3. Stale digest
                let computed = entry.compute_digest();
                if computed != entry.digest {
                    issues.push(SidecarValidationIssue::StaleDigest {
                        entry_index: i,
                        stored: entry.digest.clone(),
                        computed,
                    });
                }
            }

            // 4. Broken source path
            let abs_path = workspace_root.join(&entry.source_path);
            if !abs_path.exists() {
                issues.push(SidecarValidationIssue::BrokenSourcePath {
                    entry_index: i,
                    source_path: entry.source_path.clone(),
                });
            }
        }

        issues
    }
}

impl Default for IndexSidecar {
    fn default() -> Self {
        Self {
            version: SIDECAR_VERSION,
            domain: ContentKind::Index,
            store_path: String::new(),
            generated_at: Utc::now(),
            entries: Vec::new(),
        }
    }
}

/// Errors produced by sidecar encode/decode operations.
#[derive(Debug)]
pub enum SidecarError {
    /// JSON serialization failed.
    Serialize(serde_json::Error),
    /// JSON deserialization failed.
    Deserialize(serde_json::Error),
    /// TOON encode/decode failed.
    Toon(String),
    /// I/O error reading or writing the sidecar file.
    Io(std::io::Error),
}

impl std::fmt::Display for SidecarError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::Serialize(e) => write!(f, "sidecar serialize error: {e}"),
            Self::Deserialize(e) => write!(f, "sidecar deserialize error: {e}"),
            Self::Toon(s) => write!(f, "sidecar TOON error: {s}"),
            Self::Io(e) => write!(f, "sidecar I/O error: {e}"),
        }
    }
}

impl std::error::Error for SidecarError {}

impl From<std::io::Error> for SidecarError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

/// Convenience: read and decode an `index.toon` sidecar from disk.
pub fn read_sidecar(path: &Path) -> Result<IndexSidecar, SidecarError> {
    let toon = std::fs::read_to_string(path)?;
    IndexSidecar::decode_toon(&toon)
}

/// Convenience: encode and write an `index.toon` sidecar to disk.
///
/// Creates parent directories if they don't exist.
pub fn write_sidecar(
    path: &Path,
    sidecar: &IndexSidecar,
) -> Result<(), SidecarError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let toon = sidecar.encode_toon()?;
    std::fs::write(path, toon)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;
    use crate::model::index_entry::{
        IndexEntry,
        IndexRelations,
    };

    fn make_entry(
        id: Uuid,
        source_path: &str,
    ) -> IndexEntry {
        let mut e = IndexEntry {
            id,
            kind: ContentKind::Ticket,
            source_path: source_path.to_string(),
            title: format!("Entry {id}"),
            summary: "A test entry.".to_string(),
            keywords: vec![],
            scope: None,
            non_goals: None,
            relations: IndexRelations::default(),
            digest: String::new(),
            tags: vec![],
            generated_at: chrono::DateTime::from_timestamp(0, 0).unwrap(),
            source_modified_at: None,
        };
        e.seal();
        e
    }

    #[test]
    fn encode_decode_roundtrip() {
        let sidecar = IndexSidecar::new(
            ContentKind::Ticket,
            ".ticket",
            vec![make_entry(
                Uuid::new_v4(),
                ".ticket/tickets/abc/ticket.toml",
            )],
        );
        let toon = sidecar.encode_toon().expect("encode should succeed");
        let decoded =
            IndexSidecar::decode_toon(&toon).expect("decode should succeed");
        assert_eq!(sidecar.version, decoded.version);
        assert_eq!(sidecar.domain, decoded.domain);
        assert_eq!(sidecar.store_path, decoded.store_path);
        assert_eq!(sidecar.entries.len(), decoded.entries.len());
        assert_eq!(sidecar.entries[0].id, decoded.entries[0].id);
        assert_eq!(sidecar.entries[0].digest, decoded.entries[0].digest);
    }

    #[test]
    fn validate_detects_missing_digest() {
        let mut entry = make_entry(Uuid::nil(), "nonexistent.toml");
        entry.digest = String::new(); // clear seal
        let sidecar =
            IndexSidecar::new(ContentKind::Ticket, ".ticket", vec![entry]);
        let issues = sidecar.validate(Path::new("."));
        assert!(
            issues.iter().any(|i| matches!(
                i,
                SidecarValidationIssue::MissingDigest { entry_index: 0 }
            )),
            "should detect missing digest"
        );
    }

    #[test]
    fn validate_detects_stale_digest() {
        let mut entry = make_entry(Uuid::nil(), "nonexistent.toml");
        entry.seal();
        entry.title = "mutated title".to_string(); // digest now stale
        let sidecar =
            IndexSidecar::new(ContentKind::Ticket, ".ticket", vec![entry]);
        let issues = sidecar.validate(Path::new("."));
        assert!(
            issues.iter().any(|i| matches!(
                i,
                SidecarValidationIssue::StaleDigest { entry_index: 0, .. }
            )),
            "should detect stale digest"
        );
    }

    #[test]
    fn validate_detects_duplicate_ids() {
        let id = Uuid::nil();
        let entries = vec![make_entry(id, "a.toml"), make_entry(id, "b.toml")];
        let sidecar =
            IndexSidecar::new(ContentKind::Ticket, ".ticket", entries);
        let issues = sidecar.validate(Path::new("."));
        assert!(
            issues.iter().any(|i| matches!(
                i,
                SidecarValidationIssue::DuplicateId { .. }
            )),
            "should detect duplicate ids"
        );
    }

    #[test]
    fn validate_broken_source_path() {
        let entry =
            make_entry(Uuid::new_v4(), "definitely/does/not/exist.toml");
        let sidecar =
            IndexSidecar::new(ContentKind::Ticket, ".ticket", vec![entry]);
        let issues = sidecar.validate(Path::new("."));
        assert!(
            issues.iter().any(|i| matches!(
                i,
                SidecarValidationIssue::BrokenSourcePath { entry_index: 0, .. }
            )),
            "should detect broken source path"
        );
    }

    #[test]
    fn sort_is_deterministic() {
        let a = make_entry(
            Uuid::parse_str("aaaaaaaa-0000-0000-0000-000000000000").unwrap(),
            "a.toml",
        );
        let b = make_entry(
            Uuid::parse_str("bbbbbbbb-0000-0000-0000-000000000000").unwrap(),
            "b.toml",
        );
        let mut sidecar = IndexSidecar::new(
            ContentKind::Ticket,
            ".ticket",
            vec![b.clone(), a.clone()],
        );
        sidecar.sort();
        assert_eq!(sidecar.entries[0].id, a.id);
        assert_eq!(sidecar.entries[1].id, b.id);
    }
}
