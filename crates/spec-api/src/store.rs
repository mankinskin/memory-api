use std::{
    collections::BTreeMap,
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use chrono::Utc;
use serde::{
    Deserialize,
    Serialize,
};
use serde_json::Value;
use tracing::field::Empty;
use uuid::Uuid;

use memory_api::{
    error::StorageError,
    generated_markdown::{
        GeneratedMarkdownConfig,
        GeneratedMarkdownSnippet,
        prepare_generated_output,
        render_markdown_file,
    },
    model::filesystem::{
        EntityFolderConfig,
        ScanRoot,
    },
    storage::{
        entity_fs::EntityFs,
        entity_store::{
            EntityStore,
            ScanReport,
        },
        indexed::IndexedEntity,
    },
    workspace,
};

use crate::{
    error::SpecError,
    manifest::{
        SpecHealthFinding,
        SpecHealthReport,
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
    read_section,
    read_spec_manifest,
    spec_to_entity,
    write_body,
};

const SPEC_MANIFEST_FILE: &str = "spec.toml";
const SPEC_LOCK_FILE: &str = ".spec-lock";
const SPEC_INDEX_DIR: &str = ".spec";
const GENERATED_SPEC_ARTIFACTS_FILE: &str = "generated.toml";
const SPEC_STORE_TRACE_TARGET: &str = "spec_api::store";

fn build_search_content(
    spec: &SpecManifest,
    body: &str,
) -> Option<String> {
    let body = body.trim();
    let contract = spec.contract_search_text();
    let contract = contract.trim();

    match (body.is_empty(), contract.is_empty()) {
        (true, true) => None,
        (false, true) => Some(body.to_string()),
        (true, false) => Some(contract.to_string()),
        (false, false) => Some(format!("{body}\n\n{contract}")),
    }
}


#[path = "store_generated.rs"]
mod store_generated;
use store_generated::*;
pub use store_generated::{
    GeneratedSpecArtifactLocation,
    GeneratedSpecArtifactTarget,
    GeneratedSpecArtifacts,
    GENERATED_BODY_FILE_COMMENT,
    GENERATED_SPEC_FILE_COMMENT,
    render_generated_body,
    render_generated_document,
};

pub struct SpecStore {
    inner: EntityStore,
    slug_index: SlugIndex,
}

impl SpecStore {
    /// Open an existing spec store rooted at `index_root`.
    ///
    /// Returns [`memory_api::error::StorageError::WorkspaceNotFound`] if the
    /// workspace has not been initialized. Run `spec init` first.
    pub fn open(index_root: &Path) -> Result<Self, SpecError> {
        let _span_guard = tracing::info_span!(
            target: SPEC_STORE_TRACE_TARGET,
            "spec_store_open",
            requested_root = %index_root.display(),
        )
        .entered();
        let index_root =
            workspace::resolve_store_root_from(index_root, SPEC_INDEX_DIR);
        if !index_root.join("entities.db").is_file() {
            return Err(memory_api::error::StorageError::WorkspaceNotFound {
                path: index_root,
            }
            .into());
        }
        let store = Self::open_internal(&index_root)?;
        tracing::info!(
            target: SPEC_STORE_TRACE_TARGET,
            resolved_root = %index_root.display(),
            "spec_store_open_complete"
        );
        Ok(store)
    }

    /// Initialize a new spec store rooted at `index_root`.
    ///
    /// Creates the workspace directory and all required index files. Idempotent:
    /// if the workspace already exists it is opened without error.
    pub fn init(index_root: &Path) -> Result<Self, SpecError> {
        let _span_guard = tracing::info_span!(
            target: SPEC_STORE_TRACE_TARGET,
            "spec_store_init",
            requested_root = %index_root.display(),
        )
        .entered();
        let index_root =
            workspace::resolve_store_root_from(index_root, SPEC_INDEX_DIR);
        let store = Self::open_internal(&index_root)?;
        tracing::info!(
            target: SPEC_STORE_TRACE_TARGET,
            resolved_root = %index_root.display(),
            "spec_store_init_complete"
        );
        Ok(store)
    }

    /// Open an existing spec store, or initialize and force-scan it when the
    /// local derived index artifacts do not exist yet.
    pub fn open_or_init(index_root: &Path) -> Result<Self, SpecError> {
        let span = tracing::info_span!(
            target: SPEC_STORE_TRACE_TARGET,
            "spec_store_open_or_init",
            requested_root = %index_root.display(),
            initialized_store = Empty,
        );
        let _span_guard = span.enter();
        match Self::open(index_root) {
            Ok(store) => {
                span.record("initialized_store", false);
                tracing::info!(
                    target: SPEC_STORE_TRACE_TARGET,
                    initialized_store = false,
                    "spec_store_open_or_init_complete"
                );
                Ok(store)
            },
            Err(SpecError::Storage(StorageError::WorkspaceNotFound {
                ..
            })) => {
                span.record("initialized_store", true);
                let mut store = Self::init(index_root)?;
                store.scan(true)?;
                tracing::info!(
                    target: SPEC_STORE_TRACE_TARGET,
                    initialized_store = true,
                    "spec_store_open_or_init_complete"
                );
                Ok(store)
            },
            Err(error) => Err(error),
        }
    }

    fn open_internal(index_root: &Path) -> Result<Self, SpecError> {
        let _span_guard = tracing::debug_span!(
            target: SPEC_STORE_TRACE_TARGET,
            "spec_store_open_internal",
            resolved_root = %index_root.display(),
        )
        .entered();
        let fs = EntityFs::with_config(
            EntityFolderConfig::new(SPEC_MANIFEST_FILE, SPEC_LOCK_FILE)
                .with_body_file("body.md"),
        );
        let registry = crate::default_schema::spec_schema_registry();
        let inner = EntityStore::open_with(index_root, fs, registry)?;
        inner.add_scan_root(ScanRoot {
            path: index_root.join("specs"),
            label: "specs".to_string(),
        })?;
        tracing::debug!(
            target: SPEC_STORE_TRACE_TARGET,
            scan_root = %index_root.join("specs").display(),
            "spec_store_default_scan_root_registered"
        );
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
        let _span_guard = tracing::info_span!(
            target: SPEC_STORE_TRACE_TARGET,
            "spec_store_scan",
            reindex,
            slug_entries = Empty,
        )
        .entered();
        let report = self.inner.scan(reindex)?;
        self.rebuild_slug_index()?;
        let slug_entries = self.slug_index_len();
        tracing::Span::current().record("slug_entries", slug_entries);
        tracing::info!(
            target: SPEC_STORE_TRACE_TARGET,
            reindex,
            integrated = report.integrated,
            pruned = report.pruned,
            diagnostics = report.diagnostics.len(),
            slug_entries,
            "spec_store_scan_complete"
        );
        Ok(report)
    }

    fn rebuild_slug_index(&mut self) -> Result<(), SpecError> {
        let _span_guard = tracing::debug_span!(
            target: SPEC_STORE_TRACE_TARGET,
            "spec_store_rebuild_slug_index",
            indexed_entities = Empty,
            slug_entries = Empty,
        )
        .entered();
        let all = self.inner.list_indexed()?;
        tracing::Span::current().record("indexed_entities", all.len());
        let entries = all.iter().filter_map(|entry| {
            let manifest = self.inner.fs.read(&entry.path).ok()?;
            let slug = manifest.extra.get("slug")?.as_str()?.to_string();
            Some((slug, entry.id))
        });
        self.slug_index = SlugIndex::rebuild(entries)?;
        let slug_entries = self.slug_index_len();
        tracing::Span::current().record("slug_entries", slug_entries);
        tracing::debug!(
            target: SPEC_STORE_TRACE_TARGET,
            indexed_entities = all.len(),
            slug_entries,
            "spec_store_rebuild_slug_index_complete"
        );
        Ok(())
    }

    pub fn resolve_id(
        &self,
        id_or_slug: &str,
    ) -> Result<Uuid, SpecError> {
        let _span_guard = tracing::debug_span!(
            target: SPEC_STORE_TRACE_TARGET,
            "spec_store_resolve_id",
            input = id_or_slug,
        )
        .entered();
        if let Ok(uuid) = id_or_slug.parse::<Uuid>() {
            tracing::debug!(
                target: SPEC_STORE_TRACE_TARGET,
                resolution = "uuid",
                resolved_id = %uuid,
                "spec_store_resolve_id_complete"
            );
            return Ok(uuid);
        }
        if let Some(uuid) = self.resolve_prefix(id_or_slug)? {
            tracing::debug!(
                target: SPEC_STORE_TRACE_TARGET,
                resolution = "prefix",
                resolved_id = %uuid,
                "spec_store_resolve_id_complete"
            );
            return Ok(uuid);
        }
        let resolved = self
            .slug_index
            .resolve(id_or_slug)
            .ok_or_else(|| {
                SpecError::NotFound(format!(
                    "{}; {}",
                    id_or_slug,
                    crate::workspace::workspace_recovery_hint(
                        &self.inner.index_root
                    )
                ))
            })?;
        tracing::debug!(
            target: SPEC_STORE_TRACE_TARGET,
            resolution = "slug",
            resolved_id = %resolved,
            "spec_store_resolve_id_complete"
        );
        Ok(resolved)
    }

    fn slug_index_len(&self) -> usize {
        self.slug_index.len()
    }

    fn resolve_prefix(
        &self,
        prefix: &str,
    ) -> Result<Option<Uuid>, SpecError> {
        if prefix.len() < 4 {
            return Ok(None);
        }
        let all = self.inner.list_indexed().map_err(SpecError::Storage)?;
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
        };
        self.inner.index.insert_ticket(&indexed)?;
        let search_content = build_search_content(manifest, body);
        let created_at_str = manifest.created_at.to_rfc3339();
        let effort_str = entity.extra.get("effort").and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        });
        self.inner.search.upsert(
            &manifest.id,
            title.as_deref(),
            search_content.as_deref(),
            state.as_deref(),
            Some(&type_id),
            Some(&created_at_str),
            effort_str.as_deref(),
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
        let Some(target_root) = target_root else {
            // Canonical: write into the workspace's own .spec/specs/ directory
            // (resolved via the index_root), ignoring any registered scan roots.
            // Callers that want to place specs elsewhere must pass an explicit
            // `target_root`.
            return Ok(self.inner.index_root.join("specs"));
        };

        let roots = self.inner.list_scan_roots()?;

        let requested = if target_root.is_dir() {
            target_root.to_path_buf()
        } else {
            target_root.parent().unwrap_or(target_root).to_path_buf()
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
        if store_root.file_name().and_then(|name| name.to_str())
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
        let spec = read_spec_manifest(&indexed.path)?;
        let body = read_body(&indexed.path);
        Ok((spec, body))
    }

    pub fn health(
        &self,
        id_or_slug: &str,
    ) -> Result<SpecHealthReport, SpecError> {
        let spec = self.get(id_or_slug)?;
        Ok(Self::build_health_report([spec]))
    }

    pub fn health_all(&self) -> Result<SpecHealthReport, SpecError> {
        let all = self.inner.list_indexed().map_err(SpecError::Storage)?;
        let specs = all
            .iter()
            .filter_map(|indexed| self.get(&indexed.id.to_string()).ok())
            .collect::<Vec<_>>();
        Ok(Self::build_health_report(specs))
    }

    fn build_health_report(
        specs: impl IntoIterator<Item = SpecManifest>
    ) -> SpecHealthReport {
        let specs = specs.into_iter().collect::<Vec<_>>();
        let issues = specs
            .iter()
            .flat_map(|spec| {
                spec.health_issues()
                    .into_iter()
                    .map(|issue| SpecHealthFinding { id: spec.id, issue })
            })
            .collect();

        SpecHealthReport {
            specs_checked: specs.len(),
            issues,
        }
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
        let existing_entity = self.inner.fs.read(&indexed.path)?;

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
        let changed = updated_entity.extra != existing_entity.extra;

        if !changed {
            return Ok(entity_to_spec(&updated_entity));
        }

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
        let spec = entity_to_spec(&updated_entity);

        let refreshed = IndexedEntity {
            id: uuid,
            path: indexed.path.clone(),
            type_id: type_id.clone(),
            title: title.clone(),
            state: state.clone(),
            created_at: indexed.created_at,
            updated_at: Utc::now(),
        };
        self.inner.index.insert_ticket(&refreshed)?;

        let body = read_body(&indexed.path);
        let search_content = build_search_content(&spec, &body);
        let created_at_str = indexed.created_at.to_rfc3339();
        let effort_str =
            updated_entity.extra.get("effort").and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            });
        self.inner.search.upsert(
            &uuid,
            title.as_deref(),
            search_content.as_deref(),
            state.as_deref(),
            Some(&type_id),
            Some(&created_at_str),
            effort_str.as_deref(),
        )?;

        let _ = self.inner.fs.append_history(
            &indexed.path,
            updated_entity.extra.clone(),
            None,
        );

        Ok(spec)
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
        self.inner.fs.delete(&indexed.path)?;
        self.inner.index.remove_ticket(&uuid)?;
        self.inner.search.remove(&uuid)?;

        Ok(())
    }
}
