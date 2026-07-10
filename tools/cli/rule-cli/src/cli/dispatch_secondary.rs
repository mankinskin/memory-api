use super::*;

pub(super) fn generate_file_command(
    store: &RuleStore,
    args: GenerateFileArgs,
) -> Result<Value, CliRunError> {
    validate_generate_args(&args)?;
    let filter = RuleFilter {
        state: args.state.clone(),
        file_kind: Some(args.file_kind.clone()),
        section: args.section.clone(),
        repo_scope: Some(args.repo_scope.clone()),
        path_scope: args.path_scope.clone(),
        slug: None,
        has_low_feedback: None,
        has_unresolved_feedback: None,
    };
    let rules = store.list(&filter, None)?;
    let rendered = render_markdown_file(&rules);

    if args.check {
        let output = args.output.as_deref().expect("validated output path");
        ensure_generated_output_matches(output, &rendered)?;
    } else if !args.dry_run {
        let output = args.output.as_deref().expect("validated output path");
        write_generated_output(output, &rendered)?;
    }

    Ok(json!({
        "status": "ok",
        "count": rules.len(),
        "file_kind": args.file_kind,
        "repo_scope": args.repo_scope,
        "path_scope": args.path_scope,
        "section": args.section,
        "output": args.output,
        "dry_run": args.dry_run,
        "check": args.check,
        "content": args.dry_run.then_some(rendered),
    }))
}

pub(super) fn generate_target_command(
    store: &RuleStore,
    args: GenerateTargetArgs,
) -> Result<Value, CliRunError> {
    validate_generate_target_args(&args)?;
    let config = load_render_target_config(&args.config)?;
    let target =
        render_target_by_selector(&config, &args.config, &args.target)?;
    let output = resolve_render_target_output(&args.config, target);
    let payload = generate_target_payload(
        store,
        target,
        args.dry_run,
        args.check,
        &output,
    )?;

    Ok(json!({
        "status": "ok",
        "target": target.name,
        "output": output,
        "count": payload.count,
        "file_kind": target.file_kind,
        "repo_scope": target.repo_scope,
        "path_scope": target.path_scope,
        "section": target.section,
        "dry_run": args.dry_run,
        "check": args.check,
        "content": payload.content,
    }))
}

pub(super) fn explain_target_command(
    store: &RuleStore,
    args: ExplainTargetArgs,
) -> Result<Value, CliRunError> {
    let config = load_render_target_config(&args.config)?;
    let target =
        render_target_by_selector(&config, &args.config, &args.target)?;
    let output = resolve_render_target_output(&args.config, target);
    let outline = explain_target(store, target)?;

    Ok(json!({
        "status": "ok",
        "target": target.name,
        "output": output,
        "outline": outline,
    }))
}

pub(super) fn sync_targets_command(
    store: &mut RuleStore,
    args: SyncTargetsArgs,
) -> Result<Value, CliRunError> {
    validate_sync_target_args(&args)?;
    let payload =
        sync_targets_payload(store, &args.config, args.dry_run, args.check)?;

    Ok(json!({
        "status": "ok",
        "config": args.config,
        "generated": payload.generated,
        "removed": payload.removed,
        "dry_run": args.dry_run,
        "check": args.check,
    }))
}

