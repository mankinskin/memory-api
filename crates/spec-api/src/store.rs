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

pub const GENERATED_SPEC_FILE_COMMENT: &str =
    "<!-- spec-api:file generated=true -->";

pub const GENERATED_BODY_FILE_COMMENT: &str = GENERATED_SPEC_FILE_COMMENT;

const GENERATED_SPEC_ENTRY_PREFIX: &str = "spec-api:entry";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GeneratedSpecArtifactLocation {
    Body { spec_id: Uuid },
    Section { spec_id: Uuid, section: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedSpecArtifactTarget {
    pub config: String,
    pub target: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GeneratedSpecArtifacts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<GeneratedSpecArtifactTarget>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sections: BTreeMap<String, GeneratedSpecArtifactTarget>,
}

impl GeneratedSpecArtifactTarget {
    fn validate(
        &self,
        location: &str,
    ) -> Result<(), SpecError> {
        if self.config.trim().is_empty() {
            return Err(SpecError::InvalidGeneratedArtifact(format!(
                "generated artifact '{}' is missing config",
                location
            )));
        }
        if self.target.trim().is_empty() {
            return Err(SpecError::InvalidGeneratedArtifact(format!(
                "generated artifact '{}' is missing target",
                location
            )));
        }
        Ok(())
    }
}

impl GeneratedSpecArtifacts {
    fn normalized(&self) -> Result<Self, SpecError> {
        self.body
            .as_ref()
            .map(|target| target.validate("body"))
            .transpose()?;

        let mut sections = BTreeMap::new();
        for (name, target) in &self.sections {
            let normalized_name = normalize_generated_section_name(name)?;
            target.validate(&format!("sections/{}.md", normalized_name))?;
            if sections
                .insert(normalized_name.clone(), target.clone())
                .is_some()
            {
                return Err(SpecError::InvalidGeneratedArtifact(format!(
                    "duplicate generated section mapping for '{}'",
                    normalized_name
                )));
            }
        }

        Ok(Self {
            body: self.body.clone(),
            sections,
        })
    }

    fn is_empty(&self) -> bool {
        self.body.is_none() && self.sections.is_empty()
    }
}

fn normalize_generated_section_name(name: &str) -> Result<String, SpecError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(SpecError::InvalidGeneratedArtifact(
            "generated section name cannot be empty".into(),
        ));
    }

    let normalized = trimmed.strip_suffix(".md").unwrap_or(trimmed);
    if normalized.is_empty() || normalized == "." || normalized == ".." {
        return Err(SpecError::InvalidGeneratedArtifact(format!(
            "invalid generated section name '{}'",
            name
        )));
    }

    if normalized.contains('/') || normalized.contains('\\') {
        return Err(SpecError::InvalidGeneratedArtifact(format!(
            "generated section '{}' must stay within sections/*.md",
            name
        )));
    }

    Ok(normalized.to_string())
}

fn invalid_generated_artifact_path(
    artifact_path: &Path,
    reason: &str,
) -> SpecError {
    SpecError::InvalidGeneratedArtifact(format!(
        "invalid generated artifact path '{}': {}",
        artifact_path.display(),
        reason
    ))
}

