use std::collections::BTreeMap;
use std::path::Path;
use std::{collections::VecDeque, fs};

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use memory_api::error::StorageError;
use memory_api::model::entity::EntityManifest;
use memory_api::storage::entity_fs::EntityFs;
use memory_api::storage::entity_store::{EntityStore, ScanReport};
use memory_api::storage::indexed::IndexedEntity;

use crate::error::SpecError;
use crate::manifest::{SpecId, SpecManifest};
use crate::slug::SlugIndex;

/// Spec filesystem configuration constants.
const SPEC_MANIFEST_FILE: &str = "spec.toml";
const SPEC_LOCK_FILE: &str = ".spec-lock";

/// The central spec store: wraps `EntityStore` with spec-specific features.
///
/// Adds slug uniqueness enforcement, `body.md` management, `sections/` CRUD,
/// and parent-child hierarchy traversal on top of the generic entity store.
pub struct SpecStore {
    inner: EntityStore,
    slug_index: SlugIndex,
}

impl SpecStore {
    /// Open a SpecStore at `index_root`.
    pub fn open(index_root: &Path) -> Result<Self, SpecError> {
        let fs = EntityFs::new(SPEC_MANIFEST_FILE, SPEC_LOCK_FILE);
        let registry = crate::default_schema::spec_schema_registry();
        let inner = EntityStore::open_with(index_root, fs, registry)?;
        Ok(Self {
            inner,
            slug_index: SlugIndex::new(),
        })
    }

    /// Access the underlying EntityStore.
    pub fn entity_store(&self) -> &EntityStore {
        &self.inner
    }

    // ── Scan & index ────────────────────────────────────────────────

    /// Scan all roots and rebuild the slug index.
    pub fn scan(&mut self, reindex: bool) -> Result<ScanReport, SpecError> {
        let report = self.inner.scan(reindex)?;
        self.rebuild_slug_index()?;
        Ok(report)
    }

    /// Rebuild the SlugIndex from the redb index.
    fn rebuild_slug_index(&mut self) -> Result<(), SpecError> {
        let all = self.inner.list_indexed(false)?;
        let entries = all.iter().filter_map(|e| {
            let manifest = self.inner.fs.read(&e.path).ok()?;
            let slug = manifest.extra.get("slug")?.as_str()?.to_string();
            Some((slug, e.id))
        });
        self.slug_index = SlugIndex::rebuild(entries)?;
        Ok(())
    }

    // ── Resolution ──────────────────────────────────────────────────

    /// Resolve an ID (full UUID, UUID prefix, or slug) to a UUID.
    pub fn resolve_id(&self, id_or_slug: &str) -> Result<Uuid, SpecError> {
        // Try full UUID
        if let Ok(uuid) = id_or_slug.parse::<Uuid>() {
            return Ok(uuid);
        }
        // Try UUID prefix match
        if let Some(uuid) = self.resolve_prefix(id_or_slug)? {
            return Ok(uuid);
        }
        // Try slug resolution
        self.slug_index
            .resolve(id_or_slug)
            .ok_or_else(|| SpecError::NotFound(id_or_slug.to_string()))
    }

    /// Resolve a UUID prefix to a full UUID. Returns `None` if no match,
    /// error if ambiguous (multiple matches).
    fn resolve_prefix(&self, prefix: &str) -> Result<Option<Uuid>, SpecError> {
        if prefix.len() < 4 {
            return Ok(None);
        }
        let all = self
            .inner
            .list_indexed(false)
            .map_err(SpecError::Storage)?;
        let matches: Vec<_> = all
            .iter()
            .filter(|e| e.id.to_string().starts_with(prefix))
            .collect();
        match matches.len() {
            0 => Ok(None),
            1 => Ok(Some(matches[0].id)),
            _ => Err(SpecError::NotFound(format!(
                "ambiguous prefix '{}' matches {} specs",
                prefix,
                matches.len()
            ))),
        }
    }

    // ── CRUD ────────────────────────────────────────────────────────

