use std::fs;
use std::path::Path;

use serde_json::{
    Value,
    json,
};

use memory_api::generated_markdown::prepare_generated_output;
use spec_api::{
    SPEC_INDEX_AGENT_HOOK_PATH,
    SpecCatalogSource,
    SpecStore,
    generate_spec_catalog,
};

use crate::cli::{
    CliRunError,
    StoreIndexArgs,
};

const STORE_DIR: &str = ".spec";

/// Generate (or check) the committed spec catalog artifacts:
/// `.spec/README.md`, `.spec/index.toon`, and `.agents/spec-catalog.md`.
pub(crate) fn cmd_store_index(
    args: StoreIndexArgs,
    store: &SpecStore,
    workspace_root: &Path,
) -> Result<Value, CliRunError> {
    // Join every spec manifest with its on-disk spec.toml path + body.
    let indexed = store.entity_store().list_indexed()?;
    let mut loaded = Vec::new();
    for entity in &indexed {
        if let Ok((manifest, body)) = store.get_full(&entity.id.to_string()) {
            let spec_file = entity.path.join("spec.toml");
            let source_path = memory_api::index_generator::to_relative_slash(
                workspace_root,
                &spec_file,
            );
            loaded.push((manifest, source_path, body));
        }
    }

    let sources: Vec<SpecCatalogSource<'_>> = loaded
        .iter()
        .map(|(manifest, source_path, body)| SpecCatalogSource {
            manifest,
            source_path: source_path.clone(),
            body: body.clone(),
        })
        .collect();

    let artifacts = generate_spec_catalog(&sources, STORE_DIR);

    let readme_path = workspace_root.join(STORE_DIR).join("README.md");
    let sidecar_path = workspace_root.join(STORE_DIR).join("index.toon");
    let agent_hook_path = workspace_root.join(SPEC_INDEX_AGENT_HOOK_PATH);

    let sidecar_toon = artifacts
        .sidecar
        .encode_toon()
        .map_err(|e| CliRunError::BadRequest(e.to_string()))?;

    // Match existing on-disk line endings so the diff is content-only.
    let readme_out = prepare_generated_output(
        &artifacts.readme_markdown,
        read_existing(&readme_path).as_deref(),
    );
    let agent_hook_out = prepare_generated_output(
        &artifacts.agent_hook_markdown,
        read_existing(&agent_hook_path).as_deref(),
    );
    let sidecar_out = prepare_generated_output(
        &sidecar_toon,
        read_existing(&sidecar_path).as_deref(),
    );

    let planned = [
        (&readme_path, &readme_out),
        (&sidecar_path, &sidecar_out),
        (&agent_hook_path, &agent_hook_out),
    ];

    if args.check {
        let drifted: Vec<String> = planned
            .iter()
            .filter(|(path, content)| {
                read_existing(path).as_deref() != Some(content.as_str())
            })
            .map(|(path, _)| display_path(path))
            .collect();

        if !drifted.is_empty() {
            return Err(CliRunError::BadRequest(format!(
                "spec store-index is out of date; regenerate and re-stage: {}",
                drifted.join(", ")
            )));
        }

        return Ok(json!({
            "command": "store-index",
            "status": "ok",
            "check": true,
            "drift": false,
            "specs": sources.len(),
        }));
    }

    let mut written = Vec::new();
    for (path, content) in planned {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(memory_api::error::StorageError::Io)?;
        }
        fs::write(path, content).map_err(memory_api::error::StorageError::Io)?;
        written.push(display_path(path));
    }

    let root_count = artifacts
        .sidecar
        .entries
        .iter()
        .filter(|e| e.tags.iter().any(|t| t == "root"))
        .count();

    Ok(json!({
        "command": "store-index",
        "status": "ok",
        "check": false,
        "specs": sources.len(),
        "roots": root_count,
        "written": written,
    }))
}

fn read_existing(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}
