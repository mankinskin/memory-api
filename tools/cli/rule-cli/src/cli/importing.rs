use std::{
    collections::BTreeMap,
    fs,
};

use rule_api::{
    ImportedRuleBlock,
    MarkdownImportOptions,
    RuleManifest,
    RuleStore,
    import_markdown_blocks,
    parse_generated_artifact,
};
use serde_json::{
    Value,
    json,
};

use super::{
    CliRunError,
    ImportFileArgs,
    SyncRulesArgs,
    helpers::default_section_from_path,
};

pub(super) fn import_file(
    store: &mut RuleStore,
    args: &ImportFileArgs,
) -> Result<Vec<Value>, CliRunError> {
    let content = fs::read_to_string(&args.path).map_err(|err| {
        CliRunError::BadRequest(format!("read {}: {err}", args.path.display()))
    })?;
    let default_section = args
        .default_section
        .clone()
        .unwrap_or_else(|| default_section_from_path(&args.path));
    let imported_blocks = import_markdown_blocks(
        &content,
        &MarkdownImportOptions {
            slug_prefix: args.slug_prefix.clone(),
            default_section,
        },
    );
    let source_repo = args
        .source_repo
        .as_deref()
        .or_else(|| args.repo_scope.first().map(String::as_str))
        .ok_or_else(|| {
            CliRunError::BadRequest(
                "at least one --repo is required".to_string(),
            )
        })?;
    let source_path = args.path.to_string_lossy().replace('\\', "/");

    let mut items = Vec::new();
    for imported in imported_blocks {
        let mut manifest = RuleManifest::new(
            &imported.slug,
            &imported.title,
            &args.file_kind,
            &imported.section,
            &imported.body,
        );
        manifest.set_order_key(imported.order_key);
        manifest.set_repo_scopes(args.repo_scope.iter().map(String::as_str));
        if !args.path_scope.is_empty() {
            manifest
                .set_path_scopes(args.path_scope.iter().map(String::as_str));
        }
        manifest.set_source_location(
            source_repo,
            &source_path,
            imported.source_start_line,
            imported.source_end_line,
        );

        let action = if args.dry_run {
            "preview"
        } else if store.get(&imported.slug).is_ok() {
            let patch = import_patch(&manifest);
            store.update_body(&imported.slug, &imported.body)?;
            let _ = store.update(&imported.slug, patch, None)?;
            "updated"
        } else {
            let _ = store.create(&manifest, None)?;
            "created"
        };

        items.push(imported_rule_json(&imported, action));
    }

    Ok(items)
}

