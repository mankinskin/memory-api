use std::{
    fs,
    path::Path,
};

use memory_api::{
    error::StorageError,
    model::entity::EntityManifest,
};

use crate::{
    error::SpecError,
    manifest::SpecManifest,
};

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

pub(super) fn read_spec_manifest(
    spec_path: &Path
) -> Result<SpecManifest, SpecError> {
    let manifest_path = spec_path.join(super::SPEC_MANIFEST_FILE);
    let content = fs::read_to_string(&manifest_path)
        .map_err(|error| SpecError::Storage(StorageError::Io(error)))?;
    toml::from_str(&content)
        .map_err(|error| SpecError::Serialization(error.to_string()))
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
    read_markdown_file(&body_path)
}

pub(super) fn write_body(
    spec_path: &Path,
    content: &str,
) -> Result<(), SpecError> {
    let body_path = spec_path.join("body.md");
    write_markdown_file(&body_path, content)
}

pub(super) fn read_section(
    spec_path: &Path,
    name: &str,
) -> String {
    let file_name = normalize_section_name(name);
    let path = spec_path.join("sections").join(file_name);
    read_markdown_file(&path)
}

pub(super) fn write_section(
    spec_path: &Path,
    name: &str,
    content: &str,
) -> Result<(), SpecError> {
    let file_name = normalize_section_name(name);
    let sections_dir = spec_path.join("sections");
    fs::create_dir_all(&sections_dir)
        .map_err(|error| SpecError::Storage(StorageError::Io(error)))?;
    let path = sections_dir.join(file_name);
    write_markdown_file(&path, content)
}

fn read_markdown_file(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn write_markdown_file(
    path: &Path,
    content: &str,
) -> Result<(), SpecError> {
    fs::write(path, content)
        .map_err(|error| SpecError::Storage(StorageError::Io(error)))
}
