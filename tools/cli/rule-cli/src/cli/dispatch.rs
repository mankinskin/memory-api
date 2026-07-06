use std::{
    collections::BTreeSet,
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use memory_api::model::filesystem::ScanRoot;
use rule_api::{
    FeedbackNoteKind,
    FeedbackRating,
    RuleFeedbackInput,
    RuleFilter,
    RuleManifest,
    RuleStore,
    discover_workspace_scan_roots,
    explain_target,
    load_render_target_config,
    render_markdown_file,
    render_target_by_selector,
    resolve_render_target_output,
};
use serde_json::{
    Value,
    json,
};

use super::{
    AddRootArgs,
    CliRunError,
    CreateArgs,
    ExplainTargetArgs,
    FeedbackArgs,
    GenerateFileArgs,
    GenerateTargetArgs,
    IdArgs,
    ImportFileArgs,
    ListArgs,
    MoveArgs,
    RuleCommandCli,
    ScanArgs,
    SearchArgs,
    StoreIndexArgs,
    SyncTargetsArgs,
    UpdateArgs,
    helpers::{
        apply_source_location,
        list_filter,
        parse_fields,
        read_body,
        read_optional_body,
        resolve_workspace_root,
        rule_json,
        rule_summary_json,
    },
    importing::import_file,
    rendering::{
        ensure_generated_output_matches,
        generate_target_payload,
        sync_targets_payload,
        validate_generate_args,
        validate_generate_target_args,
        validate_sync_target_args,
        write_generated_output,
    },
};

pub(super) fn dispatch_with_workspace_root(
    command: RuleCommandCli,
    index_root: &Path,
    workspace_root_override: Option<&Path>,
) -> Result<Value, CliRunError> {
    if matches!(command, RuleCommandCli::Init) {
        let store = RuleStore::init(index_root)?;
        return Ok(json!({
            "command": "init",
            "status": "ok",
            "workspace": store.entity_store().index_root.display().to_string(),
            "message": "workspace initialized",
        }));
    }

    let mut store = RuleStore::open_or_init(index_root)?;
    bootstrap_rule_store(
        &mut store,
        &command,
        index_root,
        workspace_root_override,
    )?;
    match command {
        RuleCommandCli::Create(args) => create_command(&mut store, args),
        RuleCommandCli::Get(args) => get_command(&store, args),
        RuleCommandCli::Delete(args) => delete_command(&mut store, args),
        RuleCommandCli::ImportFile(args) =>
            import_file_command(&mut store, args),
        RuleCommandCli::Update(args) => update_command(&mut store, args),
        RuleCommandCli::Feedback(args) => feedback_command(&mut store, args),
        RuleCommandCli::Scan(args) =>
            scan_command(&mut store, args, index_root, workspace_root_override),
        RuleCommandCli::Move(args) => move_command(&store, args),
        RuleCommandCli::Init => unreachable!("Init handled before store open"),
        other => dispatch_secondary(other, &mut store, index_root),
    }
}

fn bootstrap_rule_store(
    store: &mut RuleStore,
    command: &RuleCommandCli,
    index_root: &Path,
    workspace_root_override: Option<&Path>,
) -> Result<(), CliRunError> {
    if !matches!(
        command,
        RuleCommandCli::GenerateFile(_)
            | RuleCommandCli::GenerateTarget(_)
            | RuleCommandCli::ExplainTarget(_)
            | RuleCommandCli::SyncTargets(_)
            | RuleCommandCli::List(_)
            | RuleCommandCli::Search(_)
            | RuleCommandCli::StoreIndex(_)
    ) {
        return Ok(());
    }

    let Some(workspace_root) =
        resolve_workspace_root(command, index_root, workspace_root_override)
    else {
        return Ok(());
    };

    let reindex = discover_child_scan_roots(store, &workspace_root)?;
    store.scan(reindex)?;
    Ok(())
}

fn dispatch_secondary(
    command: RuleCommandCli,
    store: &mut RuleStore,
    index_root: &Path,
) -> Result<Value, CliRunError> {
    match command {
        RuleCommandCli::GenerateFile(args) =>
            generate_file_command(store, args),
        RuleCommandCli::GenerateTarget(args) =>
            generate_target_command(store, args),
        RuleCommandCli::ExplainTarget(args) =>
            explain_target_command(store, args),
        RuleCommandCli::SyncTargets(args) => sync_targets_command(store, args),
        RuleCommandCli::List(args) => list_command(store, args),
        RuleCommandCli::Search(args) => search_command(store, args),
        RuleCommandCli::StoreIndex(args) =>
            store_index_command(store, args, index_root),
        RuleCommandCli::AddRoot(args) => add_root_command(store, args),
        RuleCommandCli::Create(_)
        | RuleCommandCli::Get(_)
        | RuleCommandCli::Delete(_)
        | RuleCommandCli::ImportFile(_)
        | RuleCommandCli::Update(_)
        | RuleCommandCli::Feedback(_)
        | RuleCommandCli::Scan(_)
        | RuleCommandCli::Move(_)
        | RuleCommandCli::Init => unreachable!("handled in primary dispatch"),
    }
}

fn discover_child_scan_roots(
    store: &mut RuleStore,
    workspace_root: &Path,
) -> Result<bool, CliRunError> {
    let mut known_scan_roots = store
        .entity_store()
        .list_scan_roots()?
        .into_iter()
        .map(|root| root.path)
        .collect::<BTreeSet<_>>();
    let mut reindex = false;

    for root in discover_workspace_scan_roots(workspace_root) {
        if known_scan_roots.insert(root.path.clone()) {
            reindex = true;
        }
        store.entity_store().add_scan_root(root)?;
    }

    Ok(reindex)
}

fn display_scan_path(path: &Path) -> String {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        memory_api::workspace::working_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(path)
    };
    let normalized = fs::canonicalize(&absolute)
        .or_else(|_| {
            absolute.parent().map_or_else(
                || Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
                |parent| {
                    fs::canonicalize(parent).map(|canonical_parent| {
                        canonical_parent.join(
                            absolute.file_name().unwrap_or_default(),
                        )
                    })
                },
            )
        })
        .unwrap_or(absolute);
    let rendered = normalized.to_string_lossy().replace('\\', "/");
    rendered
        .strip_prefix("//?/")
        .unwrap_or(rendered.as_str())
        .to_string()
}