    /// Create a new spec.
    pub fn create(
        &mut self,
        manifest: &SpecManifest,
        body: &str,
        target_root: Option<&Path>,
    ) -> Result<SpecId, SpecError> {
        // Validate slug
        let slug = manifest
            .slug()
            .ok_or_else(|| SpecError::InvalidSlug("missing slug".into()))?;
        crate::slug::validate_slug(slug)?;

        // Check slug uniqueness
        if let Some(existing) = self.slug_index.resolve(slug) {
            if existing != manifest.id {
                return Err(SpecError::DuplicateSlug(slug.to_string()));
            }
        }

        // Resolve target root
        let root = match target_root {
            Some(p) => p.to_path_buf(),
            None => {
                let roots = self.inner.list_scan_roots()?;
                roots
                    .into_iter()
                    .next()
                    .map(|r| r.path)
                    .unwrap_or_else(|| self.inner.index_root.join("specs"))
            }
        };
        fs::create_dir_all(&root).map_err(StorageError::Io)?;

        // Convert SpecManifest → EntityManifest and create folder via EntityFs
        let entity = spec_to_entity(manifest);
        let folder = self.inner.fs.create(&entity, &root, Some(body))?;

        // Rename description.md → body.md (EntityFs writes description.md)
        let desc_path = folder.join("description.md");
        let body_path = folder.join("body.md");
        if desc_path.exists() {
            fs::rename(&desc_path, &body_path).map_err(StorageError::Io)?;
        }

        // Index the entity in redb + search
        let type_id = manifest
            .extra
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("specification")
            .to_string();
        let title = manifest.title().map(String::from);
        let state = manifest.state().map(String::from);
        let now = Utc::now();

        let indexed = IndexedEntity {
            id: manifest.id,
            path: folder.clone(),
            type_id: type_id.clone(),
            title: title.clone(),
            state: state.clone(),
            created_at: manifest.created_at,
            updated_at: now,
            deleted: false,
        };
        self.inner.index.insert_ticket(&indexed)?;
        self.inner.search.upsert(
            &manifest.id,
            title.as_deref(),
            Some(body),
            state.as_deref(),
            Some(&type_id),
        )?;

        // Register slug
        self.slug_index.insert(slug.to_string(), manifest.id)?;

        // Append initial history
        let _ = self
            .inner
            .fs
            .append_history(&folder, entity.extra.clone(), None);

        Ok(manifest.id)
    }

