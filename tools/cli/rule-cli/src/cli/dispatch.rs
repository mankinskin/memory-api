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
    BenchmarkOperation,
    BenchmarkTargetsArgs,
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
    SyncRulesArgs,
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
    importing::{
        import_file,
        sync_rules_file,
    },
    rendering::{
        display_path,
        ensure_generated_output_matches,
        generate_target_payload,
        sync_targets_payload,
        validate_generate_args,
        validate_generate_target_args,
        validate_sync_target_args,
        write_generated_output,
    },
};

#[path = "dispatch_secondary.rs"]
mod dispatch_secondary;
use dispatch_secondary::*;

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

#[cfg(test)]
pub(super) fn dispatch(
    command: RuleCommandCli,
    index_root: &Path,
) -> Result<Value, CliRunError> {
    dispatch_with_workspace_root(command, index_root, None)
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
            | RuleCommandCli::SyncRules(_)
            | RuleCommandCli::BenchmarkTargets(_)
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
        RuleCommandCli::SyncRules(args) => sync_rules_command(store, args),
        RuleCommandCli::BenchmarkTargets(args) =>
            benchmark_targets_command(store, args),
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
                        canonical_parent
                            .join(absolute.file_name().unwrap_or_default())
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
            CliRunError::BadRequest(format!(
                "invalid --resume journal UUID: {e}"
            ))
        })?;
        let outcome = store.resume_move_with_journal(journal)?;
        return Ok(
            json!({"command":"move","status":"ok","mode":"resume","journal_id":outcome.journal.id,"phase":outcome.journal.phase}),
        );
    }
    if let Some(journal) = args.rollback.as_deref() {
        let journal = journal.parse::<uuid::Uuid>().map_err(|e| {
            CliRunError::BadRequest(format!(
                "invalid --rollback journal UUID: {e}"
            ))
        })?;
        let outcome = store.rollback_move_with_journal(journal)?;
        return Ok(
            json!({"command":"move","status":"ok","mode":"rollback","journal_id":outcome.journal.id,"phase":outcome.journal.phase}),
        );
    }
    let id = args.id.as_deref().ok_or_else(|| {
        CliRunError::BadRequest(
            "move requires <id> unless --resume/--rollback".to_string(),
        )
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
    Ok(
        json!({"command":"move","status":"ok","mode":"execute","rule_id":rule_id,"journal_id":outcome.journal.id,"phase":outcome.journal.phase}),
    )
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
                args.path_scope.into_iter().map(Value::String).collect(),
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
