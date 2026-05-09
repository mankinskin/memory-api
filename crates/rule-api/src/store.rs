mod filter;
mod generated_targets;

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
    path::Path,
};

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use memory_api::{
    error::StorageError,
    model::entity::EntityManifest,
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
    default_schema::rule_schema_registry,
    error::RuleError,
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

pub struct RuleStore {
    inner: EntityStore,
    slug_index: HashMap<String, Uuid>,
}

impl RuleStore {
    pub fn open(index_root: &Path) -> Result<Self, RuleError> {
        let fs = EntityFs::new(RULE_MANIFEST_FILE, RULE_LOCK_FILE);
        let registry = rule_schema_registry();
        let inner = EntityStore::open_with(index_root, fs, registry)?;
        let mut store = Self {
            inner,
            slug_index: HashMap::new(),
        };
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
        self.rebuild_slug_index()?;
        Ok(report)
    }

    pub fn rebuild_slug_index(&mut self) -> Result<(), RuleError> {
        let mut next = HashMap::new();
        for indexed in self.inner.list_indexed(false)? {
            let manifest = self.inner.fs.read(&indexed.path)?;
            if let Some(slug) =
                manifest.extra.get("slug").and_then(Value::as_str)
            {
                next.insert(slug.to_string(), indexed.id);
            }
        }
        self.slug_index = next;
        Ok(())
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
            Some(path) => path.to_path_buf(),
            None => self
                .inner
                .list_scan_roots()?
                .into_iter()
                .next()
                .map(|root| root.path)
                .unwrap_or_else(|| self.inner.index_root.join("rules")),
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
        self.inner.search.upsert(
            &manifest.id,
            manifest.title(),
            manifest.body(),
            manifest.state(),
            Some(RULE_ENTRY_TYPE_ID),
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

    pub fn update(
        &mut self,
        id_or_slug: &str,
        patch: BTreeMap<String, Value>,
        to_state: Option<&str>,
    ) -> Result<RuleManifest, RuleError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| RuleError::NotFound(uuid.to_string()))?;

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

        let body =
            self.inner.fs.read_description(&indexed.path).or_else(|| {
                updated_entity
                    .extra
                    .get("body")
                    .and_then(Value::as_str)
                    .map(str::to_string)
            });
        self.inner.search.upsert(
            &uuid,
            title.as_deref(),
            body.as_deref(),
            state.as_deref(),
            Some(RULE_ENTRY_TYPE_ID),
        )?;

        let _ = self.inner.fs.append_history(
            &indexed.path,
            updated_entity.extra.clone(),
            None,
        );

        Ok(entity_to_rule(&updated_entity))
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
        let patch = BTreeMap::from([(
            "body".to_string(),
            Value::String(body.to_string()),
        )]);
        let updated_entity =
            self.inner.fs.update(&indexed.path, &patch, None)?;
        self.inner.fs.write_description(&indexed.path, body)?;
        self.inner.search.upsert(
            &uuid,
            updated_entity.extra.get("title").and_then(Value::as_str),
            Some(body),
            updated_entity.extra.get("state").and_then(Value::as_str),
            Some(RULE_ENTRY_TYPE_ID),
        )?;
        let _ = self.inner.fs.append_history(
            &indexed.path,
            updated_entity.extra.clone(),
            None,
        );
        Ok(())
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
        let entity = self.inner.fs.read(&indexed.path)?;
        let mut rule = entity_to_rule(&entity);
        if let Some(body) = self.inner.fs.read_description(&indexed.path) {
            rule.set_body(&body);
        }
        Ok(rule)
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
    EntityManifest {
        id: rule.id,
        created_at: rule.created_at,
        extra: rule.extra.clone(),
    }
}

fn entity_to_rule(entity: &EntityManifest) -> RuleManifest {
    RuleManifest {
        id: entity.id,
        created_at: entity.created_at,
        extra: entity.extra.clone(),
    }
}
