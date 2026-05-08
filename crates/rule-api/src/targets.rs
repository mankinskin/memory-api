use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::error::RuleError;
use crate::manifest::{RuleId, RuleManifest};
use crate::store::{RuleFilter, RuleStore};

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
    pub fn merged_with(&self, child: &RenderTargetFilter) -> Self {
        Self {
            repo_scope: child.repo_scope.clone().or_else(|| self.repo_scope.clone()),
            file_kind: child.file_kind.clone().or_else(|| self.file_kind.clone()),
            path_scope: child.path_scope.clone().or_else(|| self.path_scope.clone()),
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

    pub fn effective_filter(&self, inherited: &RenderTargetFilter) -> RenderTargetFilter {
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
        collect_target_node_rules(store, target, &node, &inherited, &mut seen, &mut collected)?;
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
        collect_target_node_rules(store, target, child, &effective, seen, collected)?;
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
    Io { path: PathBuf, source: std::io::Error },
    #[error("parse render target config {path} as TOML: {source}")]
    ParseToml { path: PathBuf, source: toml::de::Error },
    #[error("parse render target config {path} as YAML: {source}")]
    ParseYaml { path: PathBuf, source: serde_yaml::Error },
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
    let config: RenderTargetConfig = match path.extension().and_then(|extension| extension.to_str()) {
        Some("yaml" | "yml") => serde_yaml::from_str(&content).map_err(|source| {
            TargetConfigError::ParseYaml {
                path: path.to_path_buf(),
                source,
            }
        })?,
        _ => toml::from_str(&content).map_err(|source| TargetConfigError::ParseToml {
            path: path.to_path_buf(),
            source,
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
    use tempfile::tempdir;

    use super::*;
    use crate::manifest::RuleManifest;
    use crate::store::RuleStore;

    #[test]
    fn load_render_target_config_parses_targets_and_rejects_duplicates() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("rule-targets.yaml");
        fs::write(
            &config_path,
            concat!(
                "targets:\n",
                "  - name: context-engine-agents\n",
                "    repo_scope: context-engine\n",
                "    file_kind: AGENTS\n",
                "    path_scope: AGENTS.md\n",
                "    output_path: AGENTS.md\n",
            ),
        )
        .unwrap();

        let config = load_render_target_config(&config_path).unwrap();

        assert_eq!(config.targets.len(), 1);
        assert_eq!(config.targets[0].name, "context-engine-agents");
        assert_eq!(config.targets[0].path_scope.as_deref(), Some("AGENTS.md"));
        assert_eq!(config.targets[0].ordered_nodes().len(), 1);

        let path = tmp.path().join("duplicate-rule-targets.yaml");
        fs::write(
            &path,
            concat!(
                "targets:\n",
                "  - name: dup\n",
                "    repo_scope: context-engine\n",
                "    file_kind: AGENTS\n",
                "    path_scope: AGENTS.md\n",
                "    output_path: AGENTS.md\n",
                "  - name: dup\n",
                "    repo_scope: memory-api\n",
                "    file_kind: AGENTS\n",
                "    path_scope: memory-api/AGENTS.md\n",
                "    output_path: memory-api/AGENTS.md\n",
            ),
        )
        .unwrap();

        let err = load_render_target_config(&path).unwrap_err();
        assert!(matches!(err, TargetConfigError::DuplicateName(name) if name == "dup"));
    }

        #[test]
    fn load_render_target_config_supports_legacy_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("rule-targets.toml");
                fs::write(
                        &path,
            r#"
                [[targets]]
                name = "context-engine-agents"
                repo_scope = "context-engine"
                file_kind = "AGENTS"
                path_scope = "AGENTS.md"
                output_path = "AGENTS.md"

                [[targets.nodes]]
                name = "agent-rules"
                title = "Agent Rules"
                section = "agent-rules"

                [[targets.nodes.nodes]]
                name = "operating-principles"
                title = "Operating Principles"
                section = "agent-rules/operating-principles"

                [[targets.nodes.nodes]]
                name = "task-routing"
                title = "Task Routing"
                section = "agent-rules/task-routing"
            "#,
                )
                .unwrap();

                let config = load_render_target_config(&path).unwrap();
                let target = &config.targets[0];

                assert_eq!(target.name, "context-engine-agents");
                assert_eq!(target.nodes.len(), 1);
                assert_eq!(target.nodes[0].name, "agent-rules");
                assert_eq!(target.nodes[0].nodes.len(), 2);
                assert_eq!(target.nodes[0].nodes[0].name, "operating-principles");
                assert_eq!(target.nodes[0].nodes[1].name, "task-routing");
        }

    #[test]
    fn load_render_target_config_parses_hierarchical_outline_nodes_in_order() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("hierarchical-rule-targets.yaml");
        fs::write(
            &path,
            concat!(
                "targets:\n",
                "  - name: context-engine-agents\n",
                "    repo_scope: context-engine\n",
                "    file_kind: AGENTS\n",
                "    output_path: AGENTS.md\n",
                "    nodes:\n",
                "      - name: opening\n",
                "        title: Opening\n",
                "        section: opening\n",
                "        nodes:\n",
                "          - name: validation\n",
                "            title: Validation\n",
                "            section: opening/validation\n",
                "      - name: quality-gates\n",
                "        title: Quality Gates\n",
                "        section: quality-gates\n",
            ),
        )
        .unwrap();

        let config = load_render_target_config(&path).unwrap();

        let target = &config.targets[0];
        let nodes = target.ordered_nodes();

        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].name, "opening");
        assert_eq!(nodes[1].name, "quality-gates");
        assert_eq!(nodes[0].nodes.len(), 1);
        assert_eq!(nodes[0].nodes[0].name, "validation");

        let inherited = target.flat_filter();
        assert_eq!(nodes[0].effective_filter(&inherited).repo_scope.as_deref(), Some("context-engine"));
        assert_eq!(nodes[0].effective_filter(&inherited).file_kind.as_deref(), Some("AGENTS"));
        assert_eq!(nodes[0].effective_filter(&inherited).section.as_deref(), Some("opening"));
        assert_eq!(nodes[0].nodes[0].effective_filter(&nodes[0].effective_filter(&inherited)).section.as_deref(), Some("opening/validation"));
    }

    #[test]
    fn resolve_render_target_output_uses_config_parent_for_relative_paths() {
        let config_path = PathBuf::from("repo/rule-targets.yaml");
        let target = RenderTarget {
            name: "context-engine-agents".to_string(),
            repo_scope: "context-engine".to_string(),
            file_kind: "AGENTS".to_string(),
            path_scope: Some("AGENTS.md".to_string()),
            section: None,
            state: None,
            nodes: Vec::new(),
            output_path: ".github/generated/AGENTS.md".to_string(),
        };

        assert_eq!(
            resolve_render_target_output(&config_path, &target),
            PathBuf::from("repo/.github/generated/AGENTS.md")
        );
    }

    #[test]
    fn collect_target_rules_traverses_nodes_in_outline_order() {
        let dir = tempdir().unwrap();
        let mut store = RuleStore::open(dir.path()).unwrap();

        let mut opening = RuleManifest::new(
            "shared/agents/opening",
            "Opening",
            "AGENTS",
            "opening",
            "Start with the concrete anchor.",
        );
        opening.set_repo_scopes(["context-engine"]);
        opening.set_path_scopes(["AGENTS.md"]);
        opening.set_order_key(20);

        let mut validation = RuleManifest::new(
            "shared/agents/validation",
            "Validation",
            "AGENTS",
            "opening/validation",
            "Run the focused check next.",
        );
        validation.set_repo_scopes(["context-engine"]);
        validation.set_path_scopes(["AGENTS.md"]);
        validation.set_order_key(10);

        let mut quality_gates = RuleManifest::new(
            "shared/agents/quality-gates",
            "Quality Gates",
            "AGENTS",
            "quality-gates",
            "Run relevant tests before completion.",
        );
        quality_gates.set_repo_scopes(["context-engine"]);
        quality_gates.set_path_scopes(["AGENTS.md"]);
        quality_gates.set_order_key(5);

        store.create(&opening, None).unwrap();
        store.create(&validation, None).unwrap();
        store.create(&quality_gates, None).unwrap();

        let target = RenderTarget {
            name: "context-engine-agents".to_string(),
            repo_scope: "context-engine".to_string(),
            file_kind: "AGENTS".to_string(),
            path_scope: Some("AGENTS.md".to_string()),
            section: None,
            state: None,
            nodes: vec![
                RenderTargetNode {
                    name: "opening".to_string(),
                    title: Some("Opening".to_string()),
                    repo_scope: None,
                    file_kind: None,
                    path_scope: None,
                    section: Some("opening".to_string()),
                    state: None,
                    nodes: vec![RenderTargetNode {
                        name: "validation".to_string(),
                        title: Some("Validation".to_string()),
                        repo_scope: None,
                        file_kind: None,
                        path_scope: None,
                        section: Some("opening/validation".to_string()),
                        state: None,
                        nodes: Vec::new(),
                    }],
                },
                RenderTargetNode {
                    name: "quality-gates".to_string(),
                    title: Some("Quality Gates".to_string()),
                    repo_scope: None,
                    file_kind: None,
                    path_scope: None,
                    section: Some("quality-gates".to_string()),
                    state: None,
                    nodes: Vec::new(),
                },
            ],
            output_path: "AGENTS.md".to_string(),
        };

        let rules = collect_target_rules(&store, &target).unwrap();
        let slugs = rules
            .iter()
            .map(|rule| rule.slug().unwrap().to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            slugs,
            vec![
                "shared/agents/opening".to_string(),
                "shared/agents/validation".to_string(),
                "shared/agents/quality-gates".to_string(),
            ]
        );
    }

    #[test]
    fn collect_target_rules_rejects_duplicate_matches_across_nodes() {
        let dir = tempdir().unwrap();
        let mut store = RuleStore::open(dir.path()).unwrap();

        let mut opening = RuleManifest::new(
            "shared/agents/opening",
            "Opening",
            "AGENTS",
            "opening",
            "Start with the concrete anchor.",
        );
        opening.set_repo_scopes(["context-engine"]);
        opening.set_path_scopes(["AGENTS.md"]);
        store.create(&opening, None).unwrap();

        let target = RenderTarget {
            name: "context-engine-agents".to_string(),
            repo_scope: "context-engine".to_string(),
            file_kind: "AGENTS".to_string(),
            path_scope: Some("AGENTS.md".to_string()),
            section: None,
            state: None,
            nodes: vec![
                RenderTargetNode {
                    name: "first".to_string(),
                    title: None,
                    repo_scope: None,
                    file_kind: None,
                    path_scope: None,
                    section: Some("opening".to_string()),
                    state: None,
                    nodes: Vec::new(),
                },
                RenderTargetNode {
                    name: "second".to_string(),
                    title: None,
                    repo_scope: None,
                    file_kind: None,
                    path_scope: None,
                    section: Some("opening".to_string()),
                    state: None,
                    nodes: Vec::new(),
                },
            ],
            output_path: "AGENTS.md".to_string(),
        };

        let err = collect_target_rules(&store, &target).unwrap_err();
        assert!(matches!(
            err,
            RuleError::DuplicateRenderRule { target, node, slug }
            if target == "context-engine-agents" && node == "second" && slug == "shared/agents/opening"
        ));
    }

    #[test]
    fn explain_target_reports_node_matches_with_effective_filters() {
        let dir = tempdir().unwrap();
        let mut store = RuleStore::open(dir.path()).unwrap();

        let mut opening = RuleManifest::new(
            "shared/agents/opening",
            "Opening",
            "AGENTS",
            "agent-rules/operating-principles",
            "Gather context before coding.",
        );
        opening.set_repo_scopes(["context-engine"]);
        opening.set_path_scopes(["AGENTS.md"]);
        opening.set_order_key(10);
        store.create(&opening, None).unwrap();

        let target = RenderTarget {
            name: "context-engine-agents".to_string(),
            repo_scope: "context-engine".to_string(),
            file_kind: "AGENTS".to_string(),
            path_scope: Some("AGENTS.md".to_string()),
            section: None,
            state: None,
            nodes: vec![RenderTargetNode {
                name: "agent-rules".to_string(),
                title: Some("Agent Rules".to_string()),
                repo_scope: None,
                file_kind: None,
                path_scope: None,
                section: Some("agent-rules".to_string()),
                state: None,
                nodes: vec![RenderTargetNode {
                    name: "operating-principles".to_string(),
                    title: Some("Operating Principles".to_string()),
                    repo_scope: None,
                    file_kind: None,
                    path_scope: None,
                    section: Some("agent-rules/operating-principles".to_string()),
                    state: None,
                    nodes: Vec::new(),
                }],
            }],
            output_path: "AGENTS.md".to_string(),
        };

        let explained = explain_target(&store, &target).unwrap();

        assert_eq!(explained.name, "context-engine-agents");
        assert_eq!(explained.matched_rule_count, 1);
        assert_eq!(explained.nodes.len(), 1);
        assert_eq!(explained.nodes[0].nodes.len(), 1);
        assert_eq!(
            explained.nodes[0].nodes[0].effective_filter.section.as_deref(),
            Some("agent-rules/operating-principles")
        );
        assert_eq!(explained.nodes[0].nodes[0].matched_rules.len(), 1);
        assert_eq!(
            explained.nodes[0].nodes[0].matched_rules[0].slug,
            "shared/agents/opening"
        );
    }
}