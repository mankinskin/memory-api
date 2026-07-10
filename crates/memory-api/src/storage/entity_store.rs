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
        // Proactively guarantee the search index is valid and complete before a
        // read, healing structural corruption and repopulating from disk if the
        // index is empty or partial.
        self.ensure_search_ready()?;
        let expr = parse_query(query_expr)?;

        match self.search.search(&expr, limit) {
            Ok(results) => Ok(results),
            // Deep segment-content corruption keeps `meta.json` valid, so it
            // passes the cheap structural and completeness checks above and only
            // surfaces here when the searcher reads a segment. This is the one
            // class of damage that cannot be detected without reading every
            // segment, so it is the sole case repaired reactively: rebuild from
            // the filesystem source of truth and retry the read once.
            Err(error)
                if TantivySearchIndex::is_rebuildable_read_failure(&error) =>
            {
                self.search.reset_dir()?;
                self.scan_once(true)?;
                self.search.search(&expr, limit)
            },
            Err(error) => Err(error),
        }
    }

    /// Enforce the search-index readiness invariants before a read.
    ///
    /// Structural invariants (directory present, index openable, schema
    /// current) are healed inside [`TantivySearchIndex::num_docs`] /
    /// [`TantivySearchIndex::open_index`]. The **completeness** invariant —
    /// every entity in the metadata index has a search document — is then
    /// checked by comparing document counts; a mismatch (for example an empty
    /// index left by a structural rebuild) is repaired by re-indexing from the
    /// filesystem source of truth.
    fn ensure_search_ready(&self) -> Result<(), StorageError> {
        if self.search_needs_rebuild()? {
            self.scan_once(true)?;
        }
        Ok(())
    }

    /// Whether the search index must be rebuilt before it can be trusted.
    ///
    /// Returns `true` when the search index cannot be opened/counted (structural
    /// or segment-content corruption — [`TantivySearchIndex::num_docs`] errors)
    /// or when its document count differs from the metadata index (the
    /// filesystem-backed source of truth, which survives Tantivy corruption).
    /// Calling this also heals the cheap structural invariants, so a stale or
    /// empty index reports as needing a rebuild.
    fn search_needs_rebuild(&self) -> Result<bool, StorageError> {
        let indexed = self.index.list_tickets()?.len() as u64;
        match self.search.num_docs() {
            Ok(docs) => Ok(docs != indexed),
            Err(_) => Ok(true),
        }
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
    /// When `reindex` is `true` (or the search index is missing, partial, or
    /// corrupt), the search directory is **reset** and rebuilt from scratch
    /// before scanning, and stale SQLite entries are pruned.
    ///
    /// Resetting the directory (rather than clearing documents from the existing
    /// index) makes a forced rebuild robust against any on-disk corruption —
    /// including a stale Tantivy schema whose fast-field layout would otherwise
    /// panic the writer, or truncated/missing segment files that cannot be
    /// opened. The index is then repopulated from the filesystem source of
    /// truth, so the completeness invariant is restored.
    pub fn scan(
        &self,
        reindex: bool,
    ) -> Result<ScanReport, StorageError> {
        // Proactively enforce all search-index invariants before writing. The
        // rebuild check heals structural corruption (via `num_docs`) and detects
        // an empty/partial/unreadable index; either condition forces a full
        // rebuild from the filesystem.
        let force = reindex || self.search_needs_rebuild()?;
        self.scan_once(force)
    }

    fn scan_once(
        &self,
        reindex: bool,
    ) -> Result<ScanReport, StorageError> {
        if reindex {
            // Reset the directory instead of clearing documents: a forced
            // rebuild must not depend on opening the (possibly corrupt) existing
            // index. The next upsert recreates a fresh index from the current
            // schema.
            self.search.reset_dir()?;
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

    /// Re-integrate a single entity folder into the metadata index **and** the
    /// full-text search index.
    ///
    /// This is the per-entry counterpart to [`Self::scan`]: instead of walking
    /// every scan root, it reconciles just the entity located at `path`. It is
    /// used by the filesystem watcher so any change to a filesystem entry
    /// immediately refreshes that entry's search-index document.
    ///
    /// Returns `Ok(true)` when the entity was integrated, or `Ok(false)` when
    /// `path` is not a valid entity folder (non-UUID name or unreadable
    /// manifest).
    pub fn integrate_orphan(
        &self,
        path: &Path,
    ) -> Result<bool, StorageError> {
        let id: Uuid = match path
            .file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.parse().ok())
        {
            Some(id) => id,
            None => return Ok(false),
        };

        let manifest = match self.fs.read(path) {
            Ok(manifest) => manifest,
            Err(_) => return Ok(false),
        };

        // Proactively heal the search index before a single-entity write so a
        // corrupt/stale/empty index does not lose the other entities'
        // documents. If the index needed a rebuild, the full repopulation
        // already integrated this entity.
        if self.search_needs_rebuild()? {
            self.scan_once(true)?;
            return Ok(true);
        }

        let entry = EntityScanEntry {
            id,
            path: path.to_path_buf(),
            manifest,
        };
        self.integrate_entry(entry, true)?;
        Ok(true)
    }

    /// Remove a single entity from the metadata index and the search index.
    ///
    /// Used by the watcher when an entity folder is deleted from disk.
    pub fn remove_entity(
        &self,
        id: &Uuid,
    ) -> Result<(), StorageError> {
        self.index.remove_ticket(id)?;
        self.search.remove(id)?;
        Ok(())
    }

    fn integrate_entry(
        &self,
        entry: EntityScanEntry,
        update_search: bool,
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

        if update_search {
            let body = self.fs.read_description(&entry.path);
            let created_at_str = indexed.created_at.to_rfc3339();
            let effort_str =
                entry.manifest.extra.get("effort").and_then(|v| match v {
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
    fn scan_reindex_self_heals_stale_search_index_schema() {
        use crate::model::entity::EntityManifest;
        use serde_json::json;
        use tantivy::schema::{
            FAST,
            STORED,
            STRING,
            Schema,
            TEXT,
        };

        let tmp = tempfile::tempdir().unwrap();
        let store = EntityStore::open(tmp.path(), test_fs()).unwrap();
        let entity_dir = tmp.path().join("entities");
        std::fs::create_dir_all(&entity_dir).unwrap();

        let id = Uuid::new_v4();
        let mut manifest = EntityManifest::new(id, Utc::now());
        manifest
            .extra
            .insert("type".to_string(), json!("rule-entry"));
        manifest
            .extra
            .insert("title".to_string(), json!("Stale schema heals"));
        manifest.extra.insert("state".to_string(), json!("ready"));
        store
            .fs
            .create(&manifest, &entity_dir, Some("searchable body text"))
            .unwrap();

        // Replace the freshly-built (current-schema) index with one built from
        // an OLDER 5-field schema (before `created_at` and `effort` were added).
        // Writing a document that references the newer field ids would make the
        // Tantivy fast-field writer panic on a background thread; the scan must
        // detect this, reset the search dir, and rebuild from the current schema.
        let search_dir = tmp.path().join("search_index");
        std::fs::remove_dir_all(&search_dir).unwrap();
        std::fs::create_dir_all(&search_dir).unwrap();
        let mut builder = Schema::builder();
        builder.add_text_field("id", STRING | STORED);
        builder.add_text_field("title", TEXT | STORED);
        builder.add_text_field("body", TEXT | STORED);
        builder.add_text_field("state", STRING | STORED | FAST);
        builder.add_text_field("ticket_type", STRING | STORED | FAST);
        tantivy::Index::create_in_dir(&search_dir, builder.build()).unwrap();

        // Must not panic; self-heals and indexes the entity.
        store.scan(true).unwrap();

        let results = store.search("searchable", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id);
    }

    /// A read must proactively heal and repopulate the search index across the
    /// full corruption matrix — without any manual `scan` — because the search
    /// index is a derived cache rebuildable from the filesystem source of truth.
    #[test]
    fn search_heals_every_corruption_mode_without_manual_scan() {
        use crate::model::entity::EntityManifest;
        use serde_json::json;

        let tmp = tempfile::tempdir().unwrap();
        let store = EntityStore::open(tmp.path(), test_fs()).unwrap();
        let entity_dir = tmp.path().join("entities");
        std::fs::create_dir_all(&entity_dir).unwrap();

        // Seed three searchable entities.
        let mut ids = Vec::new();
        for (n, body) in [
            ("needle alpha document", "alpha"),
            ("needle beta document", "beta"),
            ("needle gamma document", "gamma"),
        ] {
            let id = Uuid::new_v4();
            let mut manifest = EntityManifest::new(id, Utc::now());
            manifest.extra.insert("type".into(), json!("rule-entry"));
            manifest.extra.insert("title".into(), json!(n));
            manifest.extra.insert("state".into(), json!("ready"));
            store.fs.create(&manifest, &entity_dir, Some(body)).unwrap();
            ids.push(id);
        }
        store.scan(true).unwrap();
        assert_eq!(store.search("needle", 10).unwrap().len(), 3);

        let search_dir = tmp.path().join("search_index");
        let list_files = |ext: &str| -> Vec<std::path::PathBuf> {
            std::fs::read_dir(&search_dir)
                .unwrap()
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().and_then(|x| x.to_str()) == Some(ext))
                .collect()
        };

        // 1. Corrupt meta.json (unopenable index).
        std::fs::write(search_dir.join("meta.json"), b"not valid json")
            .unwrap();
        assert_eq!(store.search("needle", 10).unwrap().len(), 3);

        // 2. Wipe the whole index directory.
        std::fs::remove_dir_all(&search_dir).unwrap();
        assert_eq!(store.search("needle", 10).unwrap().len(), 3);

        // 3. Truncate a segment store file (segment-content corruption that
        //    keeps meta.json valid — only surfaces on read).
        for f in list_files("store") {
            std::fs::write(&f, b"x").unwrap();
        }
        assert_eq!(store.search("needle", 10).unwrap().len(), 3);

        // 4. Delete the term dictionary files.
        for f in list_files("term") {
            std::fs::remove_file(&f).unwrap();
        }
        assert_eq!(store.search("needle", 10).unwrap().len(), 3);

        // 5. Overwrite a fast-field file with garbage.
        for f in list_files("fast") {
            std::fs::write(&f, b"zzzzz").unwrap();
        }
        assert_eq!(store.search("needle", 10).unwrap().len(), 3);

        // The index is healthy and every seeded entity is searchable again.
        let healed: std::collections::HashSet<_> = store
            .search("needle", 10)
            .unwrap()
            .into_iter()
            .map(|r| r.id)
            .collect();
        for id in ids {
            assert!(healed.contains(&id));
        }
    }

    /// A forced reindex must succeed even when the existing index is corrupt,
    /// because a rebuild resets the directory rather than opening the index.
    #[test]
    fn scan_reindex_succeeds_over_corrupt_index() {
        use crate::model::entity::EntityManifest;
        use serde_json::json;

        let tmp = tempfile::tempdir().unwrap();
        let store = EntityStore::open(tmp.path(), test_fs()).unwrap();
        let entity_dir = tmp.path().join("entities");
        std::fs::create_dir_all(&entity_dir).unwrap();

        let id = Uuid::new_v4();
        let mut manifest = EntityManifest::new(id, Utc::now());
        manifest.extra.insert("type".into(), json!("rule-entry"));
        manifest
            .extra
            .insert("title".into(), json!("Corrupt reindex"));
        store
            .fs
            .create(&manifest, &entity_dir, Some("searchable body"))
            .unwrap();
        store.scan(true).unwrap();

        // Corrupt every segment file, then force a reindex.
        let search_dir = tmp.path().join("search_index");
        for entry in std::fs::read_dir(&search_dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_file() {
                std::fs::write(&path, b"corrupt").unwrap();
            }
        }
        store.scan(true).unwrap();

        let results = store.search("searchable", 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id);
    }

    #[test]
    fn test_query_ordering_comparisons() {
        use crate::model::entity::EntityManifest;
        use chrono::TimeZone;
        use serde_json::json;

        let tmp = tempfile::tempdir().unwrap();
        let store = EntityStore::open(tmp.path(), test_fs()).unwrap();
        let entity_dir = tmp.path().join("entities");
        std::fs::create_dir_all(&entity_dir).unwrap();

        let id1 = Uuid::new_v4();
        let date1 = Utc.with_ymd_and_hms(2026, 6, 1, 12, 0, 0).unwrap();
        let mut manifest1 = EntityManifest::new(id1, date1);
        manifest1
            .extra
            .insert("type".to_string(), json!("tracker-improvement"));
        manifest1
            .extra
            .insert("title".to_string(), json!("Low effort, early date"));
        manifest1.extra.insert("state".to_string(), json!("ready"));
        manifest1.extra.insert("effort".to_string(), json!("3"));
        store
            .fs
            .create(&manifest1, &entity_dir, Some("Description 1"))
            .unwrap();

        let id2 = Uuid::new_v4();
        let date2 = Utc.with_ymd_and_hms(2026, 6, 5, 12, 0, 0).unwrap();
        let mut manifest2 = EntityManifest::new(id2, date2);
        manifest2
            .extra
            .insert("type".to_string(), json!("tracker-improvement"));
        manifest2
            .extra
            .insert("title".to_string(), json!("Medium effort, mid date"));
        manifest2
            .extra
            .insert("state".to_string(), json!("in-implementation"));
        manifest2.extra.insert("effort".to_string(), json!("5"));
        store
            .fs
            .create(&manifest2, &entity_dir, Some("Description 2"))
            .unwrap();

        let id3 = Uuid::new_v4();
        let date3 = Utc.with_ymd_and_hms(2026, 6, 10, 12, 0, 0).unwrap();
        let mut manifest3 = EntityManifest::new(id3, date3);
        manifest3
            .extra
            .insert("type".to_string(), json!("tracker-improvement"));
        manifest3
            .extra
            .insert("title".to_string(), json!("High effort, late date"));
        manifest3.extra.insert("state".to_string(), json!("done"));
        manifest3.extra.insert("effort".to_string(), json!("8"));
        store
            .fs
            .create(&manifest3, &entity_dir, Some("Description 3"))
            .unwrap();

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
        let results = store
            .search("created_at:>2026-06-05T12:00:00Z", 10)
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, id3);

        // Query 5: created_at range (should return id1 and id2)
        let results = store
            .search(
                "created_at:[2026-06-01T00:00:00Z TO 2026-06-06T00:00:00Z]",
                10,
            )
            .unwrap();
        assert_eq!(results.len(), 2);
        let ids: Vec<Uuid> = results.iter().map(|r| r.id).collect();
        assert!(ids.contains(&id1));
        assert!(ids.contains(&id2));
    }
}