pub(super) fn benchmark_targets_command(
    _store: &mut RuleStore,
    args: BenchmarkTargetsArgs,
) -> Result<Value, CliRunError> {
    if args.iterations == 0 {
        return Err(CliRunError::BadRequest(
            "--iterations must be >= 1".to_string(),
        ));
    }

    let workspace_roots = if args.workspace_roots.is_empty() {
        discover_benchmark_workspaces(&std::env::current_dir().map_err(
            |err| {
                CliRunError::BadRequest(format!(
                    "resolve current directory for benchmark: {err}"
                ))
            },
        )?)
    } else {
        args.workspace_roots.clone()
    };

    if workspace_roots.is_empty() {
        return Err(CliRunError::BadRequest(
            "no workspace roots found for benchmarking".to_string(),
        ));
    }

    let mut runs = Vec::new();
    for workspace_root in workspace_roots {
        let workspace_root =
            fs::canonicalize(&workspace_root).unwrap_or(workspace_root);
        let index_root = workspace_root.join(".rule");
        let mut workspace_store = RuleStore::open_or_init(&index_root)?;
        workspace_store.scan(false)?;

        let config_path = if args.config.is_absolute() {
            args.config.clone()
        } else {
            workspace_root.join(&args.config)
        };

        if !config_path.exists() {
            continue;
        }

        let config = load_render_target_config(&config_path)?;

        if matches!(
            args.operation,
            BenchmarkOperation::GenerateTarget | BenchmarkOperation::Both
        ) {
            let target_selector = args.target.clone().ok_or_else(|| {
                CliRunError::BadRequest(
                    "--target is required when operation includes generate-target"
                        .to_string(),
                )
            })?;
            let target = render_target_by_selector(
                &config,
                &config_path,
                &target_selector,
            )?;
            let output = resolve_render_target_output(&config_path, target);

            let durations = run_timed(args.iterations, || {
                generate_target_payload(
                    &workspace_store,
                    target,
                    true,
                    false,
                    &output,
                )
                .map(|_| ())
            })?;
            runs.push(operation_run_json(
                &workspace_root,
                "generate-target",
                Some(target.name.as_str()),
                &durations,
            ));
        }

        if matches!(
            args.operation,
            BenchmarkOperation::SyncTargets | BenchmarkOperation::Both
        ) {
            let durations = run_timed(args.iterations, || {
                sync_targets_payload(
                    &mut workspace_store,
                    &config_path,
                    true,
                    false,
                )
                .map(|_| ())
            })?;
            runs.push(operation_run_json(
                &workspace_root,
                "sync-targets",
                None,
                &durations,
            ));
        }
    }

    if runs.is_empty() {
        return Err(CliRunError::BadRequest(
            "no benchmark runs executed; check --config path and workspace roots"
                .to_string(),
        ));
    }

    Ok(json!({
        "status": "ok",
        "command": "benchmark-targets",
        "iterations": args.iterations,
        "config": args.config,
        "operation": format!("{:?}", args.operation),
        "runs": runs,
    }))
}

fn run_timed<F>(
    iterations: usize,
    mut op: F,
) -> Result<Vec<u128>, CliRunError>
where
    F: FnMut() -> Result<(), CliRunError>,
{
    let mut durations = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = std::time::Instant::now();
        op()?;
        durations.push(start.elapsed().as_micros());
    }
    Ok(durations)
}

fn operation_run_json(
    workspace_root: &Path,
    operation: &str,
    target: Option<&str>,
    durations_micros: &[u128],
) -> Value {
    let min = durations_micros.iter().min().copied().unwrap_or_default();
    let max = durations_micros.iter().max().copied().unwrap_or_default();
    let sum = durations_micros.iter().sum::<u128>();
    let mean = if durations_micros.is_empty() {
        0
    } else {
        sum / durations_micros.len() as u128
    };

    json!({
        "workspace_root": display_scan_path(workspace_root),
        "operation": operation,
        "target": target,
        "samples_micros": durations_micros,
        "min_micros": min,
        "mean_micros": mean,
        "max_micros": max,
    })
}

fn discover_benchmark_workspaces(root: &Path) -> Vec<PathBuf> {
    let mut workspaces = Vec::new();
    let candidates = [
        root.to_path_buf(),
        root.join("memory-api"),
        root.join("memory-viewers"),
        root.join("viewer-api"),
        root.join("context-stack"),
    ];

    for candidate in candidates {
        if candidate.join("rule-targets.yaml").exists() {
            workspaces.push(candidate);
        }
    }

    workspaces
}

