use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::fs;

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use memory_api::error::StorageError;
use memory_api::model::entity::EntityManifest;
use memory_api::storage::entity_fs::EntityFs;
use memory_api::storage::entity_store::{EntityStore, ScanReport};
use memory_api::storage::indexed::IndexedEntity;

use crate::default_schema::rule_schema_registry;
use crate::error::RuleError;
use crate::manifest::{RuleId, RuleManifest};

const RULE_MANIFEST_FILE: &str = "rule.toml";
const RULE_LOCK_FILE: &str = ".rule-lock";
const GENERATED_TARGET_TYPE_ID: &str = "generated-target";
const GENERATED_TARGET_ROOT_DIR: &str = "entities";

pub struct RuleStore {
    inner: EntityStore,
    slug_index: HashMap<String, Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedTargetRecord {
    pub id: Uuid,
    pub slug: String,
    pub config_path: String,
    pub target_name: String,
    pub output_path: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuleFilter {
    pub state: Option<String>,
    pub file_kind: Option<String>,
    pub section: Option<String>,
    pub repo_scope: Option<String>,
    pub path_scope: Option<String>,
    pub slug: Option<String>,
    pub has_unresolved_feedback: Option<bool>,
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

    pub fn scan(&mut self, reindex: bool) -> Result<ScanReport, RuleError> {
        let report = self.inner.scan(reindex)?;
        self.rebuild_slug_index()?;
        Ok(report)
    }

    pub fn rebuild_slug_index(&mut self) -> Result<(), RuleError> {
        let mut next = HashMap::new();
        for indexed in self.inner.list_indexed(false)? {
            let manifest = self.inner.fs.read(&indexed.path)?;
            if let Some(slug) = manifest.extra.get("slug").and_then(|value| value.as_str()) {
                next.insert(slug.to_string(), indexed.id);
            }
        }
        self.slug_index = next;
        Ok(())
    }

    pub fn resolve_id(&self, id_or_slug: &str) -> Result<Uuid, RuleError> {
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
        let slug = manifest
            .slug()
            .ok_or_else(|| RuleError::InvalidSlug("missing slug".to_string()))?;
        validate_slug(slug)?;

        if let Some(existing) = self.slug_index.get(slug) {
            if *existing != manifest.id {
                return Err(RuleError::DuplicateSlug(slug.to_string()));
            }
        }

        let root = match target_root {
            Some(path) => path.to_path_buf(),
            None => {
                let roots = self.inner.list_scan_roots()?;
                roots
                    .into_iter()
                    .next()
                    .map(|root| root.path)
                    .unwrap_or_else(|| self.inner.index_root.join("rules"))
            }
        };
        fs::create_dir_all(&root).map_err(StorageError::Io)?;

        let entity = rule_to_entity(manifest);
        self.inner
            .schema_registry()
            .get("rule-entry")
            .ok_or_else(|| RuleError::Asset("missing rule-entry schema".to_string()))?
            .validate_manifest(&entity)
            .map_err(|err| RuleError::Asset(err.to_string()))?;

        let folder = self.inner.fs.create(&entity, &root, manifest.body())?;

        let indexed = IndexedEntity {
            id: manifest.id,
            path: folder.clone(),
            type_id: "rule-entry".to_string(),
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
            Some("rule-entry"),
        )?;
        let _ = self.inner.fs.append_history(&folder, entity.extra.clone(), None);
        self.slug_index.insert(slug.to_string(), manifest.id);

        Ok(manifest.id)
    }

    pub fn get(&self, id_or_slug: &str) -> Result<RuleManifest, RuleError> {
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
                        return Err(RuleError::DuplicateSlug(new_slug.to_string()));
                    }
                }
                let current = self.inner.fs.read(&indexed.path)?;
                if let Some(old_slug) = current.extra.get("slug").and_then(Value::as_str) {
                    self.slug_index.remove(old_slug);
                }
                self.slug_index.insert(new_slug.to_string(), uuid);
            }
        }

        if let Some(next_state) = to_state {
            let current_state = indexed.state.as_deref().unwrap_or("draft");
            if let Some(schema) = self.inner.schema_registry().get("rule-entry") {
                schema
                    .ensure_transition(current_state, next_state)
                    .map_err(|err| RuleError::Asset(err.to_string()))?;
            }
        }

        let updated_entity = self.inner.fs.update(&indexed.path, &patch, to_state)?;
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
            type_id: "rule-entry".to_string(),
            title: title.clone(),
            state: state.clone(),
            created_at: indexed.created_at,
            updated_at: Utc::now(),
            deleted: false,
        };
        self.inner.index.insert_ticket(&refreshed)?;

        let body = self.inner.fs.read_description(&indexed.path).or_else(|| {
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
            Some("rule-entry"),
        )?;

        let _ = self
            .inner
            .fs
            .append_history(&indexed.path, updated_entity.extra.clone(), None);

        Ok(entity_to_rule(&updated_entity))
    }

    pub fn update_body(&self, id_or_slug: &str, body: &str) -> Result<(), RuleError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| RuleError::NotFound(uuid.to_string()))?;
        let patch = BTreeMap::from([(
            "body".to_string(),
            Value::String(body.to_string()),
        )]);
        let updated_entity = self.inner.fs.update(&indexed.path, &patch, None)?;
        self.inner.fs.write_description(&indexed.path, body)?;
        self.inner.search.upsert(
            &uuid,
            updated_entity
                .extra
                .get("title")
                .and_then(Value::as_str),
            Some(body),
            updated_entity
                .extra
                .get("state")
                .and_then(Value::as_str),
            Some("rule-entry"),
        )?;
        let _ = self
            .inner
            .fs
            .append_history(&indexed.path, updated_entity.extra.clone(), None);
        Ok(())
    }

    pub fn list(
        &self,
        filter: &RuleFilter,
        limit: Option<usize>,
    ) -> Result<Vec<RuleManifest>, RuleError> {
        let all = self.inner.list_indexed(false)?;
        let mut rules = Vec::new();

        for indexed in all {
            if let Some(state) = filter.state.as_deref() {
                if indexed.state.as_deref() != Some(state) {
                    continue;
                }
            }
            if indexed.type_id != "rule-entry" {
                continue;
            }

            let rule = self.hydrate_rule(&indexed)?;
            if filter.matches(&rule) {
                rules.push(rule);
            }
        }

        rules.sort_by_key(|rule| (rule.order_key().unwrap_or_default(), rule.slug().unwrap_or("").to_string()));
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
        let candidates = self.inner.search(query, limit.saturating_mul(4).max(limit))?;
        let mut rules = Vec::new();

        for candidate in candidates {
            let indexed = match self.inner.get_indexed(&candidate.id)? {
                Some(indexed) if !indexed.deleted => indexed,
                _ => continue,
            };
            if indexed.type_id != "rule-entry" {
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

    pub fn list_generated_targets(
        &self,
        config_path: &Path,
    ) -> Result<Vec<GeneratedTargetRecord>, RuleError> {
        let config_path = stable_path_key(config_path);
        let mut records = Vec::new();

        for indexed in self.inner.list_indexed(false)? {
            if indexed.type_id != GENERATED_TARGET_TYPE_ID {
                continue;
            }

            let entity = self.inner.fs.read(&indexed.path)?;
            let Some(record) = generated_target_from_entity(indexed.id, &entity) else {
                continue;
            };

            if record.config_path == config_path {
                records.push(record);
            }
        }

        records.sort_by(|left, right| left.target_name.cmp(&right.target_name));
        Ok(records)
    }

    pub fn upsert_generated_target(
        &mut self,
        config_path: &Path,
        target_name: &str,
        output_path: &Path,
    ) -> Result<GeneratedTargetRecord, RuleError> {
        let config_path = stable_path_key(config_path);
        let output_path = stable_path_key(output_path);
        let slug = generated_target_slug(&config_path, target_name);

        if let Some(existing_id) = self.slug_index.get(&slug).copied() {
            let indexed = self
                .inner
                .get_indexed(&existing_id)?
                .ok_or_else(|| RuleError::NotFound(existing_id.to_string()))?;

            if indexed.type_id != GENERATED_TARGET_TYPE_ID {
                return Err(RuleError::DuplicateSlug(slug));
            }

            let patch = BTreeMap::from([
                (
                    "title".to_string(),
                    Value::String(target_name.to_string()),
                ),
                (
                    "config_path".to_string(),
                    Value::String(config_path.clone()),
                ),
                (
                    "target_name".to_string(),
                    Value::String(target_name.to_string()),
                ),
                (
                    "output_path".to_string(),
                    Value::String(output_path.clone()),
                ),
            ]);
            let updated = self.inner.fs.update(&indexed.path, &patch, Some("active"))?;

            let refreshed = IndexedEntity {
                id: existing_id,
                path: indexed.path.clone(),
                type_id: GENERATED_TARGET_TYPE_ID.to_string(),
                title: Some(target_name.to_string()),
                state: Some("active".to_string()),
                created_at: indexed.created_at,
                updated_at: Utc::now(),
                deleted: false,
            };
            self.inner.index.insert_ticket(&refreshed)?;
            self.inner.search.upsert(
                &existing_id,
                Some(target_name),
                Some(&output_path),
                Some("active"),
                Some(GENERATED_TARGET_TYPE_ID),
            )?;
            let _ = self
                .inner
                .fs
                .append_history(&indexed.path, updated.extra.clone(), None);

            return generated_target_from_entity(existing_id, &updated)
                .ok_or_else(|| RuleError::Asset("invalid generated-target manifest".to_string()));
        }

        let id = Uuid::new_v4();
        let entity = generated_target_entity(id, &slug, &config_path, target_name, &output_path);
        self.inner
            .schema_registry()
            .get(GENERATED_TARGET_TYPE_ID)
            .ok_or_else(|| RuleError::Asset("missing generated-target schema".to_string()))?
            .validate_manifest(&entity)
            .map_err(|err| RuleError::Asset(err.to_string()))?;

        let root = self.inner.index_root.join(GENERATED_TARGET_ROOT_DIR);
        fs::create_dir_all(&root).map_err(StorageError::Io)?;
        let folder = self.inner.fs.create(&entity, &root, Some(&output_path))?;
        let indexed = IndexedEntity {
            id,
            path: folder.clone(),
            type_id: GENERATED_TARGET_TYPE_ID.to_string(),
            title: Some(target_name.to_string()),
            state: Some("active".to_string()),
            created_at: entity.created_at,
            updated_at: Utc::now(),
            deleted: false,
        };
        self.inner.index.insert_ticket(&indexed)?;
        self.inner.search.upsert(
            &id,
            Some(target_name),
            Some(&output_path),
            Some("active"),
            Some(GENERATED_TARGET_TYPE_ID),
        )?;
        let _ = self.inner.fs.append_history(&folder, entity.extra.clone(), None);
        self.slug_index.insert(slug, id);

        generated_target_from_entity(id, &entity)
            .ok_or_else(|| RuleError::Asset("invalid generated-target manifest".to_string()))
    }

    pub fn delete_generated_target(&mut self, slug: &str) -> Result<(), RuleError> {
        let uuid = self.resolve_id(slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| RuleError::NotFound(uuid.to_string()))?;

        if indexed.type_id != GENERATED_TARGET_TYPE_ID {
            return Err(RuleError::NotFound(slug.to_string()));
        }

        let entity = self.inner.fs.read(&indexed.path)?;
        if let Some(existing_slug) = entity.extra.get("slug").and_then(Value::as_str) {
            self.slug_index.remove(existing_slug);
        }
        self.inner.fs.mark_deleted(&indexed.path)?;

        let mut refreshed = indexed.clone();
        refreshed.deleted = true;
        refreshed.updated_at = Utc::now();
        self.inner.index.insert_ticket(&refreshed)?;

        Ok(())
    }

    fn resolve_prefix(&self, prefix: &str) -> Result<Option<Uuid>, RuleError> {
        if prefix.len() < 4 {
            return Ok(None);
        }

        let indexed = self.inner.list_indexed(false)?;
        let matches: Vec<_> = indexed
            .iter()
            .filter(|entity| entity.id.to_string().starts_with(prefix))
            .collect();

        match matches.len() {
            0 => Ok(None),
            1 => Ok(Some(matches[0].id)),
            _ => Err(RuleError::AmbiguousPrefix(prefix.to_string())),
        }
    }

    fn hydrate_rule(&self, indexed: &IndexedEntity) -> Result<RuleManifest, RuleError> {
        let entity = self.inner.fs.read(&indexed.path)?;
        let mut rule = entity_to_rule(&entity);
        if let Some(body) = self.inner.fs.read_description(&indexed.path) {
            rule.set_body(&body);
        }
        Ok(rule)
    }
}

impl RuleFilter {
    fn matches(&self, rule: &RuleManifest) -> bool {
        if let Some(file_kind) = self.file_kind.as_deref() {
            if rule.file_kind() != Some(file_kind) {
                return false;
            }
        }
        if let Some(section) = self.section.as_deref() {
            if rule.section() != Some(section) {
                return false;
            }
        }
        if let Some(repo_scope) = self.repo_scope.as_deref() {
            if !rule.repo_scopes().iter().any(|scope| scope == repo_scope) {
                return false;
            }
        }
        if let Some(path_scope) = self.path_scope.as_deref() {
            if !rule.path_scopes().iter().any(|scope| scope == path_scope) {
                return false;
            }
        }
        if let Some(slug) = self.slug.as_deref() {
            if rule.slug() != Some(slug) {
                return false;
            }
        }
        if let Some(has_unresolved_feedback) = self.has_unresolved_feedback {
            let unresolved = rule.feedback_unresolved_count().unwrap_or_default() > 0;
            if unresolved != has_unresolved_feedback {
                return false;
            }
        }
        true
    }
}

fn validate_slug(slug: &str) -> Result<(), RuleError> {
    if slug.is_empty() {
        return Err(RuleError::InvalidSlug("slug cannot be empty".to_string()));
    }

    let valid = slug.chars().all(|ch| {
        ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, '/' | '-' | '_' | '.')
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

fn generated_target_entity(
    id: Uuid,
    slug: &str,
    config_path: &str,
    target_name: &str,
    output_path: &str,
) -> EntityManifest {
    EntityManifest {
        id,
        created_at: Utc::now(),
        extra: BTreeMap::from([
            ("slug".to_string(), Value::String(slug.to_string())),
            (
                "title".to_string(),
                Value::String(target_name.to_string()),
            ),
            (
                "type".to_string(),
                Value::String(GENERATED_TARGET_TYPE_ID.to_string()),
            ),
            (
                "state".to_string(),
                Value::String("active".to_string()),
            ),
            (
                "config_path".to_string(),
                Value::String(config_path.to_string()),
            ),
            (
                "target_name".to_string(),
                Value::String(target_name.to_string()),
            ),
            (
                "output_path".to_string(),
                Value::String(output_path.to_string()),
            ),
        ]),
    }
}

fn generated_target_from_entity(
    id: Uuid,
    entity: &EntityManifest,
) -> Option<GeneratedTargetRecord> {
    Some(GeneratedTargetRecord {
        id,
        slug: entity.extra.get("slug")?.as_str()?.to_string(),
        config_path: entity.extra.get("config_path")?.as_str()?.to_string(),
        target_name: entity.extra.get("target_name")?.as_str()?.to_string(),
        output_path: entity.extra.get("output_path")?.as_str()?.to_string(),
    })
}

fn generated_target_slug(config_path: &str, target_name: &str) -> String {
    format!(
        "generated-targets/{}/{}",
        sanitize_slug_fragment(config_path),
        sanitize_slug_fragment(target_name)
    )
}

fn sanitize_slug_fragment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_lowercase()
                || ch.is_ascii_uppercase()
                || ch.is_ascii_digit()
                || matches!(ch, '/' | '-' | '_' | '.')
            {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}

fn stable_path_key(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn create_and_get_rule_by_slug() {
        let dir = tempdir().unwrap();
        let mut store = RuleStore::open(dir.path()).unwrap();
        let manifest = RuleManifest::new(
            "shared/agents/discovery-protocol",
            "Discovery Protocol",
            "AGENTS",
            "discovery-protocol",
            "Use live sources first.",
        );

        let id = store.create(&manifest, None).unwrap();
        let fetched = store.get("shared/agents/discovery-protocol").unwrap();

        assert_eq!(fetched.id, id);
        assert_eq!(fetched.slug(), manifest.slug());
        assert_eq!(fetched.body(), manifest.body());
    }

    #[test]
    fn open_rebuilds_slug_index_for_fresh_processes() {
        let dir = tempdir().unwrap();
        let mut store = RuleStore::open(dir.path()).unwrap();
        let manifest = RuleManifest::new(
            "shared/agents/reopen-test",
            "Reopen Test",
            "AGENTS",
            "operating-principles",
            "Persist slug lookup across store instances.",
        );
        store.create(&manifest, None).unwrap();
        drop(store);

        let reopened = RuleStore::open(dir.path()).unwrap();
        let fetched = reopened.get("shared/agents/reopen-test").unwrap();

        assert_eq!(fetched.slug(), Some("shared/agents/reopen-test"));
    }

    #[test]
    fn list_filters_and_sorts_rules_by_metadata() {
        let dir = tempdir().unwrap();
        let mut store = RuleStore::open(dir.path()).unwrap();

        let mut first = RuleManifest::new(
            "shared/agents/discovery-protocol",
            "Discovery Protocol",
            "AGENTS",
            "discovery-protocol",
            "Use live sources first.",
        );
        first.set_order_key(20);
        first.set_repo_scopes(["context-engine", "memory-viewers"]);
        first.set_path_scopes([".github/instructions/tests.instructions.md"]);
        first.set_feedback_summary(1, 0, 0, 1, 1, Some("2026-05-07T14:00:00Z"));

        let mut second = RuleManifest::new(
            "shared/github/readme/overview",
            "Overview",
            ".github/README",
            "overview",
            "Project overview.",
        );
        second.set_order_key(10);
        second.set_repo_scopes(["memory-api"]);
        second.set_path_scopes([".github/README.md"]);

        let mut third = RuleManifest::new(
            "shared/agents/quality-gates",
            "Quality Gates",
            "AGENTS",
            "quality-gates",
            "Run relevant tests.",
        );
        third.set_order_key(5);
        third.set_repo_scopes(["context-engine"]);
        third.set_path_scopes(["AGENTS.md"]);

        store.create(&first, None).unwrap();
        store.create(&second, None).unwrap();
        store.create(&third, None).unwrap();

        let filtered = store
            .list(
                &RuleFilter {
                    file_kind: Some("AGENTS".to_string()),
                    repo_scope: Some("context-engine".to_string()),
                    path_scope: Some("AGENTS.md".to_string()),
                    has_unresolved_feedback: Some(false),
                    ..RuleFilter::default()
                },
                None,
            )
            .unwrap();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].slug(), Some("shared/agents/quality-gates"));
    }

    #[test]
    fn search_can_filter_rule_results_after_full_text_match() {
        let dir = tempdir().unwrap();
        let mut store = RuleStore::open(dir.path()).unwrap();

        let mut shared = RuleManifest::new(
            "shared/github/readme/overview",
            "Overview",
            ".github/README",
            "overview",
            "Canonical project overview for all repos.",
        );
        shared.set_repo_scopes(["context-engine"]);
        shared.set_path_scopes([".github/README.md"]);

        let mut memory = RuleManifest::new(
            "memory-api/github/readme/overview",
            "Overview",
            ".github/README",
            "overview",
            "Canonical project overview for memory-api only.",
        );
        memory.set_repo_scopes(["memory-api"]);
        memory.set_path_scopes([".github/README.md"]);

        store.create(&shared, None).unwrap();
        store.create(&memory, None).unwrap();

        let filtered = store
            .search(
                "overview",
                &RuleFilter {
                    repo_scope: Some("memory-api".to_string()),
                    ..RuleFilter::default()
                },
                10,
            )
            .unwrap();

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].slug(), Some("memory-api/github/readme/overview"));
    }

    #[test]
    fn update_changes_slug_state_and_body() {
        let dir = tempdir().unwrap();
        let mut store = RuleStore::open(dir.path()).unwrap();
        let manifest = RuleManifest::new(
            "shared/agents/update-test",
            "Update Test",
            "AGENTS",
            "update-test",
            "Initial body.",
        );
        store.create(&manifest, None).unwrap();

        store
            .update_body("shared/agents/update-test", "Updated body.")
            .unwrap();
        let updated = store
            .update(
                "shared/agents/update-test",
                BTreeMap::from([
                    (
                        "slug".to_string(),
                        Value::String("shared/agents/update-test-renamed".to_string()),
                    ),
                    (
                        "title".to_string(),
                        Value::String("Updated Test".to_string()),
                    ),
                ]),
                Some("reviewed"),
            )
            .unwrap();

        assert_eq!(updated.slug(), Some("shared/agents/update-test-renamed"));
        assert_eq!(updated.title(), Some("Updated Test"));
        assert_eq!(updated.state(), Some("reviewed"));
        assert_eq!(updated.body(), Some("Updated body."));

        let fetched = store.get("shared/agents/update-test-renamed").unwrap();
        assert_eq!(fetched.body(), Some("Updated body."));
    }

    #[test]
    fn generated_target_records_round_trip_and_delete() {
        let dir = tempdir().unwrap();
        let mut store = RuleStore::open(dir.path()).unwrap();
        let config_path = dir.path().join("rule-targets.yaml");
        let output_path = dir.path().join(".github/README.md");

        let record = store
            .upsert_generated_target(&config_path, "context-engine-github-readme", &output_path)
            .unwrap();

        let listed = store.list_generated_targets(&config_path).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0], record);

        store.delete_generated_target(&record.slug).unwrap();
        assert!(store.list_generated_targets(&config_path).unwrap().is_empty());
    }

    #[test]
    fn generated_target_upsert_updates_existing_output_path() {
        let dir = tempdir().unwrap();
        let mut store = RuleStore::open(dir.path()).unwrap();
        let config_path = dir.path().join("rule-targets.yaml");
        let first_output = dir.path().join("memory-viewers/.github/README.md");
        let second_output = dir.path().join(".github/README.md");

        let created = store
            .upsert_generated_target(&config_path, "github-readme", &first_output)
            .unwrap();
        let updated = store
            .upsert_generated_target(&config_path, "github-readme", &second_output)
            .unwrap();

        assert_eq!(created.id, updated.id);
        assert_ne!(created.output_path, updated.output_path);
        assert_eq!(
            store.list_generated_targets(&config_path).unwrap()[0].output_path,
            updated.output_path
        );
    }
}