fn parse_generated_artifact_location(
    artifact_path: &Path,
) -> Result<GeneratedSpecArtifactLocation, SpecError> {
    let file_name = artifact_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            invalid_generated_artifact_path(
                artifact_path,
                "missing file name",
            )
        })?;

    if file_name == "body.md" {
        let spec_dir = artifact_path.parent().ok_or_else(|| {
            invalid_generated_artifact_path(
                artifact_path,
                "missing spec directory",
            )
        })?;
        let specs_dir = spec_dir.parent().ok_or_else(|| {
            invalid_generated_artifact_path(
                artifact_path,
                "missing specs directory",
            )
        })?;
        let store_dir = specs_dir.parent().ok_or_else(|| {
            invalid_generated_artifact_path(
                artifact_path,
                "missing .spec store directory",
            )
        })?;

        if specs_dir.file_name().and_then(|name| name.to_str())
            != Some("specs")
            || store_dir.file_name().and_then(|name| name.to_str())
                != Some(SPEC_INDEX_DIR)
        {
            return Err(invalid_generated_artifact_path(
                artifact_path,
                "expected .spec/specs/<uuid>/body.md",
            ));
        }

        let spec_id = Uuid::parse_str(
            spec_dir
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| {
                    invalid_generated_artifact_path(
                        artifact_path,
                        "missing spec id directory",
                    )
                })?,
        )
        .map_err(|error| {
            invalid_generated_artifact_path(
                artifact_path,
                &format!("invalid spec id: {error}"),
            )
        })?;

        return Ok(GeneratedSpecArtifactLocation::Body { spec_id });
    }

    let sections_dir = artifact_path.parent().ok_or_else(|| {
        invalid_generated_artifact_path(
            artifact_path,
            "missing sections directory",
        )
    })?;
    if sections_dir.file_name().and_then(|name| name.to_str())
        != Some("sections")
    {
        return Err(invalid_generated_artifact_path(
            artifact_path,
            "expected body.md or sections/<name>.md",
        ));
    }

    let spec_dir = sections_dir.parent().ok_or_else(|| {
        invalid_generated_artifact_path(
            artifact_path,
            "missing spec directory",
        )
    })?;
    let specs_dir = spec_dir.parent().ok_or_else(|| {
        invalid_generated_artifact_path(
            artifact_path,
            "missing specs directory",
        )
    })?;
    let store_dir = specs_dir.parent().ok_or_else(|| {
        invalid_generated_artifact_path(
            artifact_path,
            "missing .spec store directory",
        )
    })?;

    if !file_name.ends_with(".md")
        || specs_dir.file_name().and_then(|name| name.to_str())
            != Some("specs")
        || store_dir.file_name().and_then(|name| name.to_str())
            != Some(SPEC_INDEX_DIR)
    {
        return Err(invalid_generated_artifact_path(
            artifact_path,
            "expected .spec/specs/<uuid>/sections/<name>.md",
        ));
    }

    let spec_id = Uuid::parse_str(
        spec_dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                invalid_generated_artifact_path(
                    artifact_path,
                    "missing spec id directory",
                )
            })?,
    )
    .map_err(|error| {
        invalid_generated_artifact_path(
            artifact_path,
            &format!("invalid spec id: {error}"),
        )
    })?;
    let section = normalize_generated_section_name(
        file_name.strip_suffix(".md").unwrap_or(file_name),
    )?;

    Ok(GeneratedSpecArtifactLocation::Section { spec_id, section })
}