fn move_command(
    store: &RuleStore,
    args: MoveArgs,
) -> Result<Value, CliRunError> {
    if args.resume.is_some() && args.rollback.is_some() {
        return Err(CliRunError::BadRequest(
            "move accepts only one of --resume or --rollback".to_string(),
        ));
    }
    if let Some(journal) = args.resume.as_deref() {
        let journal = journal.parse::<uuid::Uuid>().map_err(|e| {
            CliRunError::BadRequest(format!("invalid --resume journal UUID: {e}"))
        })?;
        let outcome = store.resume_move_with_journal(journal)?;
        return Ok(json!({"command":"move","status":"ok","mode":"resume","journal_id":outcome.journal.id,"phase":outcome.journal.phase}));
    }
    if let Some(journal) = args.rollback.as_deref() {
        let journal = journal.parse::<uuid::Uuid>().map_err(|e| {
            CliRunError::BadRequest(format!("invalid --rollback journal UUID: {e}"))
        })?;
        let outcome = store.rollback_move_with_journal(journal)?;
        return Ok(json!({"command":"move","status":"ok","mode":"rollback","journal_id":outcome.journal.id,"phase":outcome.journal.phase}));
    }
    let id = args.id.as_deref().ok_or_else(|| {
        CliRunError::BadRequest("move requires <id> unless --resume/--rollback".to_string())
    })?;
    let to = args.to_workspace_root.as_deref().ok_or_else(|| {
        CliRunError::BadRequest("move requires --to-workspace-root".to_string())
    })?;
    let rule_id = store.resolve_id(id)?;
    let report = store.plan_move_preflight(&rule_id, to)?;
    if args.dry_run || !report.supported() {
        return Ok(json!({
            "command":"move",
            "status": if report.supported() {"ok"} else {"blocked"},
            "mode":"plan","dry_run":true,"rule_id":rule_id,
            "supported":report.supported(),"blockers":report.blockers,
        }));
    }
    let outcome = store.execute_move_with_journal(&report)?;
    Ok(json!({"command":"move","status":"ok","mode":"execute","rule_id":rule_id,"journal_id":outcome.journal.id,"phase":outcome.journal.phase}))
}

