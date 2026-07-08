use super::*;

pub const GENERATED_SPEC_FILE_COMMENT: &str =
    "<!-- spec-api:file generated=true -->";

pub const GENERATED_BODY_FILE_COMMENT: &str = GENERATED_SPEC_FILE_COMMENT;

pub(super) const GENERATED_SPEC_ENTRY_PREFIX: &str = "spec-api:entry";

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
    pub(super) fn normalized(&self) -> Result<Self, SpecError> {
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
    pub(super) fn is_empty(&self) -> bool {
        self.body.is_none() && self.sections.is_empty()
    }
}

pub(super) fn normalize_generated_section_name(name: &str) -> Result<String, SpecError> {
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

pub(super) fn invalid_generated_artifact_path(
    artifact_path: &Path,
    reason: &str,
) -> SpecError {
    SpecError::InvalidGeneratedArtifact(format!(
        "invalid generated artifact path '{}': {}",
        artifact_path.display(),
        reason
    ))
}

pub(super) fn parse_generated_artifact_location(
    artifact_path: &Path
) -> Result<GeneratedSpecArtifactLocation, SpecError> {
    let file_name = artifact_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            invalid_generated_artifact_path(artifact_path, "missing file name")
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

        if specs_dir.file_name().and_then(|name| name.to_str()) != Some("specs")
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
        invalid_generated_artifact_path(artifact_path, "missing spec directory")
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
        || specs_dir.file_name().and_then(|name| name.to_str()) != Some("specs")
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

pub(super) fn prepare_generated_document(
    snippets: &[GeneratedMarkdownSnippet<'_>],
    existing: Option<&str>,
) -> String {
    let rendered = render_generated_document(snippets);
    prepare_generated_output(&rendered, existing)
}

impl SpecStore {
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
                fs::remove_file(&path).map_err(|error| {
                    SpecError::Storage(StorageError::Io(error))
                })?;
            }
            return Ok(());
        }

        let content = toml::to_string_pretty(&normalized)
            .map_err(|error| SpecError::Serialization(error.to_string()))?;
        fs::write(&path, content)
            .map_err(|error| SpecError::Storage(StorageError::Io(error)))?;
        Ok(())
    }
}

