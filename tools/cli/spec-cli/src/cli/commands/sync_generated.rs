use std::{
    collections::BTreeSet,
    path::{
        Path,
        PathBuf,
    },
};

use memory_api::generated_markdown::GeneratedMarkdownSnippet;
use rule_api::{
    RuleManifest,
    RuleStore,
    collect_target_rules,
    discover_workspace_scan_roots,
    load_render_target_config,
    render_target_by_name,
};
use serde_json::{
    Value,
    json,
};
use spec_api::{
    SpecStore,
    store::GeneratedSpecArtifactTarget,
};

use crate::cli::{
    CliRunError,
    SyncGeneratedArgs,
};

pub(crate) fn cmd_sync_generated(
    args: SyncGeneratedArgs,
    store: &mut SpecStore,
    default_workspace_root: &Path,
) -> Result<Value, CliRunError> {
    let spec = store.get(&args.id)?;
    let workspace_root = inferred_workspace_root_for_spec(
        store,
        spec.id,
        default_workspace_root,
    );
    let artifacts =
        store.get_generated_artifacts(&args.id)?.ok_or_else(|| {
            CliRunError::BadRequest(format!(
                "spec '{}' does not declare generated artifacts",
                args.id
            ))
        })?;
    let rule_store = open_rule_store(&workspace_root)?;

    let mut generated = Vec::new();

    if let Some(target) = artifacts.body.as_ref() {
        let rules =
            collect_rules_for_target(&rule_store, &workspace_root, target)?;
        let snippets = rules_as_snippets(&rules);
        store.update_generated_body(&args.id, &snippets)?;
        generated.push(json!({
            "artifact": "body.md",
            "config": target.config,
            "target": target.target,
            "count": rules.len(),
        }));
    }

    for (name, target) in &artifacts.sections {
        let rules =
            collect_rules_for_target(&rule_store, &workspace_root, target)?;
        let snippets = rules_as_snippets(&rules);
        store.update_generated_section(&args.id, name, &snippets)?;
        generated.push(json!({
            "artifact": format!("sections/{}.md", name),
            "config": target.config,
            "target": target.target,
            "count": rules.len(),
        }));
    }

    // Reuse the normal manifest update path so body-backed search results and
    // history handling stay aligned with the rest of spec-cli.
    let refreshed =
        store.update(&args.id, std::collections::BTreeMap::new(), None)?;

    Ok(json!({
        "command": "sync_generated",
        "status": "ok",
        "id": refreshed.id,
        "workspace_root": workspace_root.to_string_lossy().replace('\\', "/"),
        "count": generated.len(),
        "generated": generated,
    }))
}

fn open_rule_store(workspace_root: &Path) -> Result<RuleStore, CliRunError> {
    let mut store = RuleStore::open(workspace_root)?;
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

    store.scan(reindex)?;
    Ok(store)
}

fn collect_rules_for_target(
    store: &RuleStore,
    workspace_root: &Path,
    target: &GeneratedSpecArtifactTarget,
) -> Result<Vec<RuleManifest>, CliRunError> {
    let config_path = resolve_config_path(workspace_root, &target.config);
    let config = load_render_target_config(&config_path)?;
    let render_target = render_target_by_name(&config, &target.target)?;
    collect_target_rules(store, render_target).map_err(CliRunError::from)
}

fn resolve_config_path(
    workspace_root: &Path,
    config: &str,
) -> PathBuf {
    let config_path = PathBuf::from(config);
    if config_path.is_absolute() {
        config_path
    } else {
        workspace_root.join(config_path)
    }
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

fn inferred_workspace_root_for_spec(
    store: &SpecStore,
    spec_id: uuid::Uuid,
    default_workspace_root: &Path,
) -> PathBuf {
    store
        .entity_store()
        .get_indexed(&spec_id)
        .ok()
        .flatten()
        .and_then(|indexed| {
            workspace_root_for_indexed_spec(store, &indexed.path)
        })
        .or_else(|| {
            workspace_root_from_store_root(&store.entity_store().index_root)
        })
        .unwrap_or_else(|| default_workspace_root.to_path_buf())
}

fn workspace_root_for_indexed_spec(
    store: &SpecStore,
    spec_path: &Path,
) -> Option<PathBuf> {
    let scan_root = store
        .entity_store()
        .list_scan_roots()
        .ok()?
        .into_iter()
        .filter(|root| spec_path.starts_with(&root.path))
        .max_by_key(|root| root.path.components().count());

    scan_root
        .as_ref()
        .and_then(|root| workspace_root_from_scan_root(&root.path))
        .or_else(|| workspace_root_from_spec_path(spec_path))
}

fn workspace_root_from_scan_root(scan_root: &Path) -> Option<PathBuf> {
    let parent = scan_root.parent()?;
    if parent.file_name().and_then(|name| name.to_str()) == Some(".spec") {
        parent.parent().map(Path::to_path_buf)
    } else {
        Some(parent.to_path_buf())
    }
}

fn workspace_root_from_store_root(store_root: &Path) -> Option<PathBuf> {
    let workspace_root =
        memory_api::workspace::resolve_workspace_root_from_store_root(
            store_root, ".spec",
        );
    if workspace_root.as_os_str().is_empty() {
        None
    } else {
        Some(workspace_root)
    }
}

fn workspace_root_from_spec_path(spec_path: &Path) -> Option<PathBuf> {
    spec_path.ancestors().find_map(|ancestor| {
        if ancestor.file_name().and_then(|name| name.to_str()) == Some(".spec")
        {
            ancestor.parent().map(Path::to_path_buf)
        } else {
            None
        }
    })
}

#[cfg(test)]
#[path = "sync_generated_tests.rs"]
mod tests;