    /// Get a spec manifest by ID or slug.
    pub fn get(&self, id_or_slug: &str) -> Result<SpecManifest, SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        if indexed.deleted {
            return Err(SpecError::NotFound(uuid.to_string()));
        }
        let spec = read_spec_manifest(&indexed.path)?;
        Ok(spec)
    }

    /// Get manifest + body.
    pub fn get_full(&self, id_or_slug: &str) -> Result<(SpecManifest, String), SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        if indexed.deleted {
            return Err(SpecError::NotFound(uuid.to_string()));
        }
        let spec = read_spec_manifest(&indexed.path)?;
        let body = read_body(&indexed.path);
        Ok((spec, body))
    }

    // ── Update ──────────────────────────────────────────────────────

    /// Update manifest fields and/or state.
    pub fn update(
        &mut self,
        id_or_slug: &str,
        patch: BTreeMap<String, Value>,
        to_state: Option<&str>,
    ) -> Result<SpecManifest, SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;

        // If slug is changing, validate + update slug index
        if let Some(new_slug_val) = patch.get("slug") {
            if let Some(new_slug) = new_slug_val.as_str() {
                crate::slug::validate_slug(new_slug)?;
                // Remove old slug, insert new
                let old = self.inner.fs.read(&indexed.path)?;
                if let Some(old_slug) = old.extra.get("slug").and_then(|v| v.as_str()) {
                    self.slug_index.remove(old_slug);
                }
                self.slug_index.insert(new_slug.to_string(), uuid)?;
            }
        }

        // State transition validation via schema registry
        if let Some(to) = to_state {
            let current = indexed.state.as_deref().unwrap_or("draft");
            if let Some(schema) = self.inner.schema_registry().get("specification") {
                schema.ensure_transition(current, to)?;
            }
        }

        let updated_entity = self.inner.fs.update(&indexed.path, &patch, to_state)?;

        // Update redb + search indexes
        let type_id = updated_entity
            .extra
            .get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("specification")
            .to_string();
        let title = updated_entity
            .extra
            .get("title")
            .and_then(|v| v.as_str())
            .map(String::from);
        let state = updated_entity
            .extra
            .get("state")
            .and_then(|v| v.as_str())
            .map(String::from);

        let refreshed = IndexedEntity {
            id: uuid,
            path: indexed.path.clone(),
            type_id: type_id.clone(),
            title: title.clone(),
            state: state.clone(),
            created_at: indexed.created_at,
            updated_at: Utc::now(),
            deleted: false,
        };
        self.inner.index.insert_ticket(&refreshed)?;

        let body = read_body(&indexed.path);
        self.inner.search.upsert(
            &uuid,
            title.as_deref(),
            Some(&body),
            state.as_deref(),
            Some(&type_id),
        )?;

        // Append history
        let _ = self.inner.fs.append_history(
            &indexed.path,
            updated_entity.extra.clone(),
            None,
        );

        Ok(entity_to_spec(&updated_entity))
    }

    /// Update body.md content.
    pub fn update_body(&self, id_or_slug: &str, content: &str) -> Result<(), SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        write_body(&indexed.path, content)?;
        Ok(())
    }

    /// Soft-delete a spec.
    pub fn delete(&mut self, id_or_slug: &str) -> Result<(), SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        // Remove slug
        let entity = self.inner.fs.read(&indexed.path)?;
        if let Some(slug) = entity.extra.get("slug").and_then(|v| v.as_str()) {
            self.slug_index.remove(slug);
        }
        self.inner.fs.mark_deleted(&indexed.path)?;

        // Update index to mark deleted
        let mut refreshed = indexed.clone();
        refreshed.deleted = true;
        refreshed.updated_at = Utc::now();
        self.inner.index.insert_ticket(&refreshed)?;

        Ok(())
    }

    // ── Section CRUD ────────────────────────────────────────────────

    /// Add a section file.
    pub fn add_section(
        &self,
        id_or_slug: &str,
        name: &str,
        content: &str,
    ) -> Result<(), SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        let sections_dir = indexed.path.join("sections");
        fs::create_dir_all(&sections_dir).map_err(StorageError::Io)?;
        let file_name = normalize_section_name(name);
        fs::write(sections_dir.join(&file_name), content).map_err(StorageError::Io)?;
        Ok(())
    }

    /// Update a section file.
    pub fn update_section(
        &self,
        id_or_slug: &str,
        name: &str,
        content: &str,
    ) -> Result<(), SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        let file_name = normalize_section_name(name);
        let path = indexed.path.join("sections").join(&file_name);
        if !path.exists() {
            return Err(SpecError::NotFound(format!("section: {}", name)));
        }
        fs::write(&path, content).map_err(StorageError::Io)?;
        Ok(())
    }

    /// Delete a section file.
    pub fn delete_section(&self, id_or_slug: &str, name: &str) -> Result<(), SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        let file_name = normalize_section_name(name);
        let path = indexed.path.join("sections").join(&file_name);
        if path.exists() {
            fs::remove_file(&path).map_err(StorageError::Io)?;
        }
        Ok(())
    }

    /// List section file names (sorted).
    pub fn list_sections(&self, id_or_slug: &str) -> Result<Vec<String>, SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        let sections_dir = indexed.path.join("sections");
        if !sections_dir.exists() {
            return Ok(Vec::new());
        }
        let mut names: Vec<String> = fs::read_dir(&sections_dir)
            .map_err(StorageError::Io)?
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| n.ends_with(".md"))
            .collect();
        names.sort();
        Ok(names)
    }

    // ── Hierarchy ───────────────────────────────────────────────────

    /// Get direct children of a spec (specs whose `parent` field == this spec's UUID).
    pub fn children(&self, id_or_slug: &str) -> Result<Vec<SpecManifest>, SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let uuid_str = uuid.to_string();
        let all = self.inner.list_indexed(false)?;
        let mut children = Vec::new();
        for indexed in &all {
            if let Ok(spec) = read_spec_manifest(&indexed.path) {
                if spec.parent() == Some(&uuid_str) {
                    children.push(spec);
                }
            }
        }
        Ok(children)
    }

    /// Walk the parent chain from a spec up to the root.
    pub fn ancestors(&self, id_or_slug: &str) -> Result<Vec<SpecManifest>, SpecError> {
        let mut result = Vec::new();
        let mut current = self.get(id_or_slug)?;
        while let Some(parent_str) = current.parent().map(String::from) {
            let parent = self.get(&parent_str)?;
            result.push(parent.clone());
            current = parent;
        }
        Ok(result)
    }

    /// BFS all descendants of a spec.
    pub fn subtree(&self, id_or_slug: &str) -> Result<Vec<SpecManifest>, SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let all = self.inner.list_indexed(false)?;
        let mut result = Vec::new();
        let mut queue = VecDeque::new();
        queue.push_back(uuid);
        while let Some(current_id) = queue.pop_front() {
            let current_str = current_id.to_string();
            for indexed in &all {
                if let Ok(spec) = read_spec_manifest(&indexed.path) {
                    if spec.parent() == Some(&current_str) {
                        queue.push_back(spec.id);
                        result.push(spec);
                    }
                }
            }
        }
        Ok(result)
    }
}

