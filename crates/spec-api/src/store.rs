use std::{
    collections::BTreeMap,
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use memory_api::{
    error::StorageError,
    workspace,
    model::filesystem::ScanRoot,
    storage::{
        entity_fs::EntityFs,
        entity_store::{
            EntityStore,
            ScanReport,
        },
        indexed::IndexedEntity,
    },
};

use crate::{
    error::SpecError,
    manifest::{
        SpecId,
        SpecManifest,
    },
    slug::SlugIndex,
};

mod helpers;
mod hierarchy;
mod sections;

#[cfg(test)]
mod tests;

use self::helpers::{
    entity_to_spec,
    read_body,
    read_spec_manifest,
    spec_to_entity,
    write_body,
};

const SPEC_MANIFEST_FILE: &str = "spec.toml";
const SPEC_LOCK_FILE: &str = ".spec-lock";
const SPEC_INDEX_DIR: &str = ".spec";

pub struct SpecStore {
    inner: EntityStore,
    slug_index: SlugIndex,
}

impl SpecStore {
    pub fn open(index_root: &Path) -> Result<Self, SpecError> {
        let index_root =
            workspace::resolve_store_root_from(index_root, SPEC_INDEX_DIR);
        let fs = EntityFs::new(SPEC_MANIFEST_FILE, SPEC_LOCK_FILE);
        let registry = crate::default_schema::spec_schema_registry();
        let inner = EntityStore::open_with(&index_root, fs, registry)?;
        inner.add_scan_root(ScanRoot {
            path: index_root.join("specs"),
            label: "specs".to_string(),
        })?;
        Ok(Self {
            inner,
            slug_index: SlugIndex::new(),
        })
    }

    pub fn entity_store(&self) -> &EntityStore {
        &self.inner
    }

    pub fn scan(
        &mut self,
        reindex: bool,
    ) -> Result<ScanReport, SpecError> {
        let report = self.inner.scan(reindex)?;
        self.rebuild_slug_index()?;
        Ok(report)
    }

    fn rebuild_slug_index(&mut self) -> Result<(), SpecError> {
        let all = self.inner.list_indexed(false)?;
        let entries = all.iter().filter_map(|entry| {
            let manifest = self.inner.fs.read(&entry.path).ok()?;
            let slug = manifest.extra.get("slug")?.as_str()?.to_string();
            Some((slug, entry.id))
        });
        self.slug_index = SlugIndex::rebuild(entries)?;
        Ok(())
    }

    pub fn resolve_id(
        &self,
        id_or_slug: &str,
    ) -> Result<Uuid, SpecError> {
        if let Ok(uuid) = id_or_slug.parse::<Uuid>() {
            return Ok(uuid);
        }
        if let Some(uuid) = self.resolve_prefix(id_or_slug)? {
            return Ok(uuid);
        }
        self.slug_index
            .resolve(id_or_slug)
            .ok_or_else(|| SpecError::NotFound(id_or_slug.to_string()))
    }

    fn resolve_prefix(
        &self,
        prefix: &str,
    ) -> Result<Option<Uuid>, SpecError> {
        if prefix.len() < 4 {
            return Ok(None);
        }
        let all = self.inner.list_indexed(false).map_err(SpecError::Storage)?;
        let matches: Vec<_> = all
            .iter()
            .filter(|entry| entry.id.to_string().starts_with(prefix))
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

    pub fn create(
        &mut self,
        manifest: &SpecManifest,
        body: &str,
        target_root: Option<&Path>,
    ) -> Result<SpecId, SpecError> {
        let slug = manifest
            .slug()
            .ok_or_else(|| SpecError::InvalidSlug("missing slug".into()))?;
        crate::slug::validate_slug(slug)?;

        if let Some(existing) = self.slug_index.resolve(slug) {
            if existing != manifest.id {
                return Err(SpecError::DuplicateSlug(slug.to_string()));
            }
        }

        let root = self.resolve_target_root(target_root)?;
        fs::create_dir_all(&root).map_err(StorageError::Io)?;

        let entity = spec_to_entity(manifest);
        let folder = self.inner.fs.create(&entity, &root, Some(body))?;

        let desc_path = folder.join("description.md");
        let body_path = folder.join("body.md");
        if desc_path.exists() {
            fs::rename(&desc_path, &body_path).map_err(StorageError::Io)?;
        }

        let type_id = manifest
            .extra
            .get("type")
            .and_then(|value| value.as_str())
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

        self.slug_index.insert(slug.to_string(), manifest.id)?;

        let _ =
            self.inner
                .fs
                .append_history(&folder, entity.extra.clone(), None);

        Ok(manifest.id)
    }

    fn resolve_target_root(
        &self,
        target_root: Option<&Path>,
    ) -> Result<PathBuf, StorageError> {
        let roots = self.inner.list_scan_roots()?;

        let Some(target_root) = target_root else {
            return Ok(roots
                .into_iter()
                .next()
                .map(|root| root.path)
                .unwrap_or_else(|| self.inner.index_root.join("specs")));
        };

        let requested = if target_root.is_dir() {
            target_root.to_path_buf()
        } else {
            target_root
                .parent()
                .unwrap_or(target_root)
                .to_path_buf()
        };

        if let Some(root) = roots
            .iter()
            .find(|root| root.path == requested)
            .map(|root| root.path.clone())
        {
            return Ok(root);
        }

        let store_root =
            workspace::resolve_store_root_from(target_root, SPEC_INDEX_DIR);
        if store_root
            .file_name()
            .and_then(|name| name.to_str())
            == Some(SPEC_INDEX_DIR)
        {
            return Ok(store_root.join("specs"));
        }

        Err(StorageError::Other(format!(
            "invalid spec root '{}': expected a registered scan root, a workspace root containing .spec, the .spec store itself, or a path inside that store",
            target_root.display()
        )))
    }

    pub fn get(
        &self,
        id_or_slug: &str,
    ) -> Result<SpecManifest, SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        if indexed.deleted {
            return Err(SpecError::NotFound(uuid.to_string()));
        }
        read_spec_manifest(&indexed.path)
    }

    pub fn get_full(
        &self,
        id_or_slug: &str,
    ) -> Result<(SpecManifest, String), SpecError> {
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

        if let Some(new_slug_val) = patch.get("slug") {
            if let Some(new_slug) = new_slug_val.as_str() {
                crate::slug::validate_slug(new_slug)?;
                let old = self.inner.fs.read(&indexed.path)?;
                if let Some(old_slug) =
                    old.extra.get("slug").and_then(|value| value.as_str())
                {
                    self.slug_index.remove(old_slug);
                }
                self.slug_index.insert(new_slug.to_string(), uuid)?;
            }
        }

        if let Some(to) = to_state {
            let current = indexed.state.as_deref().unwrap_or("draft");
            if let Some(schema) =
                self.inner.schema_registry().get("specification")
            {
                schema.ensure_transition(current, to)?;
            }
        }

        let updated_entity =
            self.inner.fs.update(&indexed.path, &patch, to_state)?;

        let type_id = updated_entity
            .extra
            .get("type")
            .and_then(|value| value.as_str())
            .unwrap_or("specification")
            .to_string();
        let title = updated_entity
            .extra
            .get("title")
            .and_then(|value| value.as_str())
            .map(String::from);
        let state = updated_entity
            .extra
            .get("state")
            .and_then(|value| value.as_str())
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

        let _ = self.inner.fs.append_history(
            &indexed.path,
            updated_entity.extra.clone(),
            None,
        );

        Ok(entity_to_spec(&updated_entity))
    }

    pub fn update_body(
        &self,
        id_or_slug: &str,
        content: &str,
    ) -> Result<(), SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        write_body(&indexed.path, content)?;
        Ok(())
    }

    pub fn delete(
        &mut self,
        id_or_slug: &str,
    ) -> Result<(), SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        let entity = self.inner.fs.read(&indexed.path)?;
        if let Some(slug) =
            entity.extra.get("slug").and_then(|value| value.as_str())
        {
            self.slug_index.remove(slug);
        }
        self.inner.fs.mark_deleted(&indexed.path)?;

        let mut refreshed = indexed.clone();
        refreshed.deleted = true;
        refreshed.updated_at = Utc::now();
        self.inner.index.insert_ticket(&refreshed)?;

        Ok(())
    }
}
