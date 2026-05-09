use std::fs;

use memory_api::error::StorageError;

use crate::error::SpecError;

use super::{
    SpecStore,
    helpers::normalize_section_name,
};

impl SpecStore {
    pub fn add_section(
        &self,
        id_or_slug: &str,
        name: &str,
        content: &str,
    ) -> Result<(), SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        let sections_dir = indexed.path.join("sections");
        fs::create_dir_all(&sections_dir).map_err(StorageError::Io)?;
        let file_name = normalize_section_name(name);
        fs::write(sections_dir.join(&file_name), content)
            .map_err(StorageError::Io)?;
        Ok(())
    }

    pub fn update_section(
        &self,
        id_or_slug: &str,
        name: &str,
        content: &str,
    ) -> Result<(), SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        let file_name = normalize_section_name(name);
        let path = indexed.path.join("sections").join(&file_name);
        if !path.exists() {
            return Err(SpecError::NotFound(format!("section: {}", name)));
        }
        fs::write(&path, content).map_err(StorageError::Io)?;
        Ok(())
    }

    pub fn delete_section(
        &self,
        id_or_slug: &str,
        name: &str,
    ) -> Result<(), SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        let file_name = normalize_section_name(name);
        let path = indexed.path.join("sections").join(&file_name);
        if path.exists() {
            fs::remove_file(&path).map_err(StorageError::Io)?;
        }
        Ok(())
    }

    pub fn list_sections(
        &self,
        id_or_slug: &str,
    ) -> Result<Vec<String>, SpecError> {
        let uuid = self.resolve_id(id_or_slug)?;
        let indexed = self
            .inner
            .get_indexed(&uuid)?
            .ok_or_else(|| SpecError::NotFound(uuid.to_string()))?;
        let sections_dir = indexed.path.join("sections");
        if !sections_dir.exists() {
            return Ok(Vec::new());
        }
        let mut names: Vec<String> = fs::read_dir(&sections_dir)
            .map_err(StorageError::Io)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.ends_with(".md"))
            .collect();
        names.sort();
        Ok(names)
    }
}