// ── Conversion helpers ──────────────────────────────────────────────────────

fn spec_to_entity(spec: &SpecManifest) -> EntityManifest {
    let mut extra = spec.extra.clone();
    // Persist code_refs into extra if non-empty
    if !spec.code_refs.is_empty() {
        if let Ok(refs_val) = serde_json::to_value(&spec.code_refs) {
            extra.insert("code_refs".to_string(), refs_val);
        }
    }
    EntityManifest {
        id: spec.id,
        created_at: spec.created_at,
        extra,
    }
}

fn entity_to_spec(entity: &EntityManifest) -> SpecManifest {
    let mut extra = entity.extra.clone();
    // Extract code_refs from extra if present
    let code_refs = extra
        .remove("code_refs")
        .and_then(|v| serde_json::from_value(v).ok())
        .unwrap_or_default();
    SpecManifest {
        id: entity.id,
        created_at: entity.created_at,
        code_refs,
        extra,
    }
}

/// Read a spec.toml directly into SpecManifest (preserves code_refs).
fn read_spec_manifest(spec_path: &Path) -> Result<SpecManifest, SpecError> {
    let manifest_path = spec_path.join(SPEC_MANIFEST_FILE);
    let content = fs::read_to_string(&manifest_path)
        .map_err(|e| SpecError::Storage(StorageError::Io(e)))?;
    let spec: SpecManifest =
        toml::from_str(&content).map_err(|e| SpecError::Serialization(e.to_string()))?;
    Ok(spec)
}

fn normalize_section_name(name: &str) -> String {
    if name.ends_with(".md") {
        name.to_string()
    } else {
        format!("{}.md", name)
    }
}

fn read_body(spec_path: &Path) -> String {
    let body_path = spec_path.join("body.md");
    fs::read_to_string(&body_path).unwrap_or_default()
}

