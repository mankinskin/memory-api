use std::path::{
    Path,
    PathBuf,
};

use chrono::Utc;
use uuid::Uuid;

use crate::{
    error::StorageError,
    model::{
        edge::EdgeRecord,
        filesystem::{
            ParseDiagnostic,
            ScanRoot,
        },
        query::parse_query,
        schema_registry::SchemaRegistry,
    },
    storage::{
        entity_fs::{
            EntityFs,
            EntityScanEntry,
        },
        index::RedbIndexStore,
        indexed::IndexedEntity,
        local_root::ensure_sqlite_index_root,
        search::{
            SearchResult,
            TantivySearchIndex,
        },
    },
};

/// Result of a full scan across all registered roots.
pub struct ScanReport {
    pub integrated: usize,
    pub pruned: usize,
    pub diagnostics: Vec<ParseDiagnostic>,
}

/// Convenience facade composing all three storage layers:
/// [`RedbIndexStore`] (metadata index), [`EntityFs`] (filesystem),
/// and [`TantivySearchIndex`] (full-text search).
///
/// Downstream crates can use this as a single entry point instead
/// of managing the three stores individually.
pub struct EntityStore {
    pub index: RedbIndexStore,
    pub fs: EntityFs,
    pub search: TantivySearchIndex,
    pub schema_registry: SchemaRegistry,
    pub index_root: PathBuf,
}

impl EntityStore {
    /// Open (or create) an entity store rooted at `index_root`.
    ///
    /// `index_root` is the directory for SQLite + Tantivy index files.
    /// `fs` provides the filesystem layout configuration for entity folders.
    pub fn open(
        index_root: &Path,
        fs: EntityFs,
    ) -> Result<Self, StorageError> {
        Self::open_with(index_root, fs, SchemaRegistry::new())
    }

    /// Open with a custom schema registry.
    pub fn open_with(
        index_root: &Path,
        fs: EntityFs,
        schema_registry: SchemaRegistry,
    ) -> Result<Self, StorageError> {
        ensure_sqlite_index_root(
            index_root,
            "entities.db",
            &["search_index/"],
        )?;
        let db_path = index_root.join("entities.db");
        let search_dir = index_root.join("search_index");

        let index = RedbIndexStore::open(&db_path)?;
        let search = TantivySearchIndex::open_or_create(&search_dir)?;

        Ok(Self {
            index,
            fs,
            search,
            schema_registry,
            index_root: index_root.to_path_buf(),
        })
    }

    pub fn schema_registry(&self) -> &SchemaRegistry {
        &self.schema_registry
    }

    // ── Scan-root management ────────────────────────────────────────

    pub fn add_scan_root(
        &self,
        root: ScanRoot,
    ) -> Result<(), StorageError> {
        self.index.add_scan_root(&root)
    }

    pub fn list_scan_roots(&self) -> Result<Vec<ScanRoot>, StorageError> {
        self.index.list_scan_roots()
    }

    // ── Index queries ───────────────────────────────────────────────

    pub fn get_indexed(
        &self,
        id: &Uuid,
    ) -> Result<Option<IndexedEntity>, StorageError> {
        self.index.get_ticket(id)
    }

    pub fn list_indexed(&self) -> Result<Vec<IndexedEntity>, StorageError> {
        self.index.list_tickets()
    }

    // ── Full-text search ────────────────────────────────────────────

