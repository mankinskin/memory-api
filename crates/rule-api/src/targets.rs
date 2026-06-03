use std::{
    collections::{
        HashMap,
        HashSet,
    },
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

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct RawRenderTargetConfig {
    #[serde(default)]
    imports: Vec<PathBuf>,
    #[serde(default)]
    schemas: Vec<RenderTargetSchema>,
    #[serde(default)]
    targets: Vec<RawRenderTarget>,
    #[serde(default)]
    folders: Vec<RenderTargetFolder>,
    #[serde(default)]
    files: Vec<RenderTargetFile>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct RawRenderTarget {
    name: String,
    repo_scope: String,
    file_kind: String,
    #[serde(default)]
    path_scope: Option<String>,
    #[serde(default)]
    section: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    nodes: Vec<RenderTargetNode>,
    output_path: String,
    #[serde(default)]
    schema: Option<String>,
    #[serde(default)]
    target_kind: Option<RenderTargetKind>,
    #[serde(default)]
    node_mode: Option<RenderTargetNodeMode>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct RenderTargetSchema {
    name: String,
    #[serde(default)]
    nodes: Vec<RenderTargetNode>,
    #[serde(default)]
    required_blocks: RenderTargetRequiredBlocks,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
struct RenderTargetRequiredBlocks {
    #[serde(default)]
    root: Vec<String>,
    #[serde(default)]
    child: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum RenderTargetKind {
    Root,
    Child,
}

impl RenderTargetKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Child => "child",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
enum RenderTargetNodeMode {
    Replace,
    Append,
}

#[derive(Debug, Default)]
struct LoadedRenderTargets {
    targets: Vec<RenderTarget>,
    schemas: HashMap<String, RenderTargetSchema>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct RenderTargetFolder {
    name: String,
    #[serde(default)]
    folders: Vec<RenderTargetFolder>,
    #[serde(default)]
    files: Vec<RenderTargetFile>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct RenderTargetFile {
    name: String,
    target: RenderTargetDefinition,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
struct RenderTargetDefinition {
    name: String,
    repo_scope: String,
    file_kind: String,
    #[serde(default)]
    path_scope: Option<String>,
    #[serde(default)]
    section: Option<String>,
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    nodes: Vec<RenderTargetNode>,
    #[serde(default)]
    schema: Option<String>,
    #[serde(default)]
    target_kind: Option<RenderTargetKind>,
    #[serde(default)]
    node_mode: Option<RenderTargetNodeMode>,
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
    #[serde(skip, default)]
    pub source_config_path: Option<PathBuf>,
    #[serde(skip, default)]
    pub source_output_root: Option<PathBuf>,
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
            has_low_feedback: None,
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
    pub fn config_path<'a>(
        &'a self,
        fallback: &'a Path,
    ) -> &'a Path {
        self.source_config_path.as_deref().unwrap_or(fallback)
    }

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

impl RawRenderTargetConfig {
    fn into_render_targets(
        self,
        config_path: &Path,
        schemas: &HashMap<String, RenderTargetSchema>,
    ) -> Result<Vec<RenderTarget>, TargetConfigError> {
        let mut targets = self
            .targets
            .into_iter()
            .map(|target| target.into_render_target(config_path, schemas))
            .collect::<Result<Vec<_>, _>>()?;
        let root = PathBuf::new();

        push_tree_files(&root, self.files, config_path, schemas, &mut targets)?;
        for folder in self.folders {
            folder.collect_targets(&root, config_path, schemas, &mut targets)?;
        }

        Ok(targets)
    }
}

impl LoadedRenderTargets {
    fn insert_schema(
        &mut self,
        schema: RenderTargetSchema,
    ) -> Result<(), TargetConfigError> {
        let name = schema.name.clone();
        if self.schemas.insert(name.clone(), schema).is_some() {
            return Err(TargetConfigError::DuplicateSchemaName(name));
        }
        Ok(())
    }

    fn merge(
        &mut self,
        other: Self,
    ) -> Result<(), TargetConfigError> {
        for schema in other.schemas.into_values() {
            self.insert_schema(schema)?;
        }
        self.targets.extend(other.targets);
        Ok(())
    }
}

impl RawRenderTarget {
    fn into_render_target(
        self,
        config_path: &Path,
        schemas: &HashMap<String, RenderTargetSchema>,
    ) -> Result<RenderTarget, TargetConfigError> {
        Ok(RenderTarget {
            name: self.name.clone(),
            repo_scope: self.repo_scope,
            file_kind: self.file_kind,
            path_scope: self.path_scope,
            section: self.section,
            state: self.state,
            nodes: resolve_target_nodes(
                &self.name,
                self.nodes,
                self.schema.as_deref(),
                self.target_kind,
                self.node_mode,
                schemas,
            )?,
            output_path: self.output_path,
            source_config_path: Some(config_path.to_path_buf()),
            source_output_root: Some(resolve_config_output_root(config_path)),
        })
    }
}

impl RenderTargetFolder {
    fn collect_targets(
        self,
        parent: &Path,
        config_path: &Path,
        schemas: &HashMap<String, RenderTargetSchema>,
        targets: &mut Vec<RenderTarget>,
    ) -> Result<(), TargetConfigError> {
        let prefix = parent.join(self.name);

        push_tree_files(&prefix, self.files, config_path, schemas, targets)?;
        for folder in self.folders {
            folder.collect_targets(&prefix, config_path, schemas, targets)?;
        }

        Ok(())
    }
}

impl RenderTargetFile {
    fn into_render_target(
        self,
        parent: &Path,
        config_path: &Path,
        schemas: &HashMap<String, RenderTargetSchema>,
    ) -> Result<RenderTarget, TargetConfigError> {
        self.target.into_render_target(
            tree_output_path(parent, &self.name),
            config_path,
            schemas,
        )
    }
}

impl RenderTargetDefinition {
    fn into_render_target(
        self,
        output_path: String,
        config_path: &Path,
        schemas: &HashMap<String, RenderTargetSchema>,
    ) -> Result<RenderTarget, TargetConfigError> {
        Ok(RenderTarget {
            name: self.name.clone(),
            repo_scope: self.repo_scope,
            file_kind: self.file_kind,
            path_scope: self.path_scope,
            section: self.section,
            state: self.state,
            nodes: resolve_target_nodes(
                &self.name,
                self.nodes,
                self.schema.as_deref(),
                self.target_kind,
                self.node_mode,
                schemas,
            )?,
            output_path,
            source_config_path: Some(config_path.to_path_buf()),
            source_output_root: Some(resolve_config_output_root(config_path)),
        })
    }
}

fn push_tree_files(
    parent: &Path,
    files: Vec<RenderTargetFile>,
    config_path: &Path,
    schemas: &HashMap<String, RenderTargetSchema>,
    targets: &mut Vec<RenderTarget>,
) -> Result<(), TargetConfigError> {
    for file in files {
        targets.push(file.into_render_target(parent, config_path, schemas)?);
    }

    Ok(())
}

fn resolve_target_nodes(
    target_name: &str,
    nodes: Vec<RenderTargetNode>,
    schema_name: Option<&str>,
    target_kind: Option<RenderTargetKind>,
    node_mode: Option<RenderTargetNodeMode>,
    schemas: &HashMap<String, RenderTargetSchema>,
) -> Result<Vec<RenderTargetNode>, TargetConfigError> {
    let Some(schema_name) = schema_name else {
        return Ok(nodes);
    };

    let schema = schemas.get(schema_name).ok_or_else(|| {
        TargetConfigError::UnknownSchema {
            target: target_name.to_string(),
            schema: schema_name.to_string(),
        }
    })?;

    let resolved = if nodes.is_empty() {
        schema.nodes.clone()
    } else if matches!(node_mode, Some(RenderTargetNodeMode::Append)) {
        let mut merged = schema.nodes.clone();
        merged.extend(nodes);
        merged
    } else {
        nodes
    };

    validate_required_blocks(target_name, schema, target_kind, &resolved)?;

    Ok(resolved)
}

fn validate_required_blocks(
    target_name: &str,
    schema: &RenderTargetSchema,
    target_kind: Option<RenderTargetKind>,
    nodes: &[RenderTargetNode],
) -> Result<(), TargetConfigError> {
    let Some(target_kind) = target_kind else {
        return Ok(());
    };

    let required = match target_kind {
        RenderTargetKind::Root => &schema.required_blocks.root,
        RenderTargetKind::Child => &schema.required_blocks.child,
    };
    let present = nodes
        .iter()
        .map(|node| node.name.as_str())
        .collect::<HashSet<_>>();

    for required_block in required {
        if !present.contains(required_block.as_str()) {
            return Err(TargetConfigError::MissingRequiredBlock {
                target: target_name.to_string(),
                schema: schema.name.clone(),
                target_kind: target_kind.as_str().to_string(),
                block: required_block.clone(),
            });
        }
    }

    Ok(())
}

fn tree_output_path(
    parent: &Path,
    name: &str,
) -> String {
    let mut path = parent.to_path_buf();
    path.push(name);
    path.to_string_lossy().replace('\\', "/")
}

fn parse_render_target_config(
    path: &Path,
) -> Result<RawRenderTargetConfig, TargetConfigError> {
    let content =
        fs::read_to_string(path).map_err(|source| TargetConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;

    match path.extension().and_then(|extension| extension.to_str()) {
        Some("yaml" | "yml") =>
            serde_yaml::from_str::<RawRenderTargetConfig>(&content)
                .map_err(|source| TargetConfigError::ParseYaml {
                    path: path.to_path_buf(),
                    source,
                }),
        _ => toml::from_str::<RawRenderTargetConfig>(&content)
            .map_err(|source| TargetConfigError::ParseToml {
                path: path.to_path_buf(),
                source,
            }),
    }
}

fn is_supported_render_target_config(
    path: &Path,
) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("yaml" | "yml" | "toml")
    )
}

fn resolve_config_output_root(
    config_path: &Path,
) -> PathBuf {
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));

    for ancestor in config_dir.ancestors() {
        if ancestor
            .file_name()
            .and_then(|name| name.to_str())
            == Some("rule-targets")
        {
            return ancestor
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf();
        }
    }

    config_dir.to_path_buf()
}

fn resolve_import_path(
    config_path: &Path,
    import: &Path,
) -> PathBuf {
    if import.is_absolute() {
        import.to_path_buf()
    } else {
        config_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(import)
    }
}

fn load_import_targets(
    import_path: &Path,
    loading: &mut Vec<PathBuf>,
) -> Result<LoadedRenderTargets, TargetConfigError> {
    let metadata = fs::metadata(import_path).map_err(|source| {
        TargetConfigError::Io {
            path: import_path.to_path_buf(),
            source,
        }
    })?;

    if !metadata.is_dir() {
        return load_render_target_config_inner(import_path, loading);
    }

    let mut fragment_paths = Vec::new();
    for entry in fs::read_dir(import_path).map_err(|source| {
        TargetConfigError::Io {
            path: import_path.to_path_buf(),
            source,
        }
    })? {
        let entry = entry.map_err(|source| TargetConfigError::Io {
            path: import_path.to_path_buf(),
            source,
        })?;
        let fragment_path = entry.path();
        if fragment_path.is_file()
            && is_supported_render_target_config(&fragment_path)
        {
            fragment_paths.push(fragment_path);
        }
    }

    fragment_paths.sort();

    let mut loaded = LoadedRenderTargets::default();
    for fragment_path in fragment_paths {
        loaded.merge(load_render_target_config_inner(&fragment_path, loading)?)?;
    }

    Ok(loaded)
}

fn load_render_target_config_inner(
    path: &Path,
    loading: &mut Vec<PathBuf>,
) -> Result<LoadedRenderTargets, TargetConfigError> {
    if path.is_dir() {
        return Err(directory_target_config_error(path));
    }

    let canonical =
        fs::canonicalize(path).map_err(|source| TargetConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    if loading.contains(&canonical) {
        return Err(TargetConfigError::ImportCycle {
            path: path.to_path_buf(),
        });
    }

    loading.push(canonical);
    let result = (|| {
        let raw = parse_render_target_config(path)?;
        let mut loaded = LoadedRenderTargets::default();

        for import in raw.imports.clone() {
            let import_path = resolve_import_path(path, &import);
            loaded.merge(load_import_targets(&import_path, loading)?)?;
        }

        for schema in raw.schemas.iter().cloned() {
            loaded.insert_schema(schema)?;
        }

        loaded
            .targets
            .extend(raw.into_render_targets(path, &loaded.schemas)?);
        Ok(loaded)
    })();
    loading.pop();
    result
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
    #[error(
        "render target config must be a file, not a directory: {path}. Did you mean {suggested}?"
    )]
    DirectoryPathWithSuggestion {
        path: PathBuf,
        suggested: PathBuf,
    },
    #[error("render target config must be a file, not a directory: {path}")]
    DirectoryPath { path: PathBuf },
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
    #[error("render target selector {selector} matches multiple targets: {matches}")]
    AmbiguousSelector {
        selector: String,
        matches: String,
    },
    #[error("duplicate render target name: {0}")]
    DuplicateName(String),
    #[error("duplicate render target schema name: {0}")]
    DuplicateSchemaName(String),
    #[error("render target config import cycle detected at {path}")]
    ImportCycle { path: PathBuf },
    #[error("render target {target} references unknown schema {schema}")]
    UnknownSchema {
        target: String,
        schema: String,
    },
    #[error(
        "render target {target} missing required {target_kind} README block {block} from schema {schema}"
    )]
    MissingRequiredBlock {
        target: String,
        schema: String,
        target_kind: String,
        block: String,
    },
}

fn directory_target_config_error(
    path: &Path,
) -> TargetConfigError {
    if let Some(suggested) = suggested_render_target_config_path(path) {
        TargetConfigError::DirectoryPathWithSuggestion {
            path: path.to_path_buf(),
            suggested,
        }
    } else {
        TargetConfigError::DirectoryPath {
            path: path.to_path_buf(),
        }
    }
}

fn suggested_render_target_config_path(
    path: &Path,
) -> Option<PathBuf> {
    let parent = path.parent()?;
    let stem = path.file_name()?.to_str()?;

    ["yaml", "yml", "toml"]
        .into_iter()
        .map(|extension| parent.join(format!("{stem}.{extension}")))
        .find(|candidate| candidate.is_file())
}

fn normalize_render_target_selector(
    selector: &str,
) -> String {
    selector.replace('\\', "/")
}

pub fn load_render_target_config(
    path: &Path
) -> Result<RenderTargetConfig, TargetConfigError> {
    let mut loading = Vec::new();
    let loaded = if path.is_dir() {
        load_import_targets(path, &mut loading)?
    } else {
        load_render_target_config_inner(path, &mut loading)?
    };
    let config = RenderTargetConfig {
        targets: loaded.targets,
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

pub fn render_target_by_selector<'a>(
    config: &'a RenderTargetConfig,
    config_path: &Path,
    selector: &str,
) -> Result<&'a RenderTarget, TargetConfigError> {
    if let Ok(target) = render_target_by_name(config, selector) {
        return Ok(target);
    }

    let selector = normalize_render_target_selector(selector);
    let matches = config
        .targets
        .iter()
        .filter(|target| {
            normalize_render_target_selector(&target.output_path) == selector
                ||
            normalize_render_target_selector(
                resolve_render_target_output(config_path, target)
                    .to_string_lossy()
                    .as_ref(),
            ) == selector
        })
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [target] => Ok(*target),
        [] => Err(TargetConfigError::NotFound(selector)),
        _ => Err(TargetConfigError::AmbiguousSelector {
            selector,
            matches: matches
                .iter()
                .map(|target| target.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
        }),
    }
}

pub fn resolve_render_target_output(
    config_path: &Path,
    target: &RenderTarget,
) -> PathBuf {
    let output = PathBuf::from(&target.output_path);
    if output.is_absolute() {
        output
    } else {
        target
            .source_output_root
            .as_deref()
            .unwrap_or_else(|| {
                target
                    .config_path(config_path)
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
            })
            .join(output)
    }
}
