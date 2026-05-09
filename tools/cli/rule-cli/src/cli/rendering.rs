use std::{
    fs,
    path::Path,
};

use rule_api::{
    GENERATED_FILE_COMMENT,
    RenderTarget,
    RuleStore,
    collect_target_rules,
    load_render_target_config,
    render_markdown_file,
    resolve_render_target_output,
};
use serde_json::{
    Value,
    json,
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

    if existing == rendered {
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
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            CliRunError::BadRequest(format!(
                "create {}: {err}",
                parent.display()
            ))
        })?;
    }

    fs::write(output, rendered).map_err(|err| {
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

pub(super) fn sync_targets_payload(
    store: &mut RuleStore,
    config_path: &Path,
    dry_run: bool,
    check: bool,
) -> Result<SyncTargetsPayload, CliRunError> {
    let config = load_render_target_config(config_path)?;
    let previous = store.list_generated_targets(config_path)?;
    let current_outputs = config
        .targets
        .iter()
        .map(|target| {
            stable_output_key(&resolve_render_target_output(
                config_path,
                target,
            ))
        })
        .collect::<std::collections::HashSet<_>>();

    let mut generated = Vec::new();
    for target in &config.targets {
        let output = resolve_render_target_output(config_path, target);
        let payload =
            generate_target_payload(store, target, dry_run, check, &output)?;

        if !dry_run && !check {
            if let Some(previous_record) = previous
                .iter()
                .find(|record| record.target_name == target.name)
            {
                if previous_record.output_path != stable_output_key(&output)
                    && !current_outputs.contains(&previous_record.output_path)
                {
                    remove_generated_output(
                        Path::new(&previous_record.output_path),
                        config_root(config_path),
                    )?;
                }
            }
            store.upsert_generated_target(
                config_path,
                &target.name,
                &output,
            )?;
        }

        generated.push(json!({
            "target": target.name,
            "output": output,
            "count": payload.count,
            "content": payload.content,
        }));
    }

    let stale = previous
        .into_iter()
        .filter(|record| {
            !config
                .targets
                .iter()
                .any(|target| target.name == record.target_name)
        })
        .collect::<Vec<_>>();

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
