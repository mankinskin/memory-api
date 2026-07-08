use super::*;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct RenderTargetConfig {
    #[serde(default)]
    pub targets: Vec<RenderTarget>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub(super) struct RawTargetDefaults {
    #[serde(default)]
    pub(super) repo_scope: Option<String>,
    #[serde(default)]
    pub(super) file_kind: Option<String>,
    #[serde(default)]
    pub(super) path_scope: Option<String>,
    #[serde(default)]
    pub(super) section: Option<String>,
    #[serde(default)]
    pub(super) state: Option<String>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct RawRenderTargetConfig {
    #[serde(default)]
    pub(super) imports: Vec<PathBuf>,
    #[serde(default)]
    pub(super) defaults: RawTargetDefaults,
    #[serde(default)]
    pub(super) schemas: Vec<RenderTargetSchema>,
    #[serde(default)]
    pub(super) targets: Vec<RawRenderTarget>,
    #[serde(default)]
    pub(super) folders: Vec<RenderTargetFolder>,
    #[serde(default)]
    pub(super) files: Vec<RenderTargetFile>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct RawRenderTarget {
    pub(super) name: String,
    #[serde(default)]
    pub(super) scope: Option<String>,
    #[serde(default)]
    pub(super) repo_scope: Option<String>,
    #[serde(default)]
    pub(super) file_kind: Option<String>,
    #[serde(default)]
    pub(super) path_scope: Option<String>,
    #[serde(default)]
    pub(super) section: Option<String>,
    #[serde(default)]
    pub(super) state: Option<String>,
    #[serde(default)]
    pub(super) nodes: Vec<RenderTargetNode>,
    #[serde(default)]
    pub(super) output_path: Option<String>,
    #[serde(default)]
    pub(super) schema: Option<String>,
    #[serde(default)]
    pub(super) target_kind: Option<RenderTargetKind>,
    #[serde(default)]
    pub(super) node_mode: Option<RenderTargetNodeMode>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct RenderTargetSchema {
    pub(super) name: String,
    #[serde(default)]
    pub(super) nodes: Vec<RenderTargetNode>,
    #[serde(default)]
    pub(super) required_blocks: RenderTargetRequiredBlocks,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq, Eq)]
pub(super) struct RenderTargetRequiredBlocks {
    #[serde(default)]
    pub(super) root: Vec<String>,
    #[serde(default)]
    pub(super) child: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum RenderTargetKind {
    Root,
    Child,
}

impl RenderTargetKind {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::Root => "root",
            Self::Child => "child",
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum RenderTargetNodeMode {
    Replace,
    Append,
}

/// Accumulator returned by each loader call - targets only; schemas are shared
/// globally through `LoadCtx` so every fragment can see schemas registered by
/// any prior import.
#[derive(Debug, Default)]
pub(super) struct LoadedRenderTargets {
    pub(super) targets: Vec<RenderTarget>,
}

/// Shared mutable context threaded through the entire config-load tree.
/// Schemas are registered here on first encounter and are visible to all
/// subsequent fragments regardless of import order.
#[derive(Debug, Default)]
pub(super) struct LoadCtx {
    /// Canonicalized paths of files currently being loaded (cycle detection).
    pub(super) visiting: Vec<PathBuf>,
    /// Canonicalized paths of files that have been fully loaded (dedup guard).
    pub(super) visited: HashSet<PathBuf>,
    /// All schemas registered so far across every loaded fragment.
    pub(super) schemas: HashMap<String, RenderTargetSchema>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct RenderTargetFolder {
    pub(super) name: String,
    #[serde(default)]
    pub(super) folders: Vec<RenderTargetFolder>,
    #[serde(default)]
    pub(super) files: Vec<RenderTargetFile>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct RenderTargetFile {
    pub(super) name: String,
    pub(super) target: RenderTargetDefinition,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub(super) struct RenderTargetDefinition {
    pub(super) name: String,
    #[serde(default)]
    pub(super) scope: Option<String>,
    #[serde(default)]
    pub(super) repo_scope: Option<String>,
    #[serde(default)]
    pub(super) file_kind: Option<String>,
    #[serde(default)]
    pub(super) path_scope: Option<String>,
    #[serde(default)]
    pub(super) section: Option<String>,
    #[serde(default)]
    pub(super) state: Option<String>,
    #[serde(default)]
    pub(super) nodes: Vec<RenderTargetNode>,
    #[serde(default)]
    pub(super) schema: Option<String>,
    #[serde(default)]
    pub(super) target_kind: Option<RenderTargetKind>,
    #[serde(default)]
    pub(super) node_mode: Option<RenderTargetNodeMode>,
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
    pub(super) fn into_render_targets(
        self,
        config_path: &Path,
        schemas: &HashMap<String, RenderTargetSchema>,
    ) -> Result<Vec<RenderTarget>, TargetConfigError> {
        let defaults = &self.defaults;
        let mut targets = self
            .targets
            .into_iter()
            .map(|target| {
                target.into_render_target(config_path, schemas, defaults)
            })
            .collect::<Result<Vec<_>, _>>()?;
        let root = PathBuf::new();

        push_tree_files(
            &root,
            self.files,
            config_path,
            schemas,
            defaults,
            &mut targets,
        )?;
        for folder in self.folders {
            folder.collect_targets(
                &root,
                config_path,
                schemas,
                defaults,
                &mut targets,
            )?;
        }

        Ok(targets)
    }
}

impl LoadedRenderTargets {
    pub(super) fn merge(
        &mut self,
        other: Self,
    ) {
        self.targets.extend(other.targets);
    }
}

impl LoadCtx {
    pub(super) fn insert_schema(
        &mut self,
        schema: RenderTargetSchema,
    ) -> Result<(), TargetConfigError> {
        let name = schema.name.clone();
        if let Some(existing) = self.schemas.get(&name) {
            if existing == &schema {
                return Ok(());
            }
            return Err(TargetConfigError::DuplicateSchemaName(name));
        }
        self.schemas.insert(name, schema);
        Ok(())
    }
}

impl RawRenderTarget {
    pub(super) fn into_render_target(
        self,
        config_path: &Path,
        schemas: &HashMap<String, RenderTargetSchema>,
        defaults: &RawTargetDefaults,
    ) -> Result<RenderTarget, TargetConfigError> {
        let (repo_scope, file_kind, path_scope) = resolve_scope_fields(
            &self.name,
            self.repo_scope,
            self.file_kind,
            self.path_scope,
            self.scope,
            defaults,
        )?;
        let section = self.section.or_else(|| defaults.section.clone());
        let state = self.state.or_else(|| defaults.state.clone());
        let output_path = self
            .output_path
            .or_else(|| path_scope.clone())
            .ok_or_else(|| TargetConfigError::MissingOutputPath {
                target: self.name.clone(),
            })?;
        Ok(RenderTarget {
            name: self.name.clone(),
            repo_scope,
            file_kind,
            path_scope,
            section,
            state,
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

impl RenderTargetFolder {
    pub(super) fn collect_targets(
        self,
        parent: &Path,
        config_path: &Path,
        schemas: &HashMap<String, RenderTargetSchema>,
        defaults: &RawTargetDefaults,
        targets: &mut Vec<RenderTarget>,
    ) -> Result<(), TargetConfigError> {
        let prefix = parent.join(self.name);

        push_tree_files(
            &prefix,
            self.files,
            config_path,
            schemas,
            defaults,
            targets,
        )?;
        for folder in self.folders {
            folder.collect_targets(
                &prefix,
                config_path,
                schemas,
                defaults,
                targets,
            )?;
        }

        Ok(())
    }
}

impl RenderTargetFile {
    pub(super) fn into_render_target(
        self,
        parent: &Path,
        config_path: &Path,
        schemas: &HashMap<String, RenderTargetSchema>,
        defaults: &RawTargetDefaults,
    ) -> Result<RenderTarget, TargetConfigError> {
        self.target.into_render_target(
            tree_output_path(parent, &self.name),
            config_path,
            schemas,
            defaults,
        )
    }
}

impl RenderTargetDefinition {
    pub(super) fn into_render_target(
        self,
        default_output_path: String,
        config_path: &Path,
        schemas: &HashMap<String, RenderTargetSchema>,
        defaults: &RawTargetDefaults,
    ) -> Result<RenderTarget, TargetConfigError> {
        let explicit_path_like_output = self.path_scope.is_some() || self.scope.is_some();
        let (repo_scope, file_kind, path_scope) = resolve_scope_fields(
            &self.name,
            self.repo_scope,
            self.file_kind,
            self.path_scope,
            self.scope,
            defaults,
        )?;
        let section = self.section.or_else(|| defaults.section.clone());
        let state = self.state.or_else(|| defaults.state.clone());
        let output_path = if explicit_path_like_output {
            path_scope.clone().unwrap_or(default_output_path)
        } else {
            default_output_path
        };
        Ok(RenderTarget {
            name: self.name.clone(),
            repo_scope,
            file_kind,
            path_scope,
            section,
            state,
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
