use std::path::Path;

use serde_json::{
    Value,
    json,
};
use spec_api::{
    SpecManifest,
    SpecStore,
    code_ref::CodeRef,
};
use walkdir::WalkDir;

use crate::cli::{
    BootstrapArgs,
    CliRunError,
};

mod extract;

use extract::{
    ExtractedItem,
    extract_public_items,
};

// ── One spec to be generated per source file ──────────────────────────────────

#[derive(Debug)]
struct ModuleSpec {
    /// Human-readable title, e.g. "store"
    title: String,
    /// Slug for the spec, e.g. "ticket-api/storage/store"
    slug: String,
    /// Workspace-relative path to the source file
    workspace_rel_file: String,
    /// Parent slug — the crate root spec, or a parent module if nested.
    parent_slug: String,
    /// Public items found in this file.
    items: Vec<ExtractedItem>,
}

// ── Entry point ───────────────────────────────────────────────────────────────

pub fn cmd_bootstrap(
    args: BootstrapArgs,
    store: &mut SpecStore,
) -> Result<Value, CliRunError> {
    let crate_path = args.crate_path.canonicalize().map_err(|e| {
        CliRunError::BadRequest(format!("cannot resolve crate path: {e}"))
    })?;

    let workspace_root = args
        .source_workspace_root
        .map(|p| p.canonicalize().unwrap_or(p))
        .unwrap_or_else(|| {
            let cwd = std::env::current_dir().expect("cwd");
            cwd.canonicalize().unwrap_or(cwd)
        });

    // Crate name: from Cargo.toml name field, or last path segment
    let crate_name = read_crate_name(&crate_path).unwrap_or_else(|| {
        crate_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("crate")
            .to_string()
    });

    let component = args.component.unwrap_or_else(|| crate_name.clone());

    let src_dir = crate_path.join("src");
    if !src_dir.exists() {
        return Err(CliRunError::BadRequest(format!(
            "no src/ directory found at {}",
            crate_path.display()
        )));
    }

    // ── Scan all .rs files ─────────────────────────────────────────────────
    let module_specs =
        scan_src_dir(&src_dir, &crate_path, &workspace_root, &crate_name);

    if module_specs.is_empty() {
        return Ok(json!({
            "command": "bootstrap",
            "status": "ok",
            "crate": crate_name,
            "dry_run": args.dry_run,
            "created": 0,
            "skipped": 0,
            "message": "no public items found",
        }));
    }

    // ── Dry-run: just report what would be created ─────────────────────────
    if args.dry_run {
        let specs_json: Vec<Value> = module_specs
            .iter()
            .map(|ms| {
                json!({
                    "slug": ms.slug,
                    "title": ms.title,
                    "parent": ms.parent_slug,
                    "file": ms.workspace_rel_file,
                    "public_items": ms.items.len(),
                    "items": ms.items.iter().map(|i| json!({
                        "name": i.name,
                        "kind": format!("{:?}", i.kind),
                        "line_start": i.line_start,
                        "line_end": i.line_end,
                    })).collect::<Vec<_>>(),
                })
            })
            .collect();

        return Ok(json!({
            "command": "bootstrap",
            "status": "dry_run",
            "crate": crate_name,
            "dry_run": true,
            "would_create": 1 + specs_json.len(),
            "crate_spec": {
                "slug": crate_name,
                "title": crate_name,
                "component": component,
            },
            "module_specs": specs_json,
        }));
    }

    // ── Create root crate spec ─────────────────────────────────────────────
    let root_slug = crate_name.clone();
    let mut created_count: usize = 0;
    let mut skipped_count: usize = 0;

    let root_id = match store.resolve_id(&root_slug) {
        Ok(existing) => {
            skipped_count += 1;
            existing
        },
        Err(_) => {
            let mut manifest =
                SpecManifest::new(&root_slug, &crate_name, &component);
            manifest.set_scope("public");
            let body = format!(
                "# {crate_name}\n\nBootstrapped from source analysis.\n\n\
                 See child specs for individual module documentation.\n"
            );
            let id = store.create(&manifest, &body, None)?;
            created_count += 1;
            id
        },
    };

    // ── Create one spec per module file ───────────────────────────────────
    for ms in &module_specs {
        if store.resolve_id(&ms.slug).is_ok() {
            skipped_count += 1;
            continue;
        }

        // Resolve parent: root spec or a parent module spec
        let parent_id = store.resolve_id(&ms.parent_slug).ok();

        let mut manifest = SpecManifest::new(&ms.slug, &ms.title, &component);
        manifest.set_scope("public");
        if let Some(pid) = parent_id {
            manifest.set_parent(&pid.to_string());
        } else {
            manifest.set_parent(&root_id.to_string());
        }

        // Build CodeRefs for each public item in this file
        let code_refs: Vec<CodeRef> = ms
            .items
            .iter()
            .map(|item| CodeRef {
                file: ms.workspace_rel_file.clone(),
                symbol: item.name.clone(),
                kind: item.kind,
                line_start: item.line_start,
                line_end: item.line_end,
                description: if item.doc_comment.is_empty() {
                    None
                } else {
                    Some(item.doc_comment.clone())
                },
            })
            .collect();
        manifest.code_refs = code_refs;

        let body =
            build_module_body(&ms.title, &ms.workspace_rel_file, &ms.items);

        store.create(&manifest, &body, None)?;
        created_count += 1;
    }

    Ok(json!({
        "command": "bootstrap",
        "status": "ok",
        "crate": crate_name,
        "dry_run": false,
        "created": created_count,
        "skipped": skipped_count,
    }))
}