pub fn render_generated_document(
    snippets: &[GeneratedMarkdownSnippet<'_>]
) -> String {
    let config = GeneratedMarkdownConfig::new(
        GENERATED_SPEC_FILE_COMMENT,
        GENERATED_SPEC_ENTRY_PREFIX,
    );

    render_markdown_file(snippets, &config)
}

pub fn render_generated_body(
    snippets: &[GeneratedMarkdownSnippet<'_>]
) -> String {
    render_generated_document(snippets)
}

fn prepare_generated_document(
    snippets: &[GeneratedMarkdownSnippet<'_>],
    existing: Option<&str>,
) -> String {
    let rendered = render_generated_document(snippets);
    prepare_generated_output(&rendered, existing)
}

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
        let index_root =
            workspace::resolve_store_root_from(index_root, SPEC_INDEX_DIR);
        if !index_root.join("entities.db").is_file() {
            return Err(memory_api::error::StorageError::WorkspaceNotFound {
                path: index_root,
            }
            .into());
        }
        Self::open_internal(&index_root)
    }

    /// Initialize a new spec store rooted at `index_root`.
    ///
    /// Creates the workspace directory and all required index files. Idempotent:
    /// if the workspace already exists it is opened without error.
    pub fn init(index_root: &Path) -> Result<Self, SpecError> {
        let index_root =
            workspace::resolve_store_root_from(index_root, SPEC_INDEX_DIR);
        Self::open_internal(&index_root)
    }

    /// Open an existing spec store, or initialize and force-scan it when the
    /// local derived index artifacts do not exist yet.
    pub fn open_or_init(index_root: &Path) -> Result<Self, SpecError> {
        match Self::open(index_root) {
            Ok(store) => Ok(store),
            Err(SpecError::Storage(StorageError::WorkspaceNotFound { .. })) => {
                let mut store = Self::init(index_root)?;
                store.scan(true)?;
                Ok(store)
            }
            Err(error) => Err(error),
        }
    }

    fn open_internal(index_root: &Path) -> Result<Self, SpecError> {
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

    pub fn update_generated_body(
        &self,
        id_or_slug: &str,
        snippets: &[GeneratedMarkdownSnippet<'_>],
    ) -> Result<(), SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        let existing = read_body(&indexed.path);
        let prepared = prepare_generated_document(snippets, Some(&existing));

        write_body(&indexed.path, &prepared)?;
        Ok(())
    }

    pub fn generated_artifact_matches(
        &self,
        artifact_path: &Path,
        snippets: &[GeneratedMarkdownSnippet<'_>],
    ) -> Result<bool, SpecError> {
        let location = parse_generated_artifact_location(artifact_path)?;

        match location {
            GeneratedSpecArtifactLocation::Body { spec_id } => {
                let indexed = self
                    .inner
                    .get_indexed(&spec_id)?
                    .ok_or_else(|| SpecError::NotFound(spec_id.to_string()))?;
                let existing = read_body(&indexed.path);
                let expected =
                    prepare_generated_document(snippets, Some(&existing));
                Ok(existing == expected)
            },
            GeneratedSpecArtifactLocation::Section { spec_id, section } => {
                let indexed = self
                    .inner
                    .get_indexed(&spec_id)?
                    .ok_or_else(|| SpecError::NotFound(spec_id.to_string()))?;
                let existing = read_section(&indexed.path, &section);
                let expected =
                    prepare_generated_document(snippets, Some(&existing));
                Ok(existing == expected)
            },
        }
    }

    pub fn sync_generated_artifact(
        &mut self,
        artifact_path: &Path,
        snippets: &[GeneratedMarkdownSnippet<'_>],
    ) -> Result<GeneratedSpecArtifactLocation, SpecError> {
        let location = parse_generated_artifact_location(artifact_path)?;

        match &location {
            GeneratedSpecArtifactLocation::Body { spec_id } => {
                self.update_generated_body(&spec_id.to_string(), snippets)?;
                self.update(&spec_id.to_string(), BTreeMap::new(), None)?;
            },
            GeneratedSpecArtifactLocation::Section { spec_id, section } => {
                self.update_generated_section(
                    &spec_id.to_string(),
                    section,
                    snippets,
                )?;
                self.update(&spec_id.to_string(), BTreeMap::new(), None)?;
            },
        }

        Ok(location)
    }

    pub fn get_generated_artifacts(
        &self,
        id_or_slug: &str,
    ) -> Result<Option<GeneratedSpecArtifacts>, SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        let path = indexed.path.join(GENERATED_SPEC_ARTIFACTS_FILE);
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&path)
            .map_err(|error| SpecError::Storage(StorageError::Io(error)))?;
        let parsed: GeneratedSpecArtifacts = toml::from_str(&content)
            .map_err(|error| SpecError::Serialization(error.to_string()))?;
        Ok(Some(parsed.normalized()?))
    }

    pub fn update_generated_artifacts(
        &self,
        id_or_slug: &str,
        artifacts: &GeneratedSpecArtifacts,
    ) -> Result<(), SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        let path = indexed.path.join(GENERATED_SPEC_ARTIFACTS_FILE);
        let normalized = artifacts.normalized()?;

        if normalized.is_empty() {
            if path.exists() {
                fs::remove_file(&path)
                    .map_err(|error| SpecError::Storage(StorageError::Io(error)))?;
            }
            return Ok(());
        }

        let content = toml::to_string_pretty(&normalized)
            .map_err(|error| SpecError::Serialization(error.to_string()))?;
        fs::write(&path, content)
            .map_err(|error| SpecError::Storage(StorageError::Io(error)))?;
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
