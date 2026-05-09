use std::{
    collections::BTreeMap,
    fs,
    path::{
        Path,
        PathBuf,
    },
};

use rmcp::{
    ErrorData as McpError,
    model::CallToolResult,
};
use serde_json::{
    Value,
    json,
};

use rule_api::{
    ImportedRuleBlock,
    MarkdownImportOptions,
    RuleManifest,
    import_markdown_blocks,
};

use super::{
    ImportRuleFileInput,
    RuleServer,
};

impl RuleServer {
    pub(super) async fn rule_import_file_tool(
        &self,
        input: ImportRuleFileInput,
    ) -> Result<CallToolResult, McpError> {
        self.with_store(|store| {
            let items = import_file(store, &input)?;
            Self::json_result(&json!({
                "status": "ok",
                "count": items.len(),
                "dry_run": input.dry_run,
                "items": items,
            }))
        })
        .await
    }
}

fn import_file(
    store: &mut rule_api::RuleStore,
    input: &ImportRuleFileInput,
) -> Result<Vec<Value>, McpError> {
    let path = PathBuf::from(&input.path);
    let content = fs::read_to_string(&path).map_err(|err| {
        McpError::invalid_params(
            format!("read {}: {err}", path.display()),
            None,
        )
    })?;
    let default_section = input
        .default_section
        .clone()
        .unwrap_or_else(|| default_section_from_path(&path));
    let imported_blocks = import_markdown_blocks(
        &content,
        &MarkdownImportOptions {
            slug_prefix: input.slug_prefix.clone(),
            default_section,
        },
    );
    let source_repo = input
        .source_repo
        .as_deref()
        .or_else(|| input.repo_scope.first().map(String::as_str))
        .ok_or_else(|| {
            McpError::invalid_params(
                "at least one repo_scope is required".to_string(),
                None,
            )
        })?;
    let source_path = path.to_string_lossy().replace('\\', "/");
    let target_root = input.target_root.as_ref().map(PathBuf::from);

    let mut items = Vec::new();
    for imported in imported_blocks {
        let mut manifest = RuleManifest::new(
            &imported.slug,
            &imported.title,
            &input.file_kind,
            &imported.section,
            &imported.body,
        );
        manifest.set_order_key(imported.order_key);
        manifest.set_repo_scopes(input.repo_scope.iter().map(String::as_str));
        if !input.path_scope.is_empty() {
            manifest
                .set_path_scopes(input.path_scope.iter().map(String::as_str));
        }
        manifest.set_source_location(
            source_repo,
            &source_path,
            imported.source_start_line,
            imported.source_end_line,
        );

        let action = if input.dry_run {
            "preview"
        } else if store.get(&imported.slug).is_ok() {
            let patch = import_patch(&manifest);
            store
                .update_body(&imported.slug, &imported.body)
                .map_err(RuleServer::rule_err)?;
            let _ = store
                .update(&imported.slug, patch, None)
                .map_err(RuleServer::rule_err)?;
            "updated"
        } else {
            let _ = store
                .create(&manifest, target_root.as_deref())
                .map_err(RuleServer::rule_err)?;
            "created"
        };

        items.push(imported_rule_json(&imported, action));
    }

    Ok(items)
}

fn default_section_from_path(path: &Path) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("imported")
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
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