// ── Scanner ───────────────────────────────────────────────────────────────────

/// Walk `src/` and return one `ModuleSpec` per .rs file that has ≥1 public item.
/// Skips `lib.rs` and `main.rs` (covered by the root crate spec).
fn scan_src_dir(
    src_dir: &Path,
    _crate_path: &Path,
    workspace_root: &Path,
    crate_name: &str,
) -> Vec<ModuleSpec> {
    let mut specs = Vec::new();

    for entry in WalkDir::new(src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().map_or(false, |ext| ext == "rs"))
    {
        let abs_path = entry.path();

        // Skip the crate root files (root spec covers these)
        let file_name =
            abs_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if file_name == "main.rs" || file_name == "lib.rs" {
            continue;
        }

        // Workspace-relative path for CodeRefs
        let workspace_rel = abs_path
            .strip_prefix(workspace_root)
            .unwrap_or(abs_path)
            .to_string_lossy()
            .replace('\\', "/");

        // Module path relative to src/, e.g. "storage/store"
        let rel_to_src = abs_path.strip_prefix(src_dir).unwrap_or(abs_path);
        let module_path = rel_to_src
            .with_extension("")
            .to_string_lossy()
            .replace('\\', "/");

        // Normalize: "storage/mod" → "storage"
        let module_path = module_path.trim_end_matches("/mod").to_string();

        // Slug-safe module path: underscores → hyphens (slug rules: [a-z0-9-]).
        // The original `module_path` is preserved for the title/file display.
        let slug_module_path = module_path.replace('_', "-");

        // Slug: "ticket-api/storage/store"
        let slug = format!("{crate_name}/{slug_module_path}");

        // Title: last segment of module path (uses original, with underscores)
        let title = module_path
            .split('/')
            .last()
            .unwrap_or(&module_path)
            .to_string();

        // Parent slug: parent module or crate root
        let parent_slug = if slug_module_path.contains('/') {
            let parent = slug_module_path
                .rsplitn(2, '/')
                .nth(1)
                .unwrap_or(slug_module_path.as_str());
            format!("{crate_name}/{parent}")
        } else {
            crate_name.to_string()
        };

        // Parse the file
        let source = match std::fs::read_to_string(abs_path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let ast = match syn::parse_file(&source) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let items = extract_public_items(&ast);
        if items.is_empty() {
            continue;
        }

        specs.push(ModuleSpec {
            title,
            slug,
            workspace_rel_file: workspace_rel,
            parent_slug,
            items,
        });
    }

    // Sort by slug so parent specs are created before children
    specs.sort_by(|a, b| a.slug.cmp(&b.slug));
    specs
}

fn build_module_body(
    title: &str,
    file: &str,
    items: &[ExtractedItem],
) -> String {
    let mut body =
        format!("# {title}\n\nSource: `{file}`\n\n## Public API\n\n");
    for item in items {
        body.push_str(&format!("### `{}` ({:?})\n\n", item.name, item.kind));
        if !item.doc_comment.is_empty() {
            body.push_str(&item.doc_comment);
            body.push_str("\n\n");
        }
    }
    body
}

fn read_crate_name(crate_path: &Path) -> Option<String> {
    let cargo_toml = crate_path.join("Cargo.toml");
    let content = std::fs::read_to_string(&cargo_toml).ok()?;
    // Simple line-based extraction (no full TOML parse needed)
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("name") && trimmed.contains('=') {
            let val = trimmed.splitn(2, '=').nth(1)?.trim();
            let name = val.trim_matches('"').trim_matches('\'').to_string();
            if !name.is_empty() {
                return Some(name);
            }
        }
    }
    None
}