fn create_command(
    store: &mut RuleStore,
    args: CreateArgs,
) -> Result<Value, CliRunError> {
    let body = read_body(args.body, args.body_file.as_deref())?;
    let mut manifest = RuleManifest::new(
        &args.slug,
        &args.title,
        &args.file_kind,
        &args.section,
        &body,
    );
    if let Some(order_key) = args.order_key {
        manifest.set_order_key(order_key);
    }
    if !args.repo_scope.is_empty() {
        manifest.set_repo_scopes(args.repo_scope);
    }
    if !args.path_scope.is_empty() {
        manifest.set_path_scopes(args.path_scope);
    }
    apply_source_location(
        &mut manifest,
        args.source_repo.as_deref(),
        args.source_path.as_deref(),
        args.source_start_line,
        args.source_end_line,
    )?;

    let id = store.create(&manifest, None)?;
    Ok(json!({
        "status": "ok",
        "id": id,
        "slug": manifest.slug(),
        "title": manifest.title(),
        "file_kind": manifest.file_kind(),
        "section": manifest.section(),
    }))
}

fn get_command(
    store: &RuleStore,
    args: IdArgs,
) -> Result<Value, CliRunError> {
    let rule = store.get(&args.id)?;
    Ok(json!({
        "status": "ok",
        "rule": rule_json(&rule),
    }))
}

fn delete_command(
    store: &mut RuleStore,
    args: IdArgs,
) -> Result<Value, CliRunError> {
    store.delete(&args.id)?;
    Ok(json!({
        "status": "ok",
        "id": args.id,
    }))
}

fn import_file_command(
    store: &mut RuleStore,
    args: ImportFileArgs,
) -> Result<Value, CliRunError> {
    let items = import_file(store, &args)?;
    Ok(json!({
        "status": "ok",
        "count": items.len(),
        "dry_run": args.dry_run,
        "items": items,
    }))
}

fn update_command(
    store: &mut RuleStore,
    args: UpdateArgs,
) -> Result<Value, CliRunError> {
    let mut patch = parse_fields(&args.fields)?;
    if let Some(body) =
        read_optional_body(args.body, args.body_file.as_deref())?
    {
        store.update_body(&args.id, &body)?;
    }
    if !args.path_scope.is_empty() {
        patch.insert(
            "path_scopes".to_string(),
            Value::Array(
                args.path_scope
                    .into_iter()
                    .map(Value::String)
                    .collect(),
            ),
        );
    } else if !args.add_path_scope.is_empty() {
        let current = store.get(&args.id)?;
        let mut scopes = current.path_scopes();
        for s in args.add_path_scope {
            if !scopes.contains(&s) {
                scopes.push(s);
            }
        }
        patch.insert(
            "path_scopes".to_string(),
            Value::Array(scopes.into_iter().map(Value::String).collect()),
        );
    }
    let rule = store.update(&args.id, patch, args.to_state.as_deref())?;
    Ok(json!({
        "status": "ok",
        "rule": rule_json(&rule),
    }))
}

fn feedback_command(
    store: &mut RuleStore,
    args: FeedbackArgs,
) -> Result<Value, CliRunError> {
    let rating = args
        .rating
        .parse::<FeedbackRating>()
        .map_err(CliRunError::BadRequest)?;
    let note_kind = args
        .note_kind
        .as_deref()
        .map(str::parse::<FeedbackNoteKind>)
        .transpose()
        .map_err(CliRunError::BadRequest)?;
    let input = RuleFeedbackInput::new(
        rating,
        args.note,
        note_kind,
        args.session_id,
        args.agent_or_user_id,
    )
    .map_err(CliRunError::BadRequest)?;
    let (rule, event) = store.record_feedback(&args.id, input)?;

    Ok(json!({
        "status": "ok",
        "event": event,
        "rule": rule_json(&rule),
    }))
}

fn generate_file_command(
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

fn generate_target_command(
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

fn explain_target_command(
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

fn sync_targets_command(
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

fn list_command(
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

fn search_command(
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

fn scan_command(
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

fn store_index_command(
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

    // Join rule manifests with their indexed on-disk paths + mtimes.
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

    // Match existing on-disk line endings so the diff is content-only.
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
        fs::write(path, content).map_err(memory_api::error::StorageError::Io)?;
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

fn add_root_command(
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
