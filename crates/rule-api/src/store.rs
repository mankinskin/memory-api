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
use tracing::field::Empty;
use uuid::Uuid;

use memory_api::{
    error::StorageError,
    model::{
        entity::EntityManifest,
        filesystem::EntityFolderConfig,
    },
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
const RULE_STORE_TRACE_TARGET: &str = "rule_api::store";

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
        let _span_guard = tracing::info_span!(
            target: RULE_STORE_TRACE_TARGET,
            "rule_store_open",
            requested_root = %index_root.display(),
        )
        .entered();
        let index_root =
            workspace::resolve_store_root_from(index_root, ".rule");
        if !index_root.join("entities.db").is_file() {
            return Err(
                StorageError::WorkspaceNotFound { path: index_root }.into()
            );
        }
        let store = Self::open_internal(&index_root)?;
        tracing::info!(
            target: RULE_STORE_TRACE_TARGET,
            resolved_root = %index_root.display(),
            "rule_store_open_complete"
        );
        Ok(store)
    }

    /// Initialize a new rule store rooted at `index_root`.
    ///
    /// Creates the workspace directory and all required index files. Idempotent:
    /// if the workspace already exists it is opened without error.
    pub fn init(index_root: &Path) -> Result<Self, RuleError> {
        let _span_guard = tracing::info_span!(
            target: RULE_STORE_TRACE_TARGET,
            "rule_store_init",
            requested_root = %index_root.display(),
        )
        .entered();
        let index_root =
            workspace::resolve_store_root_from(index_root, ".rule");
        let store = Self::open_internal(&index_root)?;
        tracing::info!(
            target: RULE_STORE_TRACE_TARGET,
            resolved_root = %index_root.display(),
            "rule_store_init_complete"
        );
        Ok(store)
    }

    /// Open an existing rule store, or initialize and force-scan it when the
    /// local derived index artifacts do not exist yet.
    pub fn open_or_init(index_root: &Path) -> Result<Self, RuleError> {
        let span = tracing::info_span!(
            target: RULE_STORE_TRACE_TARGET,
            "rule_store_open_or_init",
            requested_root = %index_root.display(),
            initialized_store = Empty,
        );
        let _span_guard = span.enter();
        match Self::open(index_root) {
            Ok(store) => {
                span.record("initialized_store", false);
                tracing::info!(
                    target: RULE_STORE_TRACE_TARGET,
                    initialized_store = false,
                    "rule_store_open_or_init_complete"
                );
                Ok(store)
            },
            Err(RuleError::Storage(StorageError::WorkspaceNotFound {
                ..
            })) => {
                span.record("initialized_store", true);
                let mut store = Self::init(index_root)?;
                store.scan(true)?;
                tracing::info!(
                    target: RULE_STORE_TRACE_TARGET,
                    initialized_store = true,
                    "rule_store_open_or_init_complete"
                );
                Ok(store)
            },
            Err(error) => Err(error),
        }
    }

    fn open_internal(index_root: &Path) -> Result<Self, RuleError> {
        let _span_guard = tracing::debug_span!(
            target: RULE_STORE_TRACE_TARGET,
            "rule_store_open_internal",
            resolved_root = %index_root.display(),
        )
        .entered();
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
        tracing::debug!(
            target: RULE_STORE_TRACE_TARGET,
            scan_root = %index_root.join("rules").display(),
            "rule_store_default_scan_root_registered"
        );
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
        let _span_guard = tracing::info_span!(
            target: RULE_STORE_TRACE_TARGET,
            "rule_store_scan",
            reindex,
            slug_entries = Empty,
        )
        .entered();
        let report = self.inner.scan(reindex)?;
        if reindex {
            self.reindex_rule_bodies()?;
        }
        self.rebuild_slug_index()?;
        let slug_entries = self.slug_index.len();
        tracing::Span::current().record("slug_entries", slug_entries);
        tracing::info!(
            target: RULE_STORE_TRACE_TARGET,
            reindex,
            integrated = report.integrated,
            pruned = report.pruned,
            diagnostics = report.diagnostics.len(),
            slug_entries,
            "rule_store_scan_complete"
        );
        Ok(report)
    }

    fn reindex_rule_bodies(&self) -> Result<(), RuleError> {
        let _span_guard = tracing::debug_span!(
            target: RULE_STORE_TRACE_TARGET,
            "rule_store_reindex_rule_bodies",
            indexed_entities = Empty,
            reindexed_rules = Empty,
        )
        .entered();
        let all = self.inner.list_indexed()?;
        tracing::Span::current().record("indexed_entities", all.len());
        let mut reindexed_rules = 0usize;
        for indexed in all {
            if indexed.type_id != RULE_ENTRY_TYPE_ID {
                continue;
            }

            let entity = self.read_indexed_manifest(&indexed)?;
            let title = entity.extra.get("title").and_then(Value::as_str);
            let state = entity.extra.get("state").and_then(Value::as_str);
            let body = self.read_rule_body(&indexed.path, Some(&entity));
            let created_at_str = indexed.created_at.to_rfc3339();
            let effort_str = entity.extra.get("effort").and_then(|v| match v {
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
            reindexed_rules += 1;
        }

        tracing::Span::current().record("reindexed_rules", reindexed_rules);
        tracing::debug!(
            target: RULE_STORE_TRACE_TARGET,
            reindexed_rules,
            "rule_store_reindex_rule_bodies_complete"
        );

        Ok(())
    }

    pub fn rebuild_slug_index(&mut self) -> Result<(), RuleError> {
        let _span_guard = tracing::debug_span!(
            target: RULE_STORE_TRACE_TARGET,
            "rule_store_rebuild_slug_index",
            indexed_entities = Empty,
            slug_entries = Empty,
        )
        .entered();
        let mut next = HashMap::new();
        let all = self.inner.list_indexed()?;
        tracing::Span::current().record("indexed_entities", all.len());
        for indexed in all {
            let manifest = self.read_indexed_manifest(&indexed)?;
            if let Some(slug) =
                manifest.extra.get("slug").and_then(Value::as_str)
            {
                next.insert(slug.to_string(), indexed.id);
            }
        }
        self.slug_index = next;
        let slug_entries = self.slug_index.len();
        tracing::Span::current().record("slug_entries", slug_entries);
        tracing::debug!(
            target: RULE_STORE_TRACE_TARGET,
            slug_entries,
            "rule_store_rebuild_slug_index_complete"
        );
        Ok(())
    }

    fn prune_missing_index_entries(&mut self) -> Result<(), RuleError> {
        let stale_ids: Vec<_> = self
            .inner
            .list_indexed()?
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
        let _span_guard = tracing::debug_span!(
            target: RULE_STORE_TRACE_TARGET,
            "rule_store_resolve_id",
            input = id_or_slug,
        )
        .entered();
        if let Ok(uuid) = id_or_slug.parse::<Uuid>() {
            tracing::debug!(
                target: RULE_STORE_TRACE_TARGET,
                resolution = "uuid",
                resolved_id = %uuid,
                "rule_store_resolve_id_complete"
            );
            return Ok(uuid);
        }

        if let Some(uuid) = self.resolve_prefix(id_or_slug)? {
            tracing::debug!(
                target: RULE_STORE_TRACE_TARGET,
                resolution = "prefix",
                resolved_id = %uuid,
                "rule_store_resolve_id_complete"
            );
            return Ok(uuid);
        }

        let resolved = self
            .slug_index
            .get(id_or_slug)
            .copied()
            .ok_or_else(|| {
                RuleError::NotFound(format!(
                    "{}; {}",
                    id_or_slug,
                    crate::workspace::workspace_recovery_hint(
                        &self.inner.index_root
                    )
                ))
            })?;
        tracing::debug!(
            target: RULE_STORE_TRACE_TARGET,
            resolution = "slug",
            resolved_id = %resolved,
            "rule_store_resolve_id_complete"
        );
        Ok(resolved)
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
            },
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
        };
        self.inner.index.insert_ticket(&indexed)?;
        let created_at_str = manifest.created_at.to_rfc3339();
        let effort_str = entity.extra.get("effort").and_then(|v| match v {
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

        if !matches!(
            indexed.type_id.as_str(),
            RULE_ENTRY_TYPE_ID | GENERATED_TARGET_TYPE_ID
        ) {
            return Err(RuleError::NotFound(id_or_slug.to_string()));
        }

        let entity = self.inner.fs.read(&indexed.path)?;
        if let Some(existing_slug) =
            entity.extra.get("slug").and_then(Value::as_str)
        {
            self.slug_index.remove(existing_slug);
        }

        self.inner.fs.delete(&indexed.path)?;
        self.inner.index.remove_ticket(&uuid)?;
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

        self.apply_slug_patch_if_present(uuid, &indexed.path, &patch)?;
        self.validate_update_transition(indexed.state.as_deref(), to_state)?;

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
        };
        self.inner.index.insert_ticket(&refreshed)?;

        let body = self.read_rule_body(&indexed.path, Some(&updated_entity));
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

    fn apply_slug_patch_if_present(
        &mut self,
        uuid: Uuid,
        indexed_path: &Path,
        patch: &BTreeMap<String, Value>,
    ) -> Result<(), RuleError> {
        let Some(new_slug_value) = patch.get("slug") else {
            return Ok(());
        };
        let Some(new_slug) = new_slug_value.as_str() else {
            return Ok(());
        };

        validate_slug(new_slug)?;
        if let Some(existing) = self.slug_index.get(new_slug) {
            if *existing != uuid {
                return Err(RuleError::DuplicateSlug(new_slug.to_string()));
            }
        }

        let current = self.inner.fs.read(indexed_path)?;
        if let Some(old_slug) =
            current.extra.get("slug").and_then(Value::as_str)
        {
            self.slug_index.remove(old_slug);
        }
        self.slug_index.insert(new_slug.to_string(), uuid);
        Ok(())
    }

    fn validate_update_transition(
        &self,
        current_state: Option<&str>,
        to_state: Option<&str>,
    ) -> Result<(), RuleError> {
        let Some(next_state) = to_state else {
            return Ok(());
        };
        let from_state = current_state.unwrap_or("draft");
        if let Some(schema) = self.inner.schema_registry().get(RULE_ENTRY_TYPE_ID)
        {
            schema
                .ensure_transition(from_state, next_state)
                .map_err(|err| RuleError::Asset(err.to_string()))?;
        }
        Ok(())
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
        let effort_str =
            updated_entity.extra.get("effort").and_then(|v| match v {
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
        if indexed.type_id != RULE_ENTRY_TYPE_ID {
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

        for indexed in self.inner.list_indexed()? {
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
                Some(indexed) => indexed,
                None => continue,
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
            .list_indexed()?
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


#[path = "store_helpers.rs"]
mod store_helpers;
use store_helpers::*;