fn write_body(spec_path: &Path, content: &str) -> Result<(), SpecError> {
    let body_path = spec_path.join("body.md");
    fs::write(&body_path, content).map_err(|e| SpecError::Storage(StorageError::Io(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_api::model::filesystem::ScanRoot;
    use tempfile::TempDir;

    fn setup() -> (TempDir, SpecStore) {
        let tmp = TempDir::new().unwrap();
        let mut store = SpecStore::open(tmp.path()).unwrap();
        let root = tmp.path().join("specs");
        fs::create_dir_all(&root).unwrap();
        store
            .entity_store()
            .add_scan_root(ScanRoot {
                path: root,
                label: "test".into(),
            })
            .unwrap();
        (tmp, store)
    }

    #[test]
    fn test_create_and_get() {
        let (_tmp, mut store) = setup();
        let manifest = SpecManifest::new("test/example", "Example Spec", "test-component");
        let id = store
            .create(&manifest, "# Example\n\nBody text.", None)
            .unwrap();

        let retrieved = store.get(&id.to_string()).unwrap();
        assert_eq!(retrieved.slug(), Some("test/example"));
        assert_eq!(retrieved.title(), Some("Example Spec"));
    }

    #[test]
    fn test_get_full_with_body() {
        let (_tmp, mut store) = setup();
        let manifest = SpecManifest::new("test/body", "Body Test", "comp");
        store.create(&manifest, "Hello body", None).unwrap();

        let (_spec, body) = store.get_full("test/body").unwrap();
        assert_eq!(body, "Hello body");
    }

    #[test]
    fn test_slug_resolution() {
        let (_tmp, mut store) = setup();
        let manifest = SpecManifest::new("api/storage", "Storage", "api");
        let id = store.create(&manifest, "body", None).unwrap();

        // Resolve by slug
        assert_eq!(store.resolve_id("api/storage").unwrap(), id);
        // Resolve by full UUID
        assert_eq!(store.resolve_id(&id.to_string()).unwrap(), id);
    }

    #[test]
    fn test_duplicate_slug_rejected() {
        let (_tmp, mut store) = setup();
        let m1 = SpecManifest::new("unique/slug", "Spec 1", "comp");
        store.create(&m1, "body", None).unwrap();

        let m2 = SpecManifest::new("unique/slug", "Spec 2", "comp");
        assert!(store.create(&m2, "body", None).is_err());
    }

    #[test]
    fn test_update_body() {
        let (_tmp, mut store) = setup();
        let manifest = SpecManifest::new("test/update", "Update", "comp");
        store.create(&manifest, "initial", None).unwrap();

        store.update_body("test/update", "updated body").unwrap();
        let (_, body) = store.get_full("test/update").unwrap();
        assert_eq!(body, "updated body");
    }

    #[test]
    fn test_sections_crud() {
        let (_tmp, mut store) = setup();
        let manifest = SpecManifest::new("test/sections", "Sections", "comp");
        store.create(&manifest, "body", None).unwrap();

        store
            .add_section("test/sections", "overview", "# Overview")
            .unwrap();
        store
            .add_section("test/sections", "details", "# Details")
            .unwrap();

        let sections = store.list_sections("test/sections").unwrap();
        assert_eq!(sections, vec!["details.md", "overview.md"]);

        store
            .update_section("test/sections", "overview", "# Updated Overview")
            .unwrap();
        store.delete_section("test/sections", "details").unwrap();

        let sections = store.list_sections("test/sections").unwrap();
        assert_eq!(sections, vec!["overview.md"]);
    }

    #[test]
    fn test_parent_child_hierarchy() {
        let (_tmp, mut store) = setup();
        let parent = SpecManifest::new("api/parent", "Parent", "api");
        let parent_id = store.create(&parent, "parent body", None).unwrap();

        let mut child = SpecManifest::new("api/child", "Child", "api");
        child.set_parent(&parent_id.to_string());
        store.create(&child, "child body", None).unwrap();

        let children = store.children(&parent_id.to_string()).unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].slug(), Some("api/child"));

        let ancestors = store.ancestors("api/child").unwrap();
        assert_eq!(ancestors.len(), 1);
        assert_eq!(ancestors[0].slug(), Some("api/parent"));
    }

    #[test]
    fn test_delete_removes_slug() {
        let (_tmp, mut store) = setup();
        let manifest = SpecManifest::new("test/delete", "Delete", "comp");
        store.create(&manifest, "body", None).unwrap();

        store.delete("test/delete").unwrap();
        assert!(store.get("test/delete").is_err());
    }

    #[test]
    fn test_update_manifest_fields() {
        let (_tmp, mut store) = setup();
        let manifest = SpecManifest::new("test/updatable", "Original Title", "comp");
        store.create(&manifest, "body", None).unwrap();

        let mut patch = BTreeMap::new();
        patch.insert(
            "title".to_string(),
            Value::String("Updated Title".to_string()),
        );
        let updated = store.update("test/updatable", patch, None).unwrap();
        assert_eq!(updated.title(), Some("Updated Title"));
    }

    #[test]
    fn test_update_slug() {
        let (_tmp, mut store) = setup();
        let manifest = SpecManifest::new("test/old-slug", "Slug Change", "comp");
        let id = store.create(&manifest, "body", None).unwrap();

        let mut patch = BTreeMap::new();
        patch.insert(
            "slug".to_string(),
            Value::String("test/new-slug".to_string()),
        );
        store.update("test/old-slug", patch, None).unwrap();

        // Old slug should fail
        assert!(store.resolve_id("test/old-slug").is_err());
        // New slug should work
        assert_eq!(store.resolve_id("test/new-slug").unwrap(), id);
    }

    #[test]
    fn test_subtree() {
        let (_tmp, mut store) = setup();
        let root = SpecManifest::new("tree/root", "Root", "comp");
        let root_id = store.create(&root, "root body", None).unwrap();

        let mut child1 = SpecManifest::new("tree/child1", "Child 1", "comp");
        child1.set_parent(&root_id.to_string());
        let child1_id = store.create(&child1, "child1 body", None).unwrap();

        let mut grandchild = SpecManifest::new("tree/grandchild", "Grandchild", "comp");
        grandchild.set_parent(&child1_id.to_string());
        store.create(&grandchild, "grandchild body", None).unwrap();

        let tree = store.subtree(&root_id.to_string()).unwrap();
        assert_eq!(tree.len(), 2);
    }

    #[test]
    fn test_scan_rebuilds_slug_index() {
        let (_tmp, mut store) = setup();
        let manifest = SpecManifest::new("scan/test", "Scan Test", "comp");
        let id = store.create(&manifest, "body", None).unwrap();

        // Clear the in-memory slug index and rebuild via scan
        store.slug_index = SlugIndex::new();
        assert!(store.resolve_id("scan/test").is_err());

        store.scan(true).unwrap();
        assert_eq!(store.resolve_id("scan/test").unwrap(), id);
    }
}
