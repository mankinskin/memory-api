mod filter;
mod generated_targets;

#[cfg(test)]
mod feedback_tests;

#[cfg(test)]
mod tests;

pub use self::{
    filter::RuleFilter,
    generated_targets::GeneratedTargetRecord,
};

use std::{
    collections::{
        BTreeMap,
        HashMap,
    },
    fs,
    io::{
        BufRead,
        BufReader,
        Write,
    },
    path::{
        Path,
        PathBuf,
    },
};

use chrono::Utc;
use serde_json::{
    Number,
    Value,
};
use uuid::Uuid;

use memory_api::{
    error::StorageError,
    model::entity::EntityManifest,
    model::filesystem::EntityFolderConfig,
    storage::{
        ensure_gitignore_entries,
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
    default_schema::rule_schema_registry,
    error::RuleError,
    feedback::{
        FeedbackSummary,
        RuleFeedbackEvent,
        RuleFeedbackInput,
    },
    manifest::{
        RuleId,
        RuleManifest,
    },
};

const RULE_MANIFEST_FILE: &str = "rule.toml";
const RULE_LOCK_FILE: &str = ".rule-lock";
const RULE_ENTRY_TYPE_ID: &str = "rule-entry";
const GENERATED_TARGET_TYPE_ID: &str = "generated-target";
const GENERATED_TARGET_ROOT_DIR: &str = "entities";
const RULE_BODY_FILE: &str = "body.md";
const FEEDBACK_DIR: &str = "feedback";
const FEEDBACK_EVENTS_FILE: &str = "events.ndjson";

pub struct RuleStore {
    inner: EntityStore,
    slug_index: HashMap<String, Uuid>,
}

impl RuleStore {
    /// Open an existing rule store rooted at `index_root`.
    ///
    /// Returns [`StorageError::WorkspaceNotFound`] if the workspace has not been
    /// initialized. Run `rule init` first.
    pub fn open(index_root: &Path) -> Result<Self, RuleError> {
        let index_root =
            workspace::resolve_store_root_from(index_root, ".rule");
        if !index_root.join("entities.db").is_file() {
            return Err(
                StorageError::WorkspaceNotFound { path: index_root }.into()
            );
        }
        Self::open_internal(&index_root)
    }

    /// Initialize a new rule store rooted at `index_root`.
    ///
    /// Creates the workspace directory and all required index files. Idempotent:
    /// if the workspace already exists it is opened without error.
    pub fn init(index_root: &Path) -> Result<Self, RuleError> {
        let index_root =
            workspace::resolve_store_root_from(index_root, ".rule");
        Self::open_internal(&index_root)
    }

    /// Open an existing rule store, or initialize and force-scan it when the
    /// local derived index artifacts do not exist yet.
    pub fn open_or_init(index_root: &Path) -> Result<Self, RuleError> {
        match Self::open(index_root) {
            Ok(store) => Ok(store),
            Err(RuleError::Storage(StorageError::WorkspaceNotFound { .. })) => {
                let mut store = Self::init(index_root)?;
                store.scan(true)?;
                Ok(store)
            }
            Err(error) => Err(error),
        }
    }

    fn open_internal(index_root: &Path) -> Result<Self, RuleError> {
        let fs = EntityFs::with_config(
            EntityFolderConfig::new(RULE_MANIFEST_FILE, RULE_LOCK_FILE)
                .with_body_file(RULE_BODY_FILE),
        );
        let registry = rule_schema_registry();
        let inner = EntityStore::open_with(index_root, fs, registry)?;
        inner.add_scan_root(memory_api::model::filesystem::ScanRoot {
            path: index_root.join("rules"),
            label: "rules".to_string(),
        })?;
        ensure_gitignore_entries(index_root, &["entities/"])?;
        let mut store = Self {
            inner,
            slug_index: HashMap::new(),
        };
        store.prune_missing_index_entries()?;
        store.rebuild_slug_index()?;
        Ok(store)
    }

    pub fn entity_store(&self) -> &EntityStore {
        &self.inner
    }

    pub fn scan(
        &mut self,
        reindex: bool,
    ) -> Result<ScanReport, RuleError> {
        let report = self.inner.scan(reindex)?;
        if reindex {
            self.reindex_rule_bodies()?;
        }
        self.rebuild_slug_index()?;
        Ok(report)
    }

    fn reindex_rule_bodies(&self) -> Result<(), RuleError> {
        for indexed in self.inner.list_indexed(false)? {
            if indexed.type_id != RULE_ENTRY_TYPE_ID {
                continue;
            }

            let entity = self.read_indexed_manifest(&indexed)?;
            let title = entity.extra.get("title").and_then(Value::as_str);
            let state = entity.extra.get("state").and_then(Value::as_str);
            let body = self.read_rule_body(&indexed.path, Some(&entity));
            let created_at_str = indexed.created_at.to_rfc3339();
            let effort_str = entity.extra.get("effort")
                .and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                });

            self.inner.search.upsert(
                &indexed.id,
                title,
                body.as_deref(),
                state,
                Some(RULE_ENTRY_TYPE_ID),
                Some(&created_at_str),
                effort_str.as_deref(),
            )?;
        }

        Ok(())
    }

    pub fn rebuild_slug_index(&mut self) -> Result<(), RuleError> {
        let mut next = HashMap::new();
        for indexed in self.inner.list_indexed(false)? {
            let manifest = self.read_indexed_manifest(&indexed)?;
            if let Some(slug) =
                manifest.extra.get("slug").and_then(Value::as_str)
            {
                next.insert(slug.to_string(), indexed.id);
            }
        }
        self.slug_index = next;
        Ok(())
    }

    fn prune_missing_index_entries(&mut self) -> Result<(), RuleError> {
        let stale_ids: Vec<_> = self
            .inner
            .list_indexed(true)?
            .into_iter()
            .filter(|indexed| is_missing_index_entry(indexed))
            .map(|indexed| indexed.id)
            .collect();

        for id in stale_ids {
            self.inner.index.remove_ticket(&id)?;
        }

        Ok(())
    }

    fn read_indexed_manifest(
        &self,
        indexed: &IndexedEntity,
    ) -> Result<EntityManifest, RuleError> {
        self.inner.fs.read(&indexed.path).map_err(|err| {
            RuleError::Asset(format!(
                "failed to read indexed rule entity at {}: {err}",
                indexed.path.display()
            ))
        })
    }

    pub fn resolve_id(
        &self,
        id_or_slug: &str,
    ) -> Result<Uuid, RuleError> {
        if let Ok(uuid) = id_or_slug.parse::<Uuid>() {
            return Ok(uuid);
        }

        if let Some(uuid) = self.resolve_prefix(id_or_slug)? {
            return Ok(uuid);
        }

        self.slug_index
            .get(id_or_slug)
            .copied()
            .ok_or_else(|| RuleError::NotFound(id_or_slug.to_string()))
    }

    pub fn create(
        &mut self,
        manifest: &RuleManifest,
        target_root: Option<&Path>,
    ) -> Result<RuleId, RuleError> {
        let slug = manifest.slug().ok_or_else(|| {
            RuleError::InvalidSlug("missing slug".to_string())
        })?;
        validate_slug(slug)?;

        if let Some(existing) = self.slug_index.get(slug) {
            if *existing != manifest.id {
                return Err(RuleError::DuplicateSlug(slug.to_string()));
            }
        }

        let root = match target_root {
            Some(path) => {
                // Resolve the requested path back to the canonical
                // `<workspace>/.rule/rules/` directory. Without this, callers
                // that pass a workspace root (or any directory that is not
                // already the rules folder) would cause rule manifests to be
                // written directly under `<path>/<uuid>/rule.toml` instead of
                // `<path>/.rule/rules/<uuid>/rule.toml`.
                let store_root =
                    workspace::resolve_store_root_from(path, ".rule");
                if store_root.file_name().and_then(|n| n.to_str())
                    == Some(".rule")
                {
                    store_root.join("rules")
                } else {
                    // Path is not inside any recognisable `.rule` store —
                    // fall back to the canonical location under index_root.
                    self.inner.index_root.join("rules")
                }
            }
            None => self.inner.index_root.join("rules"),
        };
        fs::create_dir_all(&root).map_err(StorageError::Io)?;

        let entity = rule_to_entity(manifest);
        self.inner
            .schema_registry()
            .get(RULE_ENTRY_TYPE_ID)
            .ok_or_else(|| {
                RuleError::Asset("missing rule-entry schema".to_string())
            })?
            .validate_manifest(&entity)
            .map_err(|err| RuleError::Asset(err.to_string()))?;

        let folder = self.inner.fs.create(&entity, &root, manifest.body())?;
        let indexed = IndexedEntity {
            id: manifest.id,
            path: folder.clone(),
            type_id: RULE_ENTRY_TYPE_ID.to_string(),
            title: manifest.title().map(ToOwned::to_owned),
            state: manifest.state().map(ToOwned::to_owned),
            created_at: manifest.created_at,
            updated_at: Utc::now(),
            deleted: false,
        };
        self.inner.index.insert_ticket(&indexed)?;
        let created_at_str = manifest.created_at.to_rfc3339();
        let effort_str = entity.extra.get("effort")
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            });
        self.inner.search.upsert(
            &manifest.id,
            manifest.title(),
            manifest.body(),
            manifest.state(),
            Some(RULE_ENTRY_TYPE_ID),
            Some(&created_at_str),
            effort_str.as_deref(),
        )?;
        let _ =
            self.inner
                .fs
                .append_history(&folder, entity.extra.clone(), None);
        self.slug_index.insert(slug.to_string(), manifest.id);

        Ok(manifest.id)
    }

    pub fn get(
        &self,
        id_or_slug: &str,
    ) -> Result<RuleManifest, RuleError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| RuleError::NotFound(uuid.to_string()))?;
        if indexed.deleted {
            return Err(RuleError::NotFound(uuid.to_string()));
        }

        self.hydrate_rule(&indexed)
    }

    pub fn delete(
        &mut self,
        id_or_slug: &str,
    ) -> Result<(), RuleError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| RuleError::NotFound(uuid.to_string()))?;

        if indexed.deleted
            || !matches!(
                indexed.type_id.as_str(),
                RULE_ENTRY_TYPE_ID | GENERATED_TARGET_TYPE_ID
            )
        {
            return Err(RuleError::NotFound(id_or_slug.to_string()));
        }

        let entity = self.inner.fs.read(&indexed.path)?;
        if let Some(existing_slug) =
            entity.extra.get("slug").and_then(Value::as_str)
        {
            self.slug_index.remove(existing_slug);
        }

        self.inner.fs.mark_deleted(&indexed.path)?;

        let mut refreshed = indexed.clone();
        refreshed.deleted = true;
        refreshed.updated_at = Utc::now();
        self.inner.index.insert_ticket(&refreshed)?;
        self.inner.search.remove(&uuid)?;

        Ok(())
    }

    pub fn update(
        &mut self,
        id_or_slug: &str,
        mut patch: BTreeMap<String, Value>,
        to_state: Option<&str>,
    ) -> Result<RuleManifest, RuleError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| RuleError::NotFound(uuid.to_string()))?;
        let body_update = patch
            .remove("body")
            .and_then(|value| value.as_str().map(str::to_string));

        if let Some(new_slug_value) = patch.get("slug") {
            if let Some(new_slug) = new_slug_value.as_str() {
                validate_slug(new_slug)?;
                if let Some(existing) = self.slug_index.get(new_slug) {
                    if *existing != uuid {
                        return Err(RuleError::DuplicateSlug(
                            new_slug.to_string(),
                        ));
                    }
                }
                let current = self.inner.fs.read(&indexed.path)?;
                if let Some(old_slug) =
                    current.extra.get("slug").and_then(Value::as_str)
                {
                    self.slug_index.remove(old_slug);
                }
                self.slug_index.insert(new_slug.to_string(), uuid);
            }
        }

        if let Some(next_state) = to_state {
            let current_state = indexed.state.as_deref().unwrap_or("draft");
            if let Some(schema) =
                self.inner.schema_registry().get(RULE_ENTRY_TYPE_ID)
            {
                schema
                    .ensure_transition(current_state, next_state)
                    .map_err(|err| RuleError::Asset(err.to_string()))?;
            }
        }

        let updated_entity =
            self.inner.fs.update(&indexed.path, &patch, to_state)?;
        if let Some(body) = body_update.as_deref() {
            self.inner.fs.write_description(&indexed.path, body)?;
        }
        let title = updated_entity
            .extra
            .get("title")
            .and_then(Value::as_str)
            .map(str::to_string);
        let state = updated_entity
            .extra
            .get("state")
            .and_then(Value::as_str)
            .map(str::to_string);

        let refreshed = IndexedEntity {
            id: uuid,
            path: indexed.path.clone(),
            type_id: RULE_ENTRY_TYPE_ID.to_string(),
            title: title.clone(),
            state: state.clone(),
            created_at: indexed.created_at,
            updated_at: Utc::now(),
            deleted: false,
        };
        self.inner.index.insert_ticket(&refreshed)?;

        let body = self.read_rule_body(&indexed.path, Some(&updated_entity));
        let created_at_str = indexed.created_at.to_rfc3339();
        let effort_str = updated_entity.extra.get("effort")
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            });
        self.inner.search.upsert(
            &uuid,
            title.as_deref(),
            body.as_deref(),
            state.as_deref(),
            Some(RULE_ENTRY_TYPE_ID),
            Some(&created_at_str),
            effort_str.as_deref(),
        )?;

        let _ = self.inner.fs.append_history(
            &indexed.path,
            updated_entity.extra.clone(),
            None,
        );

        let mut rule = entity_to_rule(&updated_entity);
        if let Some(body) = body {
            rule.set_body(&body);
        }

        Ok(rule)
    }

    pub fn update_body(
        &self,
        id_or_slug: &str,
        body: &str,
    ) -> Result<(), RuleError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| RuleError::NotFound(uuid.to_string()))?;
        let updated_entity = self.inner.fs.read(&indexed.path)?;
        self.inner.fs.write_description(&indexed.path, body)?;
        let created_at_str = indexed.created_at.to_rfc3339();
        let effort_str = updated_entity.extra.get("effort")
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            });
        self.inner.search.upsert(
            &uuid,
            updated_entity.extra.get("title").and_then(Value::as_str),
            Some(body),
            updated_entity.extra.get("state").and_then(Value::as_str),
            Some(RULE_ENTRY_TYPE_ID),
            Some(&created_at_str),
            effort_str.as_deref(),
        )?;
        let _ = self.inner.fs.append_history(
            &indexed.path,
            updated_entity.extra.clone(),
            None,
        );
        Ok(())
    }

    pub fn record_feedback(
        &mut self,
        id_or_slug: &str,
        input: RuleFeedbackInput,
    ) -> Result<(RuleManifest, RuleFeedbackEvent), RuleError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| RuleError::NotFound(uuid.to_string()))?;
        if indexed.deleted || indexed.type_id != RULE_ENTRY_TYPE_ID {
            return Err(RuleError::NotFound(uuid.to_string()));
        }

        let event = input.into_event();
        append_feedback_event(&self.inner.fs, &indexed.path, &event)?;
        let events = read_feedback_events(&self.inner.fs, &indexed.path)?;
        let summary = FeedbackSummary::from_events(&events);
        let rule =
            self.update(id_or_slug, feedback_summary_patch(&summary), None)?;

        Ok((rule, event))
    }

    pub fn list(
        &self,
        filter: &RuleFilter,
        limit: Option<usize>,
    ) -> Result<Vec<RuleManifest>, RuleError> {
        let mut rules = Vec::new();

        for indexed in self.inner.list_indexed(false)? {
            if let Some(state) = filter.state.as_deref() {
                if indexed.state.as_deref() != Some(state) {
                    continue;
                }
            }
            if indexed.type_id != RULE_ENTRY_TYPE_ID {
                continue;
            }

            let rule = self.hydrate_rule(&indexed)?;
            if filter.matches(&rule) {
                rules.push(rule);
            }
        }

        rules.sort_by_key(|rule| {
            (
                rule.order_key().unwrap_or_default(),
                rule.slug().unwrap_or("").to_string(),
            )
        });
        if let Some(limit) = limit {
            rules.truncate(limit);
        }
        Ok(rules)
    }

    pub fn search(
        &self,
        query: &str,
        filter: &RuleFilter,
        limit: usize,
    ) -> Result<Vec<RuleManifest>, RuleError> {
        let candidates = self
            .inner
            .search(query, limit.saturating_mul(4).max(limit))?;
        let mut rules = Vec::new();

        for candidate in candidates {
            let indexed = match self.inner.get_indexed(&candidate.id)? {
                Some(indexed) if !indexed.deleted => indexed,
                _ => continue,
            };
            if indexed.type_id != RULE_ENTRY_TYPE_ID {
                continue;
            }

            let rule = self.hydrate_rule(&indexed)?;
            if filter.matches(&rule) {
                rules.push(rule);
            }
            if rules.len() >= limit {
                break;
            }
        }

        Ok(rules)
    }

    fn resolve_prefix(
        &self,
        prefix: &str,
    ) -> Result<Option<Uuid>, RuleError> {
        if prefix.len() < 4 {
            return Ok(None);
        }

        let matches: Vec<_> = self
            .inner
            .list_indexed(false)?
            .into_iter()
            .filter(|entity| entity.id.to_string().starts_with(prefix))
            .collect();

        match matches.len() {
            0 => Ok(None),
            1 => Ok(Some(matches[0].id)),
            _ => Err(RuleError::AmbiguousPrefix(prefix.to_string())),
        }
    }

    fn hydrate_rule(
        &self,
        indexed: &IndexedEntity,
    ) -> Result<RuleManifest, RuleError> {
        let entity = self.read_indexed_manifest(indexed)?;
        let mut rule = entity_to_rule(&entity);
        if let Some(body) = self.read_rule_body(&indexed.path, Some(&entity)) {
            rule.set_body(&body);
        }
        Ok(rule)
    }

    fn read_rule_body(
        &self,
        entity_path: &Path,
        entity: Option<&EntityManifest>,
    ) -> Option<String> {
        self.inner.fs.read_description(entity_path).or_else(|| {
            entity
                .and_then(|manifest| manifest.extra.get("body"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
    }
}

fn validate_slug(slug: &str) -> Result<(), RuleError> {
    if slug.is_empty() {
        return Err(RuleError::InvalidSlug("slug cannot be empty".to_string()));
    }

    let valid = slug.chars().all(|ch| {
        ch.is_ascii_lowercase()
            || ch.is_ascii_digit()
            || matches!(ch, '/' | '-' | '_' | '.')
    });

    if valid {
        Ok(())
    } else {
        Err(RuleError::InvalidSlug(slug.to_string()))
    }
}

fn rule_to_entity(rule: &RuleManifest) -> EntityManifest {
    let mut extra = rule.extra.clone();
    extra.remove("body");

    EntityManifest {
        id: rule.id,
        created_at: rule.created_at,
        extra,
    }
}

fn entity_to_rule(entity: &EntityManifest) -> RuleManifest {
    RuleManifest {
        id: entity.id,
        created_at: entity.created_at,
        extra: entity.extra.clone(),
    }
}

fn is_missing_index_entry(indexed: &IndexedEntity) -> bool {
    !indexed.path.is_dir() || !indexed.path.join(RULE_MANIFEST_FILE).is_file()
}

fn feedback_events_path(
    fs: &EntityFs,
    entity_path: &Path,
) -> PathBuf {
    entity_path
        .join(fs.config.assets_dir)
        .join(FEEDBACK_DIR)
        .join(FEEDBACK_EVENTS_FILE)
}

fn append_feedback_event(
    fs: &EntityFs,
    entity_path: &Path,
    event: &RuleFeedbackEvent,
) -> Result<(), RuleError> {
    fs.ensure_assets_dir(entity_path)?;
    let path = feedback_events_path(fs, entity_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(StorageError::Io)?;
    }

    let line = serde_json::to_string(event)
        .map_err(|err| StorageError::Serialization(err.to_string()))?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(StorageError::Io)?;
    writeln!(file, "{line}").map_err(StorageError::Io)?;
    Ok(())
}

fn read_feedback_events(
    fs: &EntityFs,
    entity_path: &Path,
) -> Result<Vec<RuleFeedbackEvent>, RuleError> {
    let path = feedback_events_path(fs, entity_path);
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(&path).map_err(StorageError::Io)?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for (index, line) in reader.lines().enumerate() {
        let line = line.map_err(StorageError::Io)?;
        if line.trim().is_empty() {
            continue;
        }
        let event = serde_json::from_str(&line).map_err(|err| {
            RuleError::Asset(format!(
                "invalid feedback event at {}:{}: {err}",
                path.display(),
                index + 1,
            ))
        })?;
        events.push(event);
    }

    Ok(events)
}

fn feedback_summary_patch(
    summary: &FeedbackSummary
) -> BTreeMap<String, Value> {
    let mut patch = BTreeMap::from([
        (
            "feedback_helpful_count".to_string(),
            Value::Number(Number::from(summary.helpful_count)),
        ),
        (
            "feedback_mixed_count".to_string(),
            Value::Number(Number::from(summary.mixed_count)),
        ),
        (
            "feedback_not_helpful_count".to_string(),
            Value::Number(Number::from(summary.not_helpful_count)),
        ),
        (
            "feedback_note_count".to_string(),
            Value::Number(Number::from(summary.note_count)),
        ),
        (
            "feedback_unresolved_count".to_string(),
            Value::Number(Number::from(summary.unresolved_count)),
        ),
    ]);

    if let Some(last_at) = &summary.last_at {
        patch.insert(
            "feedback_last_at".to_string(),
            Value::String(last_at.clone()),
        );
    }

    patch
}
