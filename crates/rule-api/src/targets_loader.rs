use super::*;

pub(super) fn push_tree_files(
    parent: &Path,
    files: Vec<RenderTargetFile>,
    config_path: &Path,
    schemas: &HashMap<String, RenderTargetSchema>,
    defaults: &RawTargetDefaults,
    targets: &mut Vec<RenderTarget>,
) -> Result<(), TargetConfigError> {
    for file in files {
        targets.push(file.into_render_target(
            parent,
            config_path,
            schemas,
            defaults,
        )?);
    }

    Ok(())
}

pub(super) fn resolve_target_nodes(
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

pub(super) fn validate_required_blocks(
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

pub(super) fn tree_output_path(
    parent: &Path,
    name: &str,
) -> String {
    let mut path = parent.to_path_buf();
    path.push(name);
    path.to_string_lossy().replace('\\', "/")
}

/// Parses a `scope` shorthand like `"repo_scope:path_scope"` and returns
/// `(repo_scope, path_scope)`.  Both parts must be non-empty.
pub(super) fn parse_scope(
    target_name: &str,
    scope: &str,
) -> Result<(String, String), TargetConfigError> {
    scope
        .split_once(':')
        .filter(|(repo, path)| !repo.is_empty() && !path.is_empty())
        .map(|(repo, path)| (repo.to_string(), path.to_string()))
        .ok_or_else(|| TargetConfigError::InvalidScope {
            target: target_name.to_string(),
            scope: scope.to_string(),
        })
}

/// Infers `file_kind` from a well-known path pattern.  Returns `None` when the
/// path does not match any recognised convention.
pub(super) fn infer_file_kind(path: &str) -> Option<&'static str> {
    let filename = path.rsplit('/').next().unwrap_or(path);
    match filename {
        "AGENTS.md" => Some("AGENTS"),
        "README.md" => Some("README"),
        "copilot-instructions.md" => Some("copilot-instructions"),
        _ if filename.ends_with(".agent.md") => Some(".agent"),
        _ if filename.ends_with(".prompt.md") => Some(".prompt"),
        _ if filename.ends_with(".instructions.md") => Some(".instructions"),
        _ if path.starts_with(".spec/specs/") => Some("spec-doc"),
        _ => None,
    }
}

/// Resolves `repo_scope`, `file_kind`, and `path_scope` for a target by
/// merging the explicit fields, the `scope` shorthand, and the config-level
/// `defaults` (in that precedence order).
pub(super) fn resolve_scope_fields(
    target_name: &str,
    explicit_repo_scope: Option<String>,
    explicit_file_kind: Option<String>,
    explicit_path_scope: Option<String>,
    scope: Option<String>,
    defaults: &RawTargetDefaults,
) -> Result<(String, String, Option<String>), TargetConfigError> {
    let (scope_repo, scope_path) = if let Some(s) = scope {
        let (r, p) = parse_scope(target_name, &s)?;
        (Some(r), Some(p))
    } else {
        (None, None)
    };

    let repo_scope = explicit_repo_scope
        .or(scope_repo)
        .or_else(|| defaults.repo_scope.clone())
        .ok_or_else(|| TargetConfigError::MissingRepoScope {
            target: target_name.to_string(),
        })?;

    let path_scope = explicit_path_scope
        .or(scope_path)
        .or_else(|| defaults.path_scope.clone());

    let file_kind = explicit_file_kind
        .or_else(|| defaults.file_kind.clone())
        .or_else(|| {
            path_scope
                .as_deref()
                .and_then(infer_file_kind)
                .map(str::to_owned)
        })
        .ok_or_else(|| TargetConfigError::MissingFileKind {
            target: target_name.to_string(),
        })?;

    Ok((repo_scope, file_kind, path_scope))
}

pub(super) fn parse_render_target_config(
    path: &Path
) -> Result<RawRenderTargetConfig, TargetConfigError> {
    let content =
        fs::read_to_string(path).map_err(|source| TargetConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;

    match path.extension().and_then(|extension| extension.to_str()) {
        Some("yaml" | "yml") => serde_yaml::from_str::<RawRenderTargetConfig>(
            &content,
        )
        .map_err(|source| TargetConfigError::ParseYaml {
            path: path.to_path_buf(),
            source,
        }),
        _ => toml::from_str::<RawRenderTargetConfig>(&content).map_err(
            |source| TargetConfigError::ParseToml {
                path: path.to_path_buf(),
                source,
            },
        ),
    }
}

pub(super) fn is_supported_render_target_config(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|extension| extension.to_str()),
        Some("yaml" | "yml" | "toml")
    )
}

pub(super) fn resolve_config_output_root(config_path: &Path) -> PathBuf {
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));

    for ancestor in config_dir.ancestors() {
        if ancestor.file_name().and_then(|name| name.to_str())
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

pub(super) fn resolve_import_path(
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

pub(super) fn load_import_targets(
    import_path: &Path,
    ctx: &mut LoadCtx,
) -> Result<LoadedRenderTargets, TargetConfigError> {
    let metadata =
        fs::metadata(import_path).map_err(|source| TargetConfigError::Io {
            path: import_path.to_path_buf(),
            source,
        })?;

    if !metadata.is_dir() {
        return load_render_target_config_inner(import_path, ctx);
    }

    let mut fragment_paths = Vec::new();
    for entry in
        fs::read_dir(import_path).map_err(|source| TargetConfigError::Io {
            path: import_path.to_path_buf(),
            source,
        })?
    {
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
        loaded.merge(load_render_target_config_inner(&fragment_path, ctx)?);
    }

    Ok(loaded)
}

pub(super) fn load_render_target_config_inner(
    path: &Path,
    ctx: &mut LoadCtx,
) -> Result<LoadedRenderTargets, TargetConfigError> {
    if path.is_dir() {
        return Err(directory_target_config_error(path));
    }

    let canonical =
        fs::canonicalize(path).map_err(|source| TargetConfigError::Io {
            path: path.to_path_buf(),
            source,
        })?;

    // Already loaded in a prior pass — skip silently to avoid duplicate
    // schemas/targets when a file is both imported and scanned directly.
    if ctx.visited.contains(&canonical) {
        return Ok(LoadedRenderTargets::default());
    }

    // Active call stack: detect cycles.
    if ctx.visiting.contains(&canonical) {
        return Err(TargetConfigError::ImportCycle {
            path: path.to_path_buf(),
        });
    }

    ctx.visiting.push(canonical.clone());
    let result = (|| {
        let raw = parse_render_target_config(path)?;
        let mut loaded = LoadedRenderTargets::default();

        for import in raw.imports.clone() {
            let import_path = resolve_import_path(path, &import);
            loaded.merge(load_import_targets(&import_path, ctx)?);
        }

        // Register schemas into the shared global map so all subsequent
        // fragments (siblings and children) can resolve references to them.
        for schema in raw.schemas.iter().cloned() {
            ctx.insert_schema(schema)?;
        }

        loaded
            .targets
            .extend(raw.into_render_targets(path, &ctx.schemas)?);
        Ok(loaded)
    })();
    ctx.visiting.pop();
    // Mark as fully loaded only on success so a parse/validation error
    // doesn't permanently silence the file on a later retry.
    if result.is_ok() {
        ctx.visited.insert(canonical);
    }
    result
}

