use std::{
    collections::{
        BTreeSet,
    },
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
    let artifacts = store
        .get_generated_artifacts(&args.id)?
        .ok_or_else(|| {
            CliRunError::BadRequest(format!(
                "spec '{}' does not declare generated artifacts",
                args.id
            ))
        })?;
    let rule_store = open_rule_store(&workspace_root)?;

    let mut generated = Vec::new();

    if let Some(target) = artifacts.body.as_ref() {
        let rules = collect_rules_for_target(&rule_store, &workspace_root, target)?;
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
        let rules = collect_rules_for_target(&rule_store, &workspace_root, target)?;
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
    let refreshed = store.update(
        &args.id,
        std::collections::BTreeMap::new(),
        None,
    )?;

    Ok(json!({
        "command": "sync_generated",
        "status": "ok",
        "id": refreshed.id,
        "workspace_root": workspace_root.to_string_lossy().replace('\\', "/"),
        "count": generated.len(),
        "generated": generated,
    }))
}

fn open_rule_store(
    workspace_root: &Path,
) -> Result<RuleStore, CliRunError> {
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
    rules: &[RuleManifest],
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

fn workspace_root_from_scan_root(
    scan_root: &Path,
) -> Option<PathBuf> {
    let parent = scan_root.parent()?;
    if parent.file_name().and_then(|name| name.to_str()) == Some(".spec") {
        parent.parent().map(Path::to_path_buf)
    } else {
        Some(parent.to_path_buf())
    }
}

fn workspace_root_from_store_root(
    store_root: &Path,
) -> Option<PathBuf> {
    let workspace_root = memory_api::workspace::resolve_workspace_root_from_store_root(
        store_root,
        ".spec",
    );
    if workspace_root.as_os_str().is_empty() {
        None
    } else {
        Some(workspace_root)
    }
}

fn workspace_root_from_spec_path(
    spec_path: &Path,
) -> Option<PathBuf> {
    spec_path.ancestors().find_map(|ancestor| {
        if ancestor.file_name().and_then(|name| name.to_str())
            == Some(".spec")
        {
            ancestor.parent().map(Path::to_path_buf)
        } else {
            None
        }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;

    use tempfile::tempdir;

    use super::*;
    use crate::cli::{
        SearchArgs,
        commands::cmd_search,
    };
    use spec_api::{
        SpecManifest,
        store::GeneratedSpecArtifacts,
    };

    fn create_sync_fixture(
    ) -> (tempfile::TempDir, PathBuf, PathBuf, SpecStore, String) {
        let dir = tempdir().unwrap();
        let repo_root = dir.path().join("repo");
        let child_root = repo_root.join("memory-api");
        fs::create_dir_all(&child_root).unwrap();
        fs::create_dir_all(child_root.join(".rule")).unwrap();
        fs::create_dir_all(child_root.join(".spec")).unwrap();

        let mut rule_store = RuleStore::init(&child_root).unwrap();

        let mut body_rule = RuleManifest::new(
            "shared/spec/generated/body",
            "Generated Body",
            "spec-doc",
            "body",
            "## Overview\nGenerated body text for search.\n",
        );
        body_rule.set_repo_scopes(["memory-api"]);

        let mut requirements_rule = RuleManifest::new(
            "shared/spec/generated/requirements",
            "Generated Requirements",
            "spec-doc",
            "requirements",
            "## Requirements\nGenerated section content.\n",
        );
        requirements_rule.set_repo_scopes(["memory-api"]);

        rule_store.create(&body_rule, None).unwrap();
        rule_store.create(&requirements_rule, None).unwrap();

        fs::write(
            child_root.join("rule-targets.yaml"),
            concat!(
                "targets:\n",
                "  - name: spec-body\n",
                "    repo_scope: memory-api\n",
                "    file_kind: spec-doc\n",
                "    output_path: generated/body.md\n",
                "    nodes:\n",
                "      - name: body\n",
                "        section: body\n",
                "  - name: spec-requirements\n",
                "    repo_scope: memory-api\n",
                "    file_kind: spec-doc\n",
                "    output_path: generated/requirements.md\n",
                "    nodes:\n",
                "      - name: requirements\n",
                "        section: requirements\n",
            ),
        )
        .unwrap();

        let mut spec_store = SpecStore::init(&child_root).unwrap();
        let spec = SpecManifest::new(
            "spec-cli/generated-sync",
            "Generated Sync",
            "spec-cli",
        );
        let id = spec_store.create(&spec, "placeholder body", None).unwrap();

        let mut sections = BTreeMap::new();
        sections.insert(
            "requirements".to_string(),
            GeneratedSpecArtifactTarget {
                config: "rule-targets.yaml".into(),
                target: "spec-requirements".into(),
            },
        );

        spec_store
            .update_generated_artifacts(
                &id.to_string(),
                &GeneratedSpecArtifacts {
                    body: Some(GeneratedSpecArtifactTarget {
                        config: "rule-targets.yaml".into(),
                        target: "spec-body".into(),
                    }),
                    sections,
                },
            )
            .unwrap();

        (dir, repo_root, child_root, spec_store, id.to_string())
    }

    #[test]
    fn sync_generated_uses_owning_workspace_and_updates_searchable_body() {
        let (_dir, repo_root, child_root, mut store, id) = create_sync_fixture();

        let payload = cmd_sync_generated(
            SyncGeneratedArgs { id: id.clone() },
            &mut store,
            &repo_root,
        )
        .unwrap();

        assert_eq!(payload["command"], "sync_generated");
        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["count"], 2);
        assert_eq!(
            payload["workspace_root"],
            child_root.to_string_lossy().replace('\\', "/")
        );

        let (_spec, body) = store.get_full(&id).unwrap();
        assert!(body.contains("<!-- spec-api:file generated=true -->"));
        assert!(body.contains("Generated body text for search."));
        assert!(!body.contains("<!-- rule-api:file generated=true -->"));

        let section_path = store
            .entity_store()
            .get_indexed(&store.resolve_id(&id).unwrap())
            .unwrap()
            .unwrap()
            .path
            .join("sections")
            .join("requirements.md");
        let section = fs::read_to_string(section_path).unwrap();
        assert!(section.contains("Generated section content."));

        let search = cmd_search(
            SearchArgs {
                query: "Generated body text for search".into(),
                limit: 10,
            },
            &store,
        )
        .unwrap();
        assert_eq!(search["count"], 1);
        assert_eq!(search["items"][0]["id"], id);
    }

    #[test]
    fn sync_generated_fails_when_declared_target_is_missing() {
        let (_dir, repo_root, _child_root, mut store, id) = create_sync_fixture();

        store
            .update_generated_artifacts(
                &id,
                &GeneratedSpecArtifacts {
                    body: Some(GeneratedSpecArtifactTarget {
                        config: "rule-targets.yaml".into(),
                        target: "missing-target".into(),
                    }),
                    sections: BTreeMap::new(),
                },
            )
            .unwrap();

        let error = cmd_sync_generated(
            SyncGeneratedArgs { id },
            &mut store,
            &repo_root,
        )
        .unwrap_err();

        assert!(error.to_string().contains("missing-target"));
    }
}
