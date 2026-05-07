use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RenderTargetConfig {
    #[serde(default)]
    pub targets: Vec<RenderTarget>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RenderTarget {
    pub name: String,
    pub repo_scope: String,
    pub file_kind: String,
    #[serde(default)]
    pub path_scope: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    pub output_path: String,
}

#[derive(Debug, Error)]
pub enum TargetConfigError {
    #[error("read render target config {path}: {source}")]
    Io { path: PathBuf, source: std::io::Error },
    #[error("parse render target config {path}: {source}")]
    Parse { path: PathBuf, source: toml::de::Error },
    #[error("render target not found: {0}")]
    NotFound(String),
    #[error("duplicate render target name: {0}")]
    DuplicateName(String),
}

pub fn load_render_target_config(path: &Path) -> Result<RenderTargetConfig, TargetConfigError> {
    let content = fs::read_to_string(path).map_err(|source| TargetConfigError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let config: RenderTargetConfig = toml::from_str(&content).map_err(|source| {
        TargetConfigError::Parse {
            path: path.to_path_buf(),
            source,
        }
    })?;

    let mut names = HashSet::new();
    for target in &config.targets {
        if !names.insert(target.name.clone()) {
            return Err(TargetConfigError::DuplicateName(target.name.clone()));
        }
    }

    Ok(config)
}

pub fn render_target_by_name<'a>(
    config: &'a RenderTargetConfig,
    name: &str,
) -> Result<&'a RenderTarget, TargetConfigError> {
    config
        .targets
        .iter()
        .find(|target| target.name == name)
        .ok_or_else(|| TargetConfigError::NotFound(name.to_string()))
}

pub fn resolve_render_target_output(config_path: &Path, target: &RenderTarget) -> PathBuf {
    let output = PathBuf::from(&target.output_path);
    if output.is_absolute() {
        output
    } else {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_render_target_config_parses_targets_and_rejects_duplicates() {
        let config = toml::from_str::<RenderTargetConfig>(
            r#"
                [[targets]]
                name = "context-engine-agents"
                repo_scope = "context-engine"
                file_kind = "AGENTS"
                path_scope = "AGENTS.md"
                output_path = "AGENTS.md"
            "#,
        )
        .unwrap();

        assert_eq!(config.targets.len(), 1);
        assert_eq!(config.targets[0].name, "context-engine-agents");
        assert_eq!(config.targets[0].path_scope.as_deref(), Some("AGENTS.md"));

        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("rule-targets.toml");
        fs::write(
            &path,
            r#"
                [[targets]]
                name = "dup"
                repo_scope = "context-engine"
                file_kind = "AGENTS"
                path_scope = "AGENTS.md"
                output_path = "AGENTS.md"

                [[targets]]
                name = "dup"
                repo_scope = "memory-api"
                file_kind = "AGENTS"
                path_scope = "memory-api/AGENTS.md"
                output_path = "memory-api/AGENTS.md"
            "#,
        )
        .unwrap();

        let err = load_render_target_config(&path).unwrap_err();
        assert!(matches!(err, TargetConfigError::DuplicateName(name) if name == "dup"));
    }

    #[test]
    fn resolve_render_target_output_uses_config_parent_for_relative_paths() {
        let config_path = PathBuf::from("repo/rule-targets.toml");
        let target = RenderTarget {
            name: "context-engine-agents".to_string(),
            repo_scope: "context-engine".to_string(),
            file_kind: "AGENTS".to_string(),
            path_scope: Some("AGENTS.md".to_string()),
            section: None,
            state: None,
            output_path: ".github/generated/AGENTS.md".to_string(),
        };

        assert_eq!(
            resolve_render_target_output(&config_path, &target),
            PathBuf::from("repo/.github/generated/AGENTS.md")
        );
    }
}