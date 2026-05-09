use std::{
    collections::HashSet,
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use serde::{
    Deserialize,
    Serialize,
};
use thiserror::Error;

use crate::{
    error::RuleError,
    manifest::{
        RuleId,
        RuleManifest,
    },
    store::{
        RuleFilter,
        RuleStore,
    },
};

#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RenderTargetConfig {
    #[serde(default)]
    pub targets: Vec<RenderTarget>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
pub struct RenderTargetFilter {
    #[serde(default)]
    pub repo_scope: Option<String>,
    #[serde(default)]
    pub file_kind: Option<String>,
    #[serde(default)]
    pub path_scope: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExplainedRuleMatch {
    pub id: RuleId,
    pub slug: String,
    pub title: Option<String>,
    pub section: Option<String>,
    pub order_key: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExplainedTargetNode {
    pub name: String,
    pub title: Option<String>,
    pub effective_filter: RenderTargetFilter,
    pub matched_rules: Vec<ExplainedRuleMatch>,
    pub nodes: Vec<ExplainedTargetNode>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ExplainedTarget {
    pub name: String,
    pub output_path: String,
    pub root_filter: RenderTargetFilter,
    pub matched_rule_count: usize,
    pub nodes: Vec<ExplainedTargetNode>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RenderTargetNode {
    pub name: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub repo_scope: Option<String>,
    #[serde(default)]
    pub file_kind: Option<String>,
    #[serde(default)]
    pub path_scope: Option<String>,
    #[serde(default)]
    pub section: Option<String>,
    #[serde(default)]
    pub state: Option<String>,
    #[serde(default)]
    pub nodes: Vec<RenderTargetNode>,
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
    #[serde(default)]
    pub nodes: Vec<RenderTargetNode>,
    pub output_path: String,
}

impl RenderTargetFilter {
    pub fn merged_with(
        &self,
        child: &RenderTargetFilter,
    ) -> Self {
        Self {
            repo_scope: child
                .repo_scope
                .clone()
                .or_else(|| self.repo_scope.clone()),
            file_kind: child
                .file_kind
                .clone()
                .or_else(|| self.file_kind.clone()),
            path_scope: child
                .path_scope
                .clone()
                .or_else(|| self.path_scope.clone()),
            section: child.section.clone().or_else(|| self.section.clone()),
            state: child.state.clone().or_else(|| self.state.clone()),
        }
    }

    pub fn to_rule_filter(&self) -> RuleFilter {
        RuleFilter {
            state: self.state.clone(),
            file_kind: self.file_kind.clone(),
            section: self.section.clone(),
            repo_scope: self.repo_scope.clone(),
            path_scope: self.path_scope.clone(),
            slug: None,
            has_unresolved_feedback: None,
        }
    }
}

impl RenderTargetNode {
    pub fn local_filter(&self) -> RenderTargetFilter {
        RenderTargetFilter {
            repo_scope: self.repo_scope.clone(),
            file_kind: self.file_kind.clone(),
            path_scope: self.path_scope.clone(),
            section: self.section.clone(),
            state: self.state.clone(),
        }
    }

    pub fn effective_filter(
        &self,
        inherited: &RenderTargetFilter,
    ) -> RenderTargetFilter {
        inherited.merged_with(&self.local_filter())
    }
}

impl RenderTarget {
    pub fn flat_filter(&self) -> RenderTargetFilter {
        RenderTargetFilter {
            repo_scope: Some(self.repo_scope.clone()),
            file_kind: Some(self.file_kind.clone()),
            path_scope: self.path_scope.clone(),
            section: self.section.clone(),
            state: self.state.clone(),
        }
    }

    pub fn ordered_nodes(&self) -> Vec<RenderTargetNode> {
        if self.nodes.is_empty() {
            vec![RenderTargetNode {
                name: self.name.clone(),
                title: None,
                repo_scope: Some(self.repo_scope.clone()),
                file_kind: Some(self.file_kind.clone()),
                path_scope: self.path_scope.clone(),
                section: self.section.clone(),
                state: self.state.clone(),
                nodes: Vec::new(),
            }]
        } else {
            self.nodes.clone()
        }
    }
}

pub fn collect_target_rules(
    store: &RuleStore,
    target: &RenderTarget,
) -> Result<Vec<RuleManifest>, RuleError> {
    let inherited = target.flat_filter();
    let mut collected = Vec::new();
    let mut seen = HashSet::<RuleId>::new();

    for node in target.ordered_nodes() {
        collect_target_node_rules(
            store,
            target,
            &node,
            &inherited,
            &mut seen,
            &mut collected,
        )?;
    }

    Ok(collected)
}

pub fn explain_target(
    store: &RuleStore,
    target: &RenderTarget,
) -> Result<ExplainedTarget, RuleError> {
    let root_filter = target.flat_filter();
    let mut matched_rule_count = 0usize;
    let mut seen = HashSet::<RuleId>::new();
    let mut nodes = Vec::new();

    for node in target.ordered_nodes() {
        nodes.push(explain_target_node(
            store,
            target,
            &node,
            &root_filter,
            &mut seen,
            &mut matched_rule_count,
        )?);
    }

    Ok(ExplainedTarget {
        name: target.name.clone(),
        output_path: target.output_path.clone(),
        root_filter,
        matched_rule_count,
        nodes,
    })
}

fn collect_target_node_rules(
    store: &RuleStore,
    target: &RenderTarget,
    node: &RenderTargetNode,
    inherited: &RenderTargetFilter,
    seen: &mut HashSet<RuleId>,
    collected: &mut Vec<RuleManifest>,
) -> Result<(), RuleError> {
    let effective = node.effective_filter(inherited);
    let rules = store.list(&effective.to_rule_filter(), None)?;

    for rule in rules {
        if !seen.insert(rule.id) {
            return Err(RuleError::DuplicateRenderRule {
                target: target.name.clone(),
                node: node.name.clone(),
                slug: rule
                    .slug()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| rule.id.to_string()),
            });
        }
        collected.push(rule);
    }

    for child in &node.nodes {
        collect_target_node_rules(
            store, target, child, &effective, seen, collected,
        )?;
    }

    Ok(())
}

fn explain_target_node(
    store: &RuleStore,
    target: &RenderTarget,
    node: &RenderTargetNode,
    inherited: &RenderTargetFilter,
    seen: &mut HashSet<RuleId>,
    matched_rule_count: &mut usize,
) -> Result<ExplainedTargetNode, RuleError> {
    let effective = node.effective_filter(inherited);
    let rules = store.list(&effective.to_rule_filter(), None)?;
    let mut matched_rules = Vec::new();

    for rule in rules {
        if !seen.insert(rule.id) {
            return Err(RuleError::DuplicateRenderRule {
                target: target.name.clone(),
                node: node.name.clone(),
                slug: rule
                    .slug()
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| rule.id.to_string()),
            });
        }
        *matched_rule_count += 1;
        matched_rules.push(rule_match_summary(&rule));
    }

    let mut nodes = Vec::new();
    for child in &node.nodes {
        nodes.push(explain_target_node(
            store,
            target,
            child,
            &effective,
            seen,
            matched_rule_count,
        )?);
    }

    Ok(ExplainedTargetNode {
        name: node.name.clone(),
        title: node.title.clone(),
        effective_filter: effective,
        matched_rules,
        nodes,
    })
}

fn rule_match_summary(rule: &RuleManifest) -> ExplainedRuleMatch {
    ExplainedRuleMatch {
        id: rule.id,
        slug: rule
            .slug()
            .map(ToOwned::to_owned)
            .unwrap_or_else(|| rule.id.to_string()),
        title: rule.title().map(ToOwned::to_owned),
        section: rule.section().map(ToOwned::to_owned),
        order_key: rule.order_key(),
    }
}

#[derive(Debug, Error)]
pub enum TargetConfigError {
    #[error("read render target config {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("parse render target config {path} as TOML: {source}")]
    ParseToml {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("parse render target config {path} as YAML: {source}")]
    ParseYaml {
        path: PathBuf,
        source: serde_yaml::Error,
    },
    #[error("render target not found: {0}")]
    NotFound(String),
    #[error("duplicate render target name: {0}")]
    DuplicateName(String),
}

pub fn load_render_target_config(
    path: &Path
) -> Result<RenderTargetConfig, TargetConfigError> {
    let content =
        fs::read_to_string(path).map_err(|source| TargetConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let config: RenderTargetConfig =
        match path.extension().and_then(|extension| extension.to_str()) {
            Some("yaml" | "yml") =>
                serde_yaml::from_str(&content).map_err(|source| {
                    TargetConfigError::ParseYaml {
                        path: path.to_path_buf(),
                        source,
                    }
                })?,
            _ => toml::from_str(&content).map_err(|source| {
                TargetConfigError::ParseToml {
                    path: path.to_path_buf(),
                    source,
                }
            })?,
        };

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

pub fn resolve_render_target_output(
    config_path: &Path,
    target: &RenderTarget,
) -> PathBuf {
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