pub(super) fn sync_rules_file(
    store: &RuleStore,
    args: &SyncRulesArgs,
) -> Result<Value, CliRunError> {
    if args.check && args.dry_run {
        return Err(CliRunError::BadRequest(
            "choose either --check or --dry-run".to_string(),
        ));
    }

    let content = fs::read_to_string(&args.file).map_err(|err| {
        CliRunError::BadRequest(format!("read {}: {err}", args.file.display()))
    })?;
    let parsed = parse_generated_artifact(&content).map_err(|err| {
        CliRunError::BadRequest(format!(
            "parse generated artifact {}: {err}",
            args.file.display()
        ))
    })?;

    let mut missing_ids = Vec::new();
    let mut spec_doc_ids = Vec::new();
    let mut entries = Vec::with_capacity(parsed.entries.len());

    for parsed_entry in parsed.entries {
        match store.get(&parsed_entry.id) {
            Ok(rule) => {
                if rule.file_kind() == Some("spec-doc") {
                    spec_doc_ids.push(parsed_entry.id.clone());
                }

                let existing_body = rule.body().unwrap_or_default().to_string();
                let changed = existing_body != parsed_entry.body;
                let slug_mismatch = parsed_entry
                    .slug
                    .as_deref()
                    .zip(rule.slug())
                    .map(|(marker_slug, rule_slug)| marker_slug != rule_slug)
                    .unwrap_or(false);

                entries.push(SyncRuleEntry {
                    id: parsed_entry.id,
                    marker_slug: parsed_entry.slug,
                    canonical_slug: rule.slug().map(str::to_string),
                    current_body: existing_body,
                    new_body: parsed_entry.body,
                    changed,
                    slug_mismatch,
                });
            },
            Err(_) => missing_ids.push(parsed_entry.id),
        }
    }

    if !missing_ids.is_empty() {
        return Err(CliRunError::BadRequest(format!(
            "orphan generated entry ids not found in store: {}",
            missing_ids.join(", ")
        )));
    }
    if !spec_doc_ids.is_empty() {
        return Err(CliRunError::BadRequest(format!(
            "reverse-sync of spec-doc artifacts is not supported: {}",
            spec_doc_ids.join(", ")
        )));
    }

    let changed_count = entries.iter().filter(|entry| entry.changed).count();
    if args.check && changed_count > 0 {
        return Err(CliRunError::BadRequest(format!(
            "reverse-sync drift detected for {}: {} entries would change",
            args.file.display(),
            changed_count
        )));
    }

    if !args.dry_run && !args.check {
        apply_entry_updates_atomically(store, &entries)?;
    }

    let items = entries
        .iter()
        .map(|entry| {
            json!({
                "id": entry.id,
                "slug": entry.canonical_slug,
                "marker_slug": entry.marker_slug,
                "slug_mismatch": entry.slug_mismatch,
                "changed": entry.changed,
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "status": "ok",
        "file": args.file.to_string_lossy().replace('\\', "/"),
        "count": items.len(),
        "changed": changed_count,
        "dry_run": args.dry_run,
        "check": args.check,
        "items": items,
    }))
}

#[derive(Debug, Clone)]
struct SyncRuleEntry {
    id: String,
    marker_slug: Option<String>,
    canonical_slug: Option<String>,
    current_body: String,
    new_body: String,
    changed: bool,
    slug_mismatch: bool,
}

fn apply_entry_updates_atomically(
    store: &RuleStore,
    entries: &[SyncRuleEntry],
) -> Result<(), CliRunError> {
    let mut applied: Vec<(&str, &str)> = Vec::new();
    for entry in entries.iter().filter(|entry| entry.changed) {
        if let Err(error) = store.update_body(&entry.id, &entry.new_body) {
            let mut rollback_errors = Vec::new();
            for (id, original_body) in applied.iter().rev() {
                if let Err(rollback_error) = store.update_body(id, original_body) {
                    rollback_errors.push(format!("{id}: {rollback_error}"));
                }
            }

            if rollback_errors.is_empty() {
                return Err(CliRunError::BadRequest(format!(
                    "reverse-sync failed while updating {}: {}",
                    entry.id, error
                )));
            }

            return Err(CliRunError::BadRequest(format!(
                "reverse-sync failed while updating {}: {}; rollback failed for {}",
                entry.id,
                error,
                rollback_errors.join(", ")
            )));
        }

        applied.push((entry.id.as_str(), entry.current_body.as_str()));
    }

    Ok(())
}

fn import_patch(manifest: &RuleManifest) -> BTreeMap<String, Value> {
    let mut patch = BTreeMap::new();
    for key in [
        "slug",
        "title",
        "file_kind",
        "section",
        "body",
        "order_key",
        "repo_scopes",
        "path_scopes",
        "source_repo",
        "source_path",
        "source_start_line",
        "source_end_line",
    ] {
        if let Some(value) = manifest.extra.get(key) {
            patch.insert(key.to_string(), value.clone());
        }
    }
    patch
}

fn imported_rule_json(
    imported: &ImportedRuleBlock,
    action: &str,
) -> Value {
    json!({
        "action": action,
        "slug": imported.slug,
        "title": imported.title,
        "section": imported.section,
        "order_key": imported.order_key,
        "source_start_line": imported.source_start_line,
        "source_end_line": imported.source_end_line,
    })
}
