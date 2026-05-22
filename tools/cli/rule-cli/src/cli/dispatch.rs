use std::{
    collections::BTreeSet,
    fs,
    path::Path,
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
    render_target_by_name,
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
    RuleCommandCli,
    ScanArgs,
    SearchArgs,
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

pub(super) fn dispatch(
    command: RuleCommandCli,
    index_root: &Path,
) -> Result<Value, CliRunError> {
    dispatch_with_workspace_root(command, index_root, None)
}

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

    let mut store = RuleStore::open(index_root)?;
    if command_uses_descendant_scan_roots(&command) {
        if let Some(workspace_root) =
            resolve_workspace_root(
                &command,
                index_root,
                workspace_root_override,
            )
        {
            let mut known_scan_roots = store
                .entity_store()
                .list_scan_roots()?
                .into_iter()
                .map(|root| root.path)
                .collect::<BTreeSet<_>>();
            let mut reindex = false;

            for root in discover_workspace_scan_roots(&workspace_root) {
                if known_scan_roots.insert(root.path.clone()) {
                    reindex = true;
                }
                store.entity_store().add_scan_root(root)?;
            }
            store.scan(reindex)?;
        }
    }

    match command {
        RuleCommandCli::Create(args) => create_command(&mut store, args),
        RuleCommandCli::Get(args) => get_command(&store, args),
        RuleCommandCli::Delete(args) => delete_command(&mut store, args),
        RuleCommandCli::ImportFile(args) =>
            import_file_command(&mut store, args),
        RuleCommandCli::Update(args) => update_command(&mut store, args),
        RuleCommandCli::Feedback(args) => feedback_command(&mut store, args),
        RuleCommandCli::Init => unreachable!("Init handled before store open"),
        other => dispatch_secondary(other, &mut store),
    }
}

fn command_uses_descendant_scan_roots(command: &RuleCommandCli) -> bool {
    matches!(
        command,
        RuleCommandCli::Get(_)
            | RuleCommandCli::GenerateFile(_)
            | RuleCommandCli::GenerateTarget(_)
            | RuleCommandCli::ExplainTarget(_)
            | RuleCommandCli::SyncTargets(_)
            | RuleCommandCli::List(_)
            | RuleCommandCli::Search(_)
            | RuleCommandCli::Scan(_)
    )
}

fn dispatch_secondary(
    command: RuleCommandCli,
    store: &mut RuleStore,
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
        RuleCommandCli::Scan(args) => scan_command(store, args),
        RuleCommandCli::AddRoot(args) => add_root_command(store, args),
        RuleCommandCli::Create(_)
        | RuleCommandCli::Get(_)
        | RuleCommandCli::Delete(_)
        | RuleCommandCli::ImportFile(_)
        | RuleCommandCli::Update(_)
        | RuleCommandCli::Feedback(_)
        | RuleCommandCli::Init => unreachable!("handled in primary dispatch"),
    }
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

    let id = store.create(&manifest, args.target_root.as_deref())?;
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
    let patch = parse_fields(&args.fields)?;
    if let Some(body) =
        read_optional_body(args.body, args.body_file.as_deref())?
    {
        store.update_body(&args.id, &body)?;
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
    let target = render_target_by_name(&config, &args.target)?;
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
        "target": args.target,
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
    let target = render_target_by_name(&config, &args.target)?;
    let output = resolve_render_target_output(&args.config, target);
    let outline = explain_target(store, target)?;

    Ok(json!({
        "status": "ok",
        "target": args.target,
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
) -> Result<Value, CliRunError> {
    let report = store.scan(args.force)?;
    Ok(json!({
        "status": "ok",
        "force": args.force,
        "integrated": report.integrated,
        "pruned": report.pruned,
        "diagnostics_count": report.diagnostics.len(),
    }))
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
