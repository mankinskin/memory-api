use std::{
    collections::BTreeMap,
    fs,
    path::Path,
};

use chrono::Utc;
use serde_json::Value;
use uuid::Uuid;

use memory_api::{
    error::StorageError,
    model::entity::EntityManifest,
    storage::indexed::IndexedEntity,
};

use crate::error::RuleError;

use super::{
    GENERATED_TARGET_ROOT_DIR,
    GENERATED_TARGET_TYPE_ID,
    RuleStore,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedTargetRecord {
    pub id: Uuid,
    pub slug: String,
    pub config_path: String,
    pub target_name: String,
    pub output_path: String,
}

impl RuleStore {
    pub fn list_generated_targets(
        &self,
        config_path: &Path,
    ) -> Result<Vec<GeneratedTargetRecord>, RuleError> {
        let config_path = stable_path_key(config_path);
        let mut records = Vec::new();

        for indexed in self.inner.list_indexed()? {
            if indexed.type_id != GENERATED_TARGET_TYPE_ID {
                continue;
            }

            let entity = self.inner.fs.read(&indexed.path)?;
            let Some(record) =
                generated_target_from_entity(indexed.id, &entity)
            else {
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
                ("title".to_string(), Value::String(target_name.to_string())),
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
            let updated =
                self.inner
                    .fs
                    .update(&indexed.path, &patch, Some("active"))?;

            let refreshed = IndexedEntity {
                id: existing_id,
                path: indexed.path.clone(),
                type_id: GENERATED_TARGET_TYPE_ID.to_string(),
                title: Some(target_name.to_string()),
                state: Some("active".to_string()),
                created_at: indexed.created_at,
                updated_at: Utc::now(),
            };
            self.inner.index.insert_ticket(&refreshed)?;
            let created_at_str = indexed.created_at.to_rfc3339();
            let effort_str = updated.extra.get("effort")
                .and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                });
            self.inner.search.upsert(
                &existing_id,
                Some(target_name),
                Some(&output_path),
                Some("active"),
                Some(GENERATED_TARGET_TYPE_ID),
                Some(&created_at_str),
                effort_str.as_deref(),
            )?;
            let _ = self.inner.fs.append_history(
                &indexed.path,
                updated.extra.clone(),
                None,
            );

            return generated_target_from_entity(existing_id, &updated)
                .ok_or_else(|| {
                    RuleError::Asset(
                        "invalid generated-target manifest".to_string(),
                    )
                });
        }

        let id = Uuid::new_v4();
        let entity = generated_target_entity(
            id,
            &slug,
            &config_path,
            target_name,
            &output_path,
        );
        self.inner
            .schema_registry()
            .get(GENERATED_TARGET_TYPE_ID)
            .ok_or_else(|| {
                RuleError::Asset("missing generated-target schema".to_string())
            })?
            .validate_manifest(&entity)
            .map_err(|err| RuleError::Asset(err.to_string()))?;

        let root = self.inner.index_root.join(GENERATED_TARGET_ROOT_DIR);
        fs::create_dir_all(&root).map_err(StorageError::Io)?;
        let folder =
            self.inner.fs.create(&entity, &root, Some(&output_path))?;
        let indexed = IndexedEntity {
            id,
            path: folder.clone(),
            type_id: GENERATED_TARGET_TYPE_ID.to_string(),
            title: Some(target_name.to_string()),
            state: Some("active".to_string()),
            created_at: entity.created_at,
            updated_at: Utc::now(),
        };
        self.inner.index.insert_ticket(&indexed)?;
        let created_at_str = entity.created_at.to_rfc3339();
        let effort_str = entity.extra.get("effort")
            .and_then(|v| match v {
                serde_json::Value::String(s) => Some(s.clone()),
                serde_json::Value::Number(n) => Some(n.to_string()),
                _ => None,
            });
        self.inner.search.upsert(
            &id,
            Some(target_name),
            Some(&output_path),
            Some("active"),
            Some(GENERATED_TARGET_TYPE_ID),
            Some(&created_at_str),
            effort_str.as_deref(),
        )?;
        let _ =
            self.inner
                .fs
                .append_history(&folder, entity.extra.clone(), None);
        self.slug_index.insert(slug, id);

        generated_target_from_entity(id, &entity).ok_or_else(|| {
            RuleError::Asset("invalid generated-target manifest".to_string())
        })
    }

    pub fn delete_generated_target(
        &mut self,
        slug: &str,
    ) -> Result<(), RuleError> {
        let uuid = self.resolve_id(slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| RuleError::NotFound(uuid.to_string()))?;

        if indexed.type_id != GENERATED_TARGET_TYPE_ID {
            return Err(RuleError::NotFound(slug.to_string()));
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
            ("title".to_string(), Value::String(target_name.to_string())),
            (
                "type".to_string(),
                Value::String(GENERATED_TARGET_TYPE_ID.to_string()),
            ),
            ("state".to_string(), Value::String("active".to_string())),
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

fn generated_target_slug(
    config_path: &str,
    target_name: &str,
) -> String {
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
