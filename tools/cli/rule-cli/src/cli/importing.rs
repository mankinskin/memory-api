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
};
use serde_json::{
    Value,
    json,
};

use super::{
    CliRunError,
    ImportFileArgs,
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
