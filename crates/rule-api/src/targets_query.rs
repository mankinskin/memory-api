use super::*;

pub(super) fn collect_target_node_rules(
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

pub(super) fn explain_target_node(
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

pub(super) fn rule_match_summary(rule: &RuleManifest) -> ExplainedRuleMatch {
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
    DirectoryPathWithSuggestion { path: PathBuf, suggested: PathBuf },
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
    #[error(
        "render target selector {selector} matches multiple targets: {matches}"
    )]
    AmbiguousSelector { selector: String, matches: String },
    #[error("duplicate render target name: {0}")]
    DuplicateName(String),
    #[error("duplicate render target schema name: {0}")]
    DuplicateSchemaName(String),
    #[error("render target config import cycle detected at {path}")]
    ImportCycle { path: PathBuf },
    #[error("render target {target} references unknown schema {schema}")]
    UnknownSchema { target: String, schema: String },
    #[error(
        "render target {target} missing required {target_kind} README block {block} from schema {schema}"
    )]
    MissingRequiredBlock {
        target: String,
        schema: String,
        target_kind: String,
        block: String,
    },
    #[error(
        "render target \"{target}\": missing repo_scope \
         (set it explicitly, via `scope: \"repo:path\"`, or in a `defaults:` block)"
    )]
    MissingRepoScope { target: String },
    #[error(
        "render target \"{target}\": missing file_kind \
         (set it explicitly, in a `defaults:` block, or use a recognised path \
         like AGENTS.md, README.md, *.agent.md, *.prompt.md, *.instructions.md)"
    )]
    MissingFileKind { target: String },
    #[error(
        "render target \"{target}\": invalid scope \"{scope}\" \
         (expected \"repo_scope:path_scope\", both parts non-empty)"
    )]
    InvalidScope { target: String, scope: String },
    #[error(
        "render target \"{target}\": missing output_path \
         (set it explicitly or provide a `path_scope` / `scope:` shorthand to use as default)"
    )]
    MissingOutputPath { target: String },
}

pub(crate) fn directory_target_config_error(path: &Path) -> TargetConfigError {
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

fn suggested_render_target_config_path(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let stem = path.file_name()?.to_str()?;

    ["yaml", "yml", "toml"]
        .into_iter()
        .map(|extension| parent.join(format!("{stem}.{extension}")))
        .find(|candidate| candidate.is_file())
}

fn normalize_render_target_selector(selector: &str) -> String {
    selector.replace('\\', "/")
}

pub fn load_render_target_config(
    path: &Path
) -> Result<RenderTargetConfig, TargetConfigError> {
    let mut ctx = LoadCtx::default();
    let loaded = if path.is_dir() {
        load_import_targets(path, &mut ctx)?
    } else {
        load_render_target_config_inner(path, &mut ctx)?
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
                || normalize_render_target_selector(
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