pub(super) fn list_command(
    store: &RuleStore,
    args: ListArgs,
) -> Result<Value, CliRunError> {
    let filter = list_filter(&args.filter);
    let rules = store.list(&filter, args.limit)?;
    Ok(json!({
        "status": "ok",
        "count": rules.len(),
        "items": rules.iter().map(rule_summary_json).collect::<Vec<_>>(),
    }))
}

pub(super) fn search_command(
    store: &RuleStore,
    args: SearchArgs,
) -> Result<Value, CliRunError> {
    let filter = list_filter(&args.filter);
    let rules = store.search(&args.query, &filter, args.limit)?;
    Ok(json!({
        "status": "ok",
        "query": args.query,
        "count": rules.len(),
        "items": rules.iter().map(rule_summary_json).collect::<Vec<_>>(),
    }))
}

pub(super) fn scan_command(
    store: &mut RuleStore,
    args: ScanArgs,
    index_root: &Path,
    workspace_root_override: Option<&Path>,
) -> Result<Value, CliRunError> {
    let reindex = resolve_workspace_root(
        &RuleCommandCli::Scan(ScanArgs { force: args.force }),
        index_root,
        workspace_root_override,
    )
    .map(|workspace_root| discover_child_scan_roots(store, &workspace_root))
    .transpose()?
    .unwrap_or(false);
    let reindexed = args.force || reindex;
    let report = store.scan(reindexed)?;
    let default_scan_root = ScanRoot {
        path: store.entity_store().index_root.join("entities"),
        label: "default".to_string(),
    };
    let registered_scan_roots = store.entity_store().list_scan_roots()?;
    let mut seen_scan_roots = BTreeSet::new();
    let active_scan_roots = std::iter::once((&default_scan_root, "default"))
        .chain(
            registered_scan_roots
                .iter()
                .map(|root| (root, "registered")),
        )
        .filter_map(|(root, kind)| {
            let path = display_scan_path(&root.path);
            let key = format!("{kind}:{path}");
            seen_scan_roots.insert(key).then(|| {
                json!({
                    "kind": kind,
                    "label": root.label,
                    "path": path,
                })
            })
        })
        .collect::<Vec<_>>();
    let mut seen_diagnostics = BTreeSet::new();
    let diagnostics = report
        .diagnostics
        .iter()
        .filter_map(|diagnostic| {
            let path = display_scan_path(&diagnostic.path);
            let key = format!("{path}:{}", diagnostic.reason);
            seen_diagnostics.insert(key).then(|| {
                json!({
                    "path": path,
                    "reason": diagnostic.reason,
                })
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "status": "ok",
        "force": args.force,
        "reindexed": reindexed,
        "integrated": report.integrated,
        "integrated_entities": report.integrated,
        "integrated_description": "Number of entity folders found on disk and integrated into the index and search stores during this scan.",
        "pruned": report.pruned,
        "pruned_entities": report.pruned,
        "pruned_description": "Number of stale indexed entities removed during a reindex because they were no longer present on disk.",
        "scan_root_count": active_scan_roots.len(),
        "active_scan_roots": active_scan_roots,
        "diagnostics_count": diagnostics.len(),
        "diagnostics_description": "Manifest and parse problems encountered while scanning active roots. Each diagnostic includes the path and the parser error.",
        "diagnostics": diagnostics,
    }))
}

pub(super) fn store_index_command(
    store: &mut RuleStore,
    args: StoreIndexArgs,
    index_root: &Path,
) -> Result<Value, CliRunError> {
    use rule_api::{
        RuleCatalogSource,
        RuleFilter,
        generate_rule_catalog,
        prepare_generated_output,
    };

    const STORE_DIR: &str = ".rule";

    let workspace_root = rule_api::workspace_root_for_index_root(index_root)
        .or_else(|| index_root.parent().map(Path::to_path_buf))
        .ok_or_else(|| {
            CliRunError::BadRequest(
                "could not resolve workspace root for rule store".to_string(),
            )
        })?;

    let manifests = store.list(&RuleFilter::default(), None)?;
    let indexed: std::collections::HashMap<_, _> = store
        .entity_store()
        .list_indexed()?
        .into_iter()
        .map(|e| (e.id, e))
        .collect();

    let sources: Vec<RuleCatalogSource<'_>> = manifests
        .iter()
        .map(|manifest| {
            let entity = indexed.get(&manifest.id);
            let source_path = entity
                .map(|e| {
                    let file = e.path.join("rule.toml");
                    memory_api::index_generator::to_relative_slash(
                        &workspace_root,
                        &file,
                    )
                })
                .unwrap_or_default();
            RuleCatalogSource {
                manifest,
                source_path,
            }
        })
        .collect();

    let artifacts = generate_rule_catalog(&sources, STORE_DIR);

    let readme_path = workspace_root.join(STORE_DIR).join("README.md");
    let sidecar_path = workspace_root.join(STORE_DIR).join("index.toon");
    let agent_hook_path =
        workspace_root.join(rule_api::RULE_CATALOG_AGENT_HOOK_PATH);

    let sidecar_toon = artifacts
        .sidecar
        .encode_toon()
        .map_err(|e| CliRunError::BadRequest(e.to_string()))?;

    let readme_out = prepare_generated_output(
        &artifacts.readme_markdown,
        read_existing(&readme_path).as_deref(),
    );
    let agent_hook_out = prepare_generated_output(
        &artifacts.agent_hook_markdown,
        read_existing(&agent_hook_path).as_deref(),
    );
    let sidecar_out = prepare_generated_output(
        &sidecar_toon,
        read_existing(&sidecar_path).as_deref(),
    );

    let planned = [
        (&readme_path, &readme_out),
        (&sidecar_path, &sidecar_out),
        (&agent_hook_path, &agent_hook_out),
    ];

    if args.check {
        let drifted: Vec<String> = planned
            .iter()
            .filter(|(path, content)| {
                read_existing(path).as_deref() != Some(content.as_str())
            })
            .map(|(path, _)| display_scan_path(path))
            .collect();

        if !drifted.is_empty() {
            return Err(CliRunError::BadRequest(format!(
                "rule store-index is out of date; regenerate and re-stage: {}",
                drifted.join(", ")
            )));
        }

        return Ok(json!({
            "command": "store-index",
            "status": "ok",
            "check": true,
            "drift": false,
            "rules": sources.len(),
        }));
    }

    let mut written = Vec::new();
    for (path, content) in planned {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(memory_api::error::StorageError::Io)?;
        }
        fs::write(path, content)
            .map_err(memory_api::error::StorageError::Io)?;
        written.push(display_scan_path(path));
    }

    Ok(json!({
        "command": "store-index",
        "status": "ok",
        "check": false,
        "rules": sources.len(),
        "low_rated": artifacts
            .sidecar
            .entries
            .iter()
            .filter(|e| e.tags.iter().any(|t| t == "low-rated"))
            .count(),
        "written": written,
    }))
}

fn read_existing(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

pub(super) fn add_root_command(
    store: &mut RuleStore,
    args: AddRootArgs,
) -> Result<Value, CliRunError> {
    fs::create_dir_all(&args.path)
        .map_err(memory_api::error::StorageError::Io)?;
    let path =
        fs::canonicalize(&args.path).unwrap_or_else(|_| args.path.clone());
    let label = args.label.unwrap_or_else(|| {
        path.file_name()
            .and_then(std::ffi::OsStr::to_str)
            .unwrap_or("rules")
            .to_string()
    });
    store.entity_store().add_scan_root(ScanRoot {
        path: path.clone(),
        label: label.clone(),
    })?;
    Ok(json!({
        "status": "ok",
        "path": path,
        "label": label,
    }))
}
