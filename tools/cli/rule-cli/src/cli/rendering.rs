use std::{
    fs,
    path::Path,
};

use memory_api::generated_markdown::GeneratedMarkdownSnippet;
use rule_api::{
    GENERATED_FILE_COMMENT,
    RenderTarget,
    RenderTargetConfig,
    RuleManifest,
    RuleStore,
    collect_target_rules,
    load_render_target_config,
    prepare_generated_output,
    render_markdown_file,
    resolve_render_target_output,
    store::GeneratedTargetRecord,
};
use serde_json::{
    Value,
    json,
};
use spec_api::{
    SpecStore,
    render_generated_document,
};

use super::{
    CliRunError,
    GenerateFileArgs,
    GenerateTargetArgs,
    SyncTargetsArgs,
};

pub(super) struct GenerateTargetPayload {
    pub(super) count: usize,
    pub(super) content: Option<String>,
}

#[derive(Debug)]
pub(super) struct SyncTargetsPayload {
    pub(super) generated: Vec<Value>,
    pub(super) removed: Vec<Value>,
}

pub(super) fn validate_generate_args(
    args: &GenerateFileArgs
) -> Result<(), CliRunError> {
    if args.check && args.dry_run {
        return Err(CliRunError::BadRequest(
            "choose either --check or --dry-run".to_string(),
        ));
    }
    if (args.check || !args.dry_run) && args.output.is_none() {
        return Err(CliRunError::BadRequest(
            "--output is required unless --dry-run is used".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_generate_target_args(
    args: &GenerateTargetArgs
) -> Result<(), CliRunError> {
    if args.check && args.dry_run {
        return Err(CliRunError::BadRequest(
            "choose either --check or --dry-run".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_sync_target_args(
    args: &SyncTargetsArgs
) -> Result<(), CliRunError> {
    if args.check && args.dry_run {
        return Err(CliRunError::BadRequest(
            "choose either --check or --dry-run".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn ensure_generated_output_matches(
    output: &Path,
    rendered: &str,
) -> Result<(), CliRunError> {
    let existing = fs::read_to_string(output).map_err(|err| {
        CliRunError::BadRequest(format!(
            "read generated file {}: {err}",
            output.display()
        ))
    })?;
    let expected = prepare_generated_output(rendered, Some(&existing));

    if existing == expected {
        Ok(())
    } else {
        Err(CliRunError::BadRequest(format!(
            "generated output differs from {}",
            output.display()
        )))
    }
}

pub(super) fn write_generated_output(
    output: &Path,
    rendered: &str,
) -> Result<(), CliRunError> {
    let existing = fs::read_to_string(output).ok();
    let prepared = prepare_generated_output(rendered, existing.as_deref());

    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            CliRunError::BadRequest(format!(
                "create {}: {err}",
                parent.display()
            ))
        })?;
    }

    fs::write(output, prepared).map_err(|err| {
        CliRunError::BadRequest(format!(
            "write generated file {}: {err}",
            output.display()
        ))
    })
}

pub(super) fn generate_target_payload(
    store: &RuleStore,
    target: &RenderTarget,
    dry_run: bool,
    check: bool,
    output: &Path,
) -> Result<GenerateTargetPayload, CliRunError> {
    let rules = collect_target_rules(store, target)?;

    if rules.is_empty() && !dry_run && !check {
        return Err(CliRunError::BadRequest(format!(
            "refusing to write generated output {} for target {}: matched zero rules",
            output.display(),
            target.name,
        )));
    }

    if is_spec_doc_target(target) {
        let snippets = rules_as_snippets(&rules);
        let rendered = render_generated_document(&snippets);

        if check {
            ensure_spec_generated_output_matches(output, &snippets)?;
        } else if !dry_run {
            write_spec_generated_output(output, &snippets)?;
        }

        return Ok(GenerateTargetPayload {
            count: rules.len(),
            content: dry_run.then_some(rendered),
        });
    }

    let rendered = render_markdown_file(&rules);

    if check {
        ensure_generated_output_matches(output, &rendered)?;
    } else if !dry_run {
        write_generated_output(output, &rendered)?;
    }

    Ok(GenerateTargetPayload {
        count: rules.len(),
        content: dry_run.then_some(rendered),
    })
}

fn is_spec_doc_target(target: &RenderTarget) -> bool {
    target.file_kind == "spec-doc"
}

fn rules_as_snippets(
    rules: &[RuleManifest]
) -> Vec<GeneratedMarkdownSnippet<'_>> {
    rules
        .iter()
        .map(|rule| {
            GeneratedMarkdownSnippet::new(
                rule.id.to_string(),
                rule.slug(),
                rule.body().unwrap_or_default(),
            )
        })
        .collect()
}

fn open_spec_store_for_artifact(
    artifact_path: &Path
) -> Result<SpecStore, CliRunError> {
    let workspace_root = artifact_path
        .ancestors()
        .find(|ancestor| {
            ancestor.file_name().and_then(|name| name.to_str()) == Some(".spec")
        })
        .and_then(Path::parent)
        .ok_or_else(|| {
            CliRunError::BadRequest(format!(
                "spec-doc target output must live under .spec/specs/**: {}",
                artifact_path.display()
            ))
        })?;

    let mut store = SpecStore::open(&workspace_root)
        .map_err(|error| CliRunError::BadRequest(error.to_string()))?;
    store
        .scan(false)
        .map_err(|error| CliRunError::BadRequest(error.to_string()))?;
    Ok(store)
}

fn ensure_spec_generated_output_matches(
    artifact_path: &Path,
    snippets: &[GeneratedMarkdownSnippet<'_>],
) -> Result<(), CliRunError> {
    let store = open_spec_store_for_artifact(artifact_path)?;

    if store
        .generated_artifact_matches(artifact_path, snippets)
        .map_err(|error| CliRunError::BadRequest(error.to_string()))?
    {
        Ok(())
    } else {
        Err(CliRunError::BadRequest(format!(
            "generated output differs from {}",
            artifact_path.display()
        )))
    }
}

fn write_spec_generated_output(
    artifact_path: &Path,
    snippets: &[GeneratedMarkdownSnippet<'_>],
) -> Result<(), CliRunError> {
    let mut store = open_spec_store_for_artifact(artifact_path)?;
    store
        .sync_generated_artifact(artifact_path, snippets)
        .map_err(|error| CliRunError::BadRequest(error.to_string()))?;
    Ok(())
}

pub(super) fn sync_targets_payload(
    store: &mut RuleStore,
    config_path: &Path,
    dry_run: bool,
    check: bool,
) -> Result<SyncTargetsPayload, CliRunError> {
    let config = load_render_target_config(config_path)?;

    ensure_no_zero_match_targets(store, config_path, &config, dry_run, check)?;

    let previous = store.list_generated_targets(config_path)?;
    let current_outputs = current_output_keys(config_path, &config);

    let mut generated = Vec::new();
    for target in &config.targets {
        generated.push(sync_target_payload_entry(
            store,
            config_path,
            target,
            &previous,
            &current_outputs,
            dry_run,
            check,
        )?);
    }

    let stale = collect_stale_generated_targets(&config, previous);

    if check && !stale.is_empty() {
        return Err(CliRunError::BadRequest(format!(
            "stale generated targets remain for {}: {}",
            config_path.display(),
            stale
                .iter()
                .map(|record| format!(
                    "{} -> {}",
                    record.target_name, record.output_path
                ))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }

    let mut removed = Vec::new();
    for record in stale {
        if !dry_run && !check {
            remove_generated_output(
                Path::new(&record.output_path),
                config_root(config_path),
            )?;
            store.delete_generated_target(&record.slug)?;
        }
        removed.push(json!({
            "target": record.target_name,
            "output": record.output_path,
        }));
    }

    Ok(SyncTargetsPayload { generated, removed })
}

fn ensure_no_zero_match_targets(
    store: &RuleStore,
    config_path: &Path,
    config: &RenderTargetConfig,
    dry_run: bool,
    check: bool,
) -> Result<(), CliRunError> {
    if dry_run || check {
        return Ok(());
    }

    let zero_matches =
        config
            .targets
            .iter()
            .filter_map(|target| {
                let output = resolve_render_target_output(config_path, target);
                match collect_target_rules(store, target) {
                    Ok(rules) if rules.is_empty() => Some(Ok(format!(
                        "{} -> {}",
                        target.name,
                        output.display()
                    ))),
                    Ok(_) => None,
                    Err(error) => Some(Err(CliRunError::from(error))),
                }
            })
            .collect::<Result<Vec<_>, _>>()?;

    if zero_matches.is_empty() {
        Ok(())
    } else {
        Err(CliRunError::BadRequest(format!(
            "refusing to overwrite generated outputs because these targets matched zero rules: {}",
            zero_matches.join(", ")
        )))
    }
}

fn current_output_keys(
    config_path: &Path,
    config: &RenderTargetConfig,
) -> std::collections::HashSet<String> {
    config
        .targets
        .iter()
        .map(|target| {
            stable_output_key(&resolve_render_target_output(
                config_path,
                target,
            ))
        })
        .collect::<std::collections::HashSet<_>>()
}

fn sync_target_payload_entry(
    store: &mut RuleStore,
    config_path: &Path,
    target: &RenderTarget,
    previous: &[GeneratedTargetRecord],
    current_outputs: &std::collections::HashSet<String>,
    dry_run: bool,
    check: bool,
) -> Result<Value, CliRunError> {
    let output = resolve_render_target_output(config_path, target);
    let payload =
        generate_target_payload(store, target, dry_run, check, &output)?;

    if !dry_run && !check {
        maybe_remove_previous_generated_target(
            store,
            target,
            previous,
            current_outputs,
            &output,
            config_path,
        )?;
        if !is_spec_doc_target(target) {
            store.upsert_generated_target(
                config_path,
                &target.name,
                &output,
            )?;
        }
    }

    Ok(json!({
        "target": target.name,
        "output": output,
        "count": payload.count,
        "content": payload.content,
    }))
}

fn maybe_remove_previous_generated_target(
    store: &mut RuleStore,
    target: &RenderTarget,
    previous: &[GeneratedTargetRecord],
    current_outputs: &std::collections::HashSet<String>,
    output: &Path,
    config_path: &Path,
) -> Result<(), CliRunError> {
    let Some(previous_record) = previous
        .iter()
        .find(|record| record.target_name == target.name)
    else {
        return Ok(());
    };

    if is_spec_doc_target(target) {
        if previous_record.output_path != stable_output_key(output) {
            remove_generated_output(
                Path::new(&previous_record.output_path),
                config_root(config_path),
            )?;
        }
        store.delete_generated_target(&previous_record.slug)?;
        return Ok(());
    }

    if previous_record.output_path != stable_output_key(output)
        && !current_outputs.contains(&previous_record.output_path)
    {
        remove_generated_output(
            Path::new(&previous_record.output_path),
            config_root(config_path),
        )?;
    }

    Ok(())
}

fn collect_stale_generated_targets(
    config: &RenderTargetConfig,
    previous: Vec<GeneratedTargetRecord>,
) -> Vec<GeneratedTargetRecord> {
    previous
        .into_iter()
        .filter(|record| {
            !config
                .targets
                .iter()
                .any(|target| target.name == record.target_name)
        })
        .collect::<Vec<_>>()
}

fn stable_output_key(path: &Path) -> String {
    fs::canonicalize(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .replace('\\', "/")
}

fn config_root(config_path: &Path) -> &Path {
    config_path.parent().unwrap_or_else(|| Path::new("."))
}

fn remove_generated_output(
    output: &Path,
    stop_at: &Path,
) -> Result<(), CliRunError> {
    if !output.exists() {
        return Ok(());
    }

    let existing = fs::read_to_string(output).map_err(|err| {
        CliRunError::BadRequest(format!(
            "read generated file {}: {err}",
            output.display()
        ))
    })?;
    if !existing.starts_with(GENERATED_FILE_COMMENT) {
        return Err(CliRunError::BadRequest(format!(
            "refusing to remove non-generated file {}",
            output.display()
        )));
    }

    fs::remove_file(output).map_err(|err| {
        CliRunError::BadRequest(format!(
            "remove generated file {}: {err}",
            output.display()
        ))
    })?;
    prune_empty_parent_dirs(output, stop_at)?;
    Ok(())
}

fn prune_empty_parent_dirs(
    path: &Path,
    stop_at: &Path,
) -> Result<(), CliRunError> {
    let stop_at =
        fs::canonicalize(stop_at).unwrap_or_else(|_| stop_at.to_path_buf());
    let mut current = path.parent();

    while let Some(dir) = current {
        let canonical =
            fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        if canonical == stop_at {
            break;
        }

        let mut entries = fs::read_dir(dir).map_err(|err| {
            CliRunError::BadRequest(format!(
                "read directory {}: {err}",
                dir.display()
            ))
        })?;
        if entries.next().is_some() {
            break;
        }

        fs::remove_dir(dir).map_err(|err| {
            CliRunError::BadRequest(format!(
                "remove empty directory {}: {err}",
                dir.display()
            ))
        })?;
        current = dir.parent();
    }

    Ok(())
}