    pub fn search(
        &self,
        query_expr: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, StorageError> {
        let expr = parse_query(query_expr)?;
        self.search.search(&expr, limit)
    }

    // ── Edge management ─────────────────────────────────────────────

    pub fn add_edge(
        &self,
        edge: EdgeRecord,
    ) -> Result<(), StorageError> {
        // Enforce acyclicity when the schema says so.
        let is_acyclic = self
            .schema_registry
            .type_ids()
            .filter_map(|tid| self.schema_registry.get(tid))
            .filter_map(|s| s.edge_rules.get(&edge.kind))
            .any(|r| r.acyclic_enforced);

        if is_acyclic && self.index.is_reachable(&edge.to, &edge.from)? {
            return Err(StorageError::DependencyCycle);
        }

        self.index.insert_edge(&edge)
    }

    pub fn remove_edge(
        &self,
        edge: EdgeRecord,
    ) -> Result<(), StorageError> {
        self.index.delete_edge(&edge)
    }

    pub fn edges_from(
        &self,
        id: &Uuid,
    ) -> Result<Vec<EdgeRecord>, StorageError> {
        self.index.edges_from(id)
    }

    pub fn list_all_edges(&self) -> Result<Vec<EdgeRecord>, StorageError> {
        self.index.list_all_edges()
    }

    // ── Scan / reconcile ────────────────────────────────────────────

    /// Scan all registered roots (plus the default entities dir under
    /// `index_root`) and reconcile the index + search stores.
    ///
    /// When `reindex` is `true`, the search index is cleared first and
    /// stale SQLite entries are pruned.
    pub fn scan(
        &self,
        reindex: bool,
    ) -> Result<ScanReport, StorageError> {
        if reindex {
            self.search.clear_all()?;
        }

        let roots = self.index.list_scan_roots()?;
        let default_root = ScanRoot {
            path: self.index_root.join("entities"),
            label: "default".into(),
        };
        let all_roots: Vec<&ScanRoot> =
            std::iter::once(&default_root).chain(roots.iter()).collect();

        let mut integrated = 0usize;
        let mut diagnostics = Vec::new();
        let mut disk_ids = std::collections::HashSet::new();

        for root in all_roots {
            if !root.path.exists() {
                continue;
            }
            let (entries, diags) = self.fs.scan_root(&root.path)?;
            diagnostics.extend(diags);

            for entry in entries {
                disk_ids.insert(entry.id);
                self.integrate_entry(entry, reindex)?;
                integrated += 1;
            }
        }

        let mut pruned = 0usize;
        if reindex {
            let indexed = self.index.list_tickets()?;
            for entity in indexed {
                if !disk_ids.contains(&entity.id) {
                    self.index.remove_ticket(&entity.id)?;
                    pruned += 1;
                }
            }
        }

        Ok(ScanReport {
            integrated,
            pruned,
            diagnostics,
        })
    }

    fn integrate_entry(
        &self,
        entry: EntityScanEntry,
        reindex: bool,
    ) -> Result<(), StorageError> {
        let type_id = entry
            .manifest
            .extra
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let title = entry
            .manifest
            .extra
            .get("title")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let state = entry
            .manifest
            .extra
            .get("state")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let now = Utc::now();

        let indexed = match self.index.get_ticket(&entry.id)? {
            Some(mut existing) => {
                existing.path = entry.path.clone();
                existing.type_id = type_id.clone();
                existing.updated_at = now;
                existing.title = title.clone();
                existing.state = state.clone();
                existing
            },
            None => IndexedEntity {
                id: entry.id,
                path: entry.path.clone(),
                type_id: type_id.clone(),
                title: title.clone(),
                state: state.clone(),
                created_at: entry.manifest.created_at,
                updated_at: now,
            },
        };
        self.index.insert_ticket(&indexed)?;

        if reindex {
            let body = self.fs.read_description(&entry.path);
            let created_at_str = indexed.created_at.to_rfc3339();
            let effort_str = entry.manifest.extra.get("effort")
                .and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                });
            self.search.upsert(
                &entry.id,
                title.as_deref(),
                body.as_deref(),
                state.as_deref(),
                Some(&type_id),
                Some(&created_at_str),
                effort_str.as_deref(),
            )?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_fs() -> EntityFs {
        EntityFs::new("entity.toml", "entity.lock")
    }

    #[test]
    fn test_entity_store_open() {
        let tmp = tempfile::tempdir().unwrap();
        let store = EntityStore::open(tmp.path(), test_fs()).unwrap();
        assert!(store.index_root.exists());
    }

    #[test]
    fn test_empty_list_and_search() {
        let tmp = tempfile::tempdir().unwrap();
        let store = EntityStore::open(tmp.path(), test_fs()).unwrap();

        let indexed = store.list_indexed().unwrap();
        assert!(indexed.is_empty());

        let results = store.search("nonexistent", 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn test_scan_empty_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let store = EntityStore::open(tmp.path(), test_fs()).unwrap();
        let report = store.scan(false).unwrap();
        assert_eq!(report.integrated, 0);
        assert_eq!(report.pruned, 0);
    }

    #[test]
    fn test_query_ordering_comparisons() {
        use chrono::TimeZone;
        use crate::model::entity::EntityManifest;
        use serde_json::json;

        let tmp = tempfile::tempdir().unwrap();
        let store = EntityStore::open(tmp.path(), test_fs()).unwrap();
        let entity_dir = tmp.path().join("entities");
        std::fs::create_dir_all(&entity_dir).unwrap();

        let id1 = Uuid::new_v4();
        let date1 = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
        let mut manifest1 = EntityManifest::new(id1, date1);
        manifest1.extra.insert("type".to_string(), json!("tracker-improvement"));
        manifest1.extra.insert("title".to_string(), json!("Low effort, early date"));
        manifest1.extra.insert("state".to_string(), json!("ready"));
        manifest1.extra.insert("effort".to_string(), json!("3"));
        store.fs.create(&manifest1, &entity_dir, Some("Description 1")).unwrap();

        let id2 = Uuid::new_v4();
        let date2 = Utc.with_ymd_and_hms(2026, 6, 5, 12, 0, 0).unwrap();
        let mut manifest2 = EntityManifest::new(id2, date2);
        manifest2.extra.insert("type".to_string(), json!("tracker-improvement"));
        manifest2.extra.insert("title".to_string(), json!("Medium effort, mid date"));
        manifest2.extra.insert("state".to_string(), json!("in-implementation"));
        manifest2.extra.insert("effort".to_string(), json!("5"));
        store.fs.create(&manifest2, &entity_dir, Some("Description 2")).unwrap();

        let id3 = Uuid::new_v4();
        let date3 = Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap();
        let mut manifest3 = EntityManifest::new(id3, date3);
        manifest3.extra.insert("type".to_string(), json!("tracker-improvement"));
        manifest3.extra.insert("title".to_string(), json!("High effort, late date"));
        manifest3.extra.insert("state".to_string(), json!("done"));
        manifest3.extra.insert("effort".to_string(), json!("8"));
        store.fs.create(&manifest3, &entity_dir, Some("Description 3")).unwrap();

        // Scan and index them all!
        store.scan(true).unwrap();

        // Query 1: effort gt 3 (should return id2 and id3)
        let results = store.search("effort:>3", 10).unwrap();
        assert_eq!(results.len(), 2);
        let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        assert!(ids.contains(&id2));
        assert!(ids.contains(&id3));

        // Query 2: effort lte 5 (should return id1 and id2)
        let results = store.search("effort:<=5", 10).unwrap();
        assert_eq!(results.len(), 2);
        let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));

        // Query 3: effort range [4 TO 8] (should return id2 and id3)
        let results = store.search("effort:[4 TO 8]", 10).unwrap();
        assert_eq!(results.len(), 2);
        let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        assert!(ids.contains(&id2));
        assert!(ids.contains(&id3));

        // Query 4: created_at gt mid date (should return id3)
        let results = store.search("created_at:>2026-06-05T12:00:00Z", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id3);

        // Query 5: created_at range (should return id1 and id2)
        let results = store.search("created_at:[2026-06-01T00:00:00Z TO 2026-06-06T00:00:00Z]", 10).unwrap();
        assert_eq!(results.len(), 2);
        let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }
}
