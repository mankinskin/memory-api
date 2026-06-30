use serde_json::{
    Value,
    json,
};

use spec_api::{
    SpecStore,
    code_ref::validate_refs,
};

use crate::cli::{
    CliRunError,
    RefsArgs,
    RefsSubcommand,
};

pub(crate) fn cmd_refs(
    args: RefsArgs,
    store: &SpecStore,
    default_workspace_root: &std::path::Path,
) -> Result<Value, CliRunError> {
    let spec = store.get(&args.id)?;

    match args.subcommand {
        Some(RefsSubcommand::Validate {
            code_workspace_root,
        }) => {
            let workspace_root = code_workspace_root
                .unwrap_or_else(|| {
                    inferred_workspace_root_for_spec(
                        store,
                        spec.id,
                        default_workspace_root,
                    )
                });
            let canonical_workspace_root =
                memory_api::workspace::canonicalize_workspace_root_strict(
                    &workspace_root,
                )
                .map_err(|error| {
                    CliRunError::BadRequest(format!(
                        "workspace root canonicalization failed for '{}': {error}",
                        workspace_root.display()
                    ))
                })?;
            let results = validate_refs(&spec.code_refs, &canonical_workspace_root);
            let items: Vec<Value> = results
                .iter()
                .map(|r| {
                    json!({
                        "file": r.code_ref.file,
                        "symbol": r.code_ref.symbol,
                        "kind": format!("{:?}", r.code_ref.kind),
                        "file_exists": r.file_exists,
                        "line_range_valid": r.line_range_valid,
                        "message": r.message,
                    })
                })
                .collect();
            let all_valid =
                results.iter().all(|r| r.file_exists && r.line_range_valid);
            Ok(json!({
                "command": "refs_validate",
                "status": "ok",
                "id": spec.id,
                "workspace_root": render_workspace_root_for_payload(
                    &canonical_workspace_root,
                )?,
                "valid": all_valid,
                "count": items.len(),
                "results": items,
            }))
        },
        None => {
            let refs: Vec<Value> = spec
                .code_refs
                .iter()
                .map(|r| {
                    json!({
                        "file": r.file,
                        "symbol": r.symbol,
                        "kind": format!("{:?}", r.kind),
                        "line_start": r.line_start,
                        "line_end": r.line_end,
                        "description": r.description,
                    })
                })
                .collect();
            Ok(json!({
                "command": "refs",
                "status": "ok",
                "id": spec.id,
                "count": refs.len(),
                "refs": refs,
            }))
        },
    }
}

fn render_workspace_root_for_payload(
    path: &std::path::Path,
) -> Result<String, CliRunError> {
    memory_api::workspace::normalize_path_for_display_strict(path).map_err(
        |error| {
            CliRunError::BadRequest(format!(
                "workspace root payload normalization failed for '{}': {error}",
                path.display()
            ))
        },
    )
}

