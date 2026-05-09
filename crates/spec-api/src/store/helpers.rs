use std::fs;
use std::path::Path;

use memory_api::error::StorageError;
use memory_api::model::entity::EntityManifest;

use crate::error::SpecError;
use crate::manifest::SpecManifest;

pub(super) fn spec_to_entity(spec: &SpecManifest) -> EntityManifest {
    let mut extra = spec.extra.clone();
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

pub(super) fn entity_to_spec(entity: &EntityManifest) -> SpecManifest {
    let mut extra = entity.extra.clone();
    let code_refs = extra
        .remove("code_refs")
        .and_then(|value| serde_json::from_value(value).ok())
        .unwrap_or_default();
    SpecManifest {
        id: entity.id,
        created_at: entity.created_at,
        code_refs,
        extra,
    }
}

pub(super) fn read_spec_manifest(spec_path: &Path) -> Result<SpecManifest, SpecError> {
    let manifest_path = spec_path.join(super::SPEC_MANIFEST_FILE);
    let content = fs::read_to_string(&manifest_path)
        .map_err(|error| SpecError::Storage(StorageError::Io(error)))?;
    toml::from_str(&content).map_err(|error| SpecError::Serialization(error.to_string()))
}

pub(super) fn normalize_section_name(name: &str) -> String {
    if name.ends_with(".md") {
        name.to_string()
    } else {
        format!("{}.md", name)
    }
}

pub(super) fn read_body(spec_path: &Path) -> String {
    let body_path = spec_path.join("body.md");
    fs::read_to_string(&body_path).unwrap_or_default()
}

pub(super) fn write_body(spec_path: &Path, content: &str) -> Result<(), SpecError> {
    let body_path = spec_path.join("body.md");
    fs::write(&body_path, content).map_err(|error| SpecError::Storage(StorageError::Io(error)))
}