fn inferred_workspace_root_for_spec(
    store: &SpecStore,
    spec_id: uuid::Uuid,
    default_workspace_root: &std::path::Path,
) -> std::path::PathBuf {
    store
        .entity_store()
        .get_indexed(&spec_id)
        .ok()
        .flatten()
        .map(|indexed| {
            let store_root = memory_api::workspace::resolve_store_root_from(
                &indexed.path,
                ".spec",
            );
            memory_api::workspace::resolve_workspace_root_from_store_root(
                &store_root,
                ".spec",
            )
        })
        .unwrap_or_else(|| default_workspace_root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::fs;

    use serde_json::Value;
    use spec_api::{
        SpecManifest,
        code_ref::{
            CodeRef,
            SymbolKind,
        },
    };
    use tempfile::tempdir;

    use super::*;

    fn create_spec_with_ref(
        workspace_root: &std::path::Path,
        file_root: &std::path::Path,
    ) -> (SpecStore, String) {
        let store_root = workspace_root.join(".spec");
        fs::create_dir_all(&store_root).unwrap();
        fs::create_dir_all(file_root.join("src")).unwrap();
        fs::write(file_root.join("src/lib.rs"), "pub fn target() {}\n").unwrap();

        let mut store = SpecStore::init(&store_root).unwrap();
        let mut manifest =
            SpecManifest::new("spec-cli/refs", "Refs", "spec-cli");
        manifest.code_refs = vec![CodeRef {
            file: "src/lib.rs".to_string(),
            symbol: "target".to_string(),
            kind: SymbolKind::Function,
            line_start: 1,
            line_end: 1,
            description: Some("validate me".to_string()),
        }];
        let id = store.create(&manifest, "body", None).unwrap().to_string();

        (store, id)
    }

    fn validate_payload(
        payload: &Value,
        expected_root: &std::path::Path,
    ) {
        assert_eq!(payload["command"], "refs_validate");
        assert_eq!(payload["valid"], true);
        assert_eq!(payload["count"], 1);
        assert_eq!(
            payload["workspace_root"],
            Value::String(
                render_workspace_root_for_payload(expected_root).unwrap(),
            )
        );
    }

    #[test]
    fn render_workspace_root_for_payload_normalizes_separators() {
        let rendered = render_workspace_root_for_payload(std::path::Path::new(
            r"C:\\repo\\memory-api",
        ))
        .unwrap();

        assert_eq!(rendered, "/c/repo/memory-api");
    }

    #[test]
    #[cfg(windows)]
    fn render_workspace_root_for_payload_strips_verbatim_prefix() {
        let rendered = render_workspace_root_for_payload(std::path::Path::new(
            r"\\?\C:\repo\memory-api",
        ))
        .unwrap();

        assert_eq!(rendered, "/c/repo/memory-api");
    }

    #[test]
    #[cfg(windows)]
    fn render_workspace_root_for_payload_preserves_unc_root() {
        let rendered = render_workspace_root_for_payload(std::path::Path::new(
            r"\\server\share\memory-api",
        ))
        .unwrap();

        assert_eq!(rendered, "//server/share/memory-api");
    }

    #[test]
    #[cfg(windows)]
    fn render_workspace_root_for_payload_normalizes_verbatim_unc_root() {
        let rendered = render_workspace_root_for_payload(std::path::Path::new(
            r"\\?\UNC\server\share\memory-api",
        ))
        .unwrap();

        assert_eq!(rendered, "//server/share/memory-api");
    }

    #[test]
    fn refs_validate_uses_default_workspace_root() {
        let dir = tempdir().unwrap();
        let workspace_root = dir.path().join("repo");
        let (store, id) = create_spec_with_ref(&workspace_root, &workspace_root);

        let payload = cmd_refs(
            RefsArgs {
                id,
                subcommand: Some(RefsSubcommand::Validate {
                    code_workspace_root: None,
                }),
            },
            &store,
            &workspace_root,
        )
        .unwrap();

        validate_payload(&payload, &workspace_root);
    }

    #[test]
    fn refs_validate_prefers_explicit_code_workspace_root() {
        let dir = tempdir().unwrap();
        let workspace_root = dir.path().join("repo");
        let override_root = dir.path().join("override");
        let (store, id) = create_spec_with_ref(&workspace_root, &override_root);

        let payload = cmd_refs(
            RefsArgs {
                id,
                subcommand: Some(RefsSubcommand::Validate {
                    code_workspace_root: Some(override_root.clone()),
                }),
            },
            &store,
            &workspace_root,
        )
        .unwrap();

        validate_payload(&payload, &override_root);
    }

    #[test]
    fn refs_validate_uses_owning_workspace_for_nested_spec() {
        let dir = tempdir().unwrap();
        let repo_root = dir.path().join("repo");
        let child_root = repo_root.join("memory-api");
        let (store, id) = create_spec_with_ref(&child_root, &child_root);

        let payload = cmd_refs(
            RefsArgs {
                id,
                subcommand: Some(RefsSubcommand::Validate {
                    code_workspace_root: None,
                }),
            },
            &store,
            &repo_root,
        )
        .unwrap();

        validate_payload(&payload, &child_root);
    }
}
