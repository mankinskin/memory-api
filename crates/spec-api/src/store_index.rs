//! Spec store hierarchy catalog generator (ticket `b9757ba7`).
//!
//! Reads spec manifests and produces the three committed catalog artifacts:
//!
//! - `.spec/README.md` — a human-browsable catalog grouped by component, where
//!   each entry surfaces its place in the parent/child hierarchy.
//! - `.spec/index.toon` — the machine-readable [`IndexSidecar`] (D8) whose
//!   entries carry typed parent/child [`IndexRef`]s (the headline hierarchy
//!   feature of this generator).
//! - `.agents/spec-catalog.md` — an agent-hook pointer at the catalog (D1).
//!
//! Per the `thin-generator-architecture` spec (Q1.1) this normalization lives in
//! the owning domain crate (`spec-api`), not in `memory-api`.
//!
//! # Determinism
//!
//! All artifacts are byte-stable when the underlying spec data is unchanged.
//! Generated artifacts carry a fixed epoch `generated_at` (never wall-clock or
//! source mtime) so a re-scan that merely touches `updated_at` does not cause
//! spurious drift; every entry is sealed with the digest contract; and the
//! markdown never embeds a timestamp. This lets the pre-commit drift check
//! (`--check`) compare rendered output against the working tree without churn.

use std::{
    collections::{
        BTreeMap,
        BTreeSet,
        HashMap,
    },
    path::Path,
};

use chrono::{
    DateTime,
    Utc,
};
use uuid::Uuid;

use memory_api::{
    ContentKind,
    IndexEntry,
    IndexRef,
    IndexRelations,
    IndexSidecar,
    RelationKind,
};

use crate::manifest::SpecManifest;

/// Provenance comment written at the top of `.spec/README.md`.
///
/// Uses an `-index` suffixed prefix so index/catalog files are never confused
/// with spec *content* files (which carry `spec-api:*` provenance) — decision
/// Q2.1 of the `rendering-pipeline-integration` spec.
pub const SPEC_INDEX_FILE_COMMENT: &str =
    "<!-- spec-index:file generated=true -->";

/// Per-entry provenance prefix (Q2.1). Each entry marker also carries a digest
/// prefix (Q4.1): `<!-- spec-index:entry id=<uuid> slug=<slug> digest=<hex12> -->`.
pub const SPEC_INDEX_ENTRY_PREFIX: &str = "spec-index:entry";

/// Provenance comment for the generated agent-hook file.
pub const SPEC_INDEX_AGENT_HOOK_COMMENT: &str =
    "<!-- spec-index:agent-hook generated=true -->";

/// Repository-relative path of the generated agent-hook file (D1).
pub const SPEC_INDEX_AGENT_HOOK_PATH: &str = ".agents/spec-catalog.md";

/// Root folder (under `.spec/`) that contains one markdown tree node per spec.
pub const SPEC_INDEX_TREE_DIR: &str = "tree";

/// Per-entry provenance comment written at the top of generated tree pages.
pub const SPEC_INDEX_TREE_ENTRY_COMMENT: &str =
    "<!-- spec-index:tree-entry generated=true -->";

/// One joined spec source: the manifest, its resolved path, and its raw body.
///
/// The generator is pure: callers (the `spec store-index` CLI) join the spec
/// manifest list with the on-disk paths + body content and pass the result
/// here. Parent/child topology is derived internally from `manifest.parent()`.
pub struct SpecCatalogSource<'a> {
    /// The spec manifest carrying slug, title, state, component, scope, parent.
    pub manifest: &'a SpecManifest,
    /// Workspace-relative path to the canonical `spec.toml` (`/` separators).
    pub source_path: String,
    /// Raw `body.md` content, used to extract a one-line summary and the
    /// `## Scope` / `## Non-goals` section bodies for the digest.
    pub body: String,
}

/// The generated spec catalog artifacts, ready for the caller to write or diff.
pub struct SpecCatalogArtifacts {
    /// Sidecar for `.spec/index.toon`. Entries are sealed and sorted by id.
    pub sidecar: IndexSidecar,
    /// Rendered `.spec/README.md` catalog (LF newlines, single trailing newline).
    pub readme_markdown: String,
    /// Rendered `.agents/spec-catalog.md` agent-hook content.
    pub agent_hook_markdown: String,
    /// Rendered per-entry markdown tree under `.spec/tree/**/README.md`.
    ///
    /// Keys are workspace-relative file paths with `/` separators.
    pub tree_markdown: BTreeMap<String, String>,
}

/// Fixed, reproducible generation timestamp embedded in every artifact.
fn epoch() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).expect("epoch is valid")
}

/// Generate the full spec hierarchy catalog from joined sources.
///
/// `store_dir` is the spec store folder relative to the workspace root
/// (normally `.spec`). Entries are produced one-per-spec, sealed, and sorted by
/// id; each entry carries typed parent/child [`IndexRef`]s derived from the
/// `parent` pointers across the whole source set.
pub fn generate_spec_catalog(
    sources: &[SpecCatalogSource<'_>],
    store_dir: &str,
) -> SpecCatalogArtifacts {
    let generated_at = epoch();

    // id → canonical source path (for resolving parent/child refs).
    let path_by_id: HashMap<Uuid, &str> = sources
        .iter()
        .map(|s| (s.manifest.id, s.source_path.as_str()))
        .collect();

    // parent id → direct child ids (sorted for determinism).
    let mut children_by_parent: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for s in sources {
        if let Some(parent_id) = parent_uuid(s.manifest) {
            children_by_parent
                .entry(parent_id)
                .or_default()
                .push(s.manifest.id);
        }
    }
    for ids in children_by_parent.values_mut() {
        ids.sort_unstable();
    }

    // Per-spec display extras not carried by the digest schema.
    let extras: BTreeMap<Uuid, SpecDisplayExtra> = sources
        .iter()
        .map(|s| (s.manifest.id, SpecDisplayExtra::from_source(s)))
        .collect();

    let mut entries: Vec<IndexEntry> = sources
        .iter()
        .map(|s| make_entry(s, generated_at, &path_by_id, &children_by_parent))
        .collect();
    for e in &mut entries {
        e.seal();
    }

    let mut sidecar = IndexSidecar::new(ContentKind::Spec, store_dir, entries);
    sidecar.generated_at = generated_at;
    sidecar.sort();

    let tree_paths = build_tree_paths(&sidecar, &extras, store_dir);
    let tree_markdown = render_tree_markdown(&sidecar, &tree_paths, &extras);
    let readme_markdown =
        render_catalog_markdown(&sidecar, &tree_paths, &extras);
    let agent_hook_markdown = render_agent_hook(&sidecar, store_dir, &extras);

    SpecCatalogArtifacts {
        sidecar,
        readme_markdown,
        agent_hook_markdown,
        tree_markdown,
    }
}

/// Per-spec display data surfaced in the catalog markdown but excluded from the
/// digest schema (component/scope visibility are filtering metadata, not part
/// of the entry identity which is captured by tags/keywords).
#[derive(Default)]
struct SpecDisplayExtra {
    slug: String,
    component: Option<String>,
    /// Visibility scope of the spec (e.g. `internal`, `public`).
    visibility: Option<String>,
    acceptance_criteria: Option<String>,
}

impl SpecDisplayExtra {
    fn from_source(source: &SpecCatalogSource<'_>) -> Self {
        let manifest = source.manifest;
        Self {
            slug: manifest.slug().unwrap_or_default().to_string(),
            component: manifest
                .component()
                .map(str::to_string)
                .filter(|s| !s.is_empty()),
            visibility: manifest
                .scope()
                .map(str::to_string)
                .filter(|s| !s.is_empty()),
            acceptance_criteria: extract_section(
                &source.body,
                "Acceptance Criteria",
            )
            .or_else(|| extract_section(&source.body, "Acceptance criteria")),
        }
    }
}

/// Parse a manifest's `parent()` accessor into a UUID, if present and valid.
fn parent_uuid(manifest: &SpecManifest) -> Option<Uuid> {
    manifest
        .parent()
        .filter(|p| !p.is_empty())
        .and_then(|p| Uuid::parse_str(p).ok())
}

fn make_entry(
    source: &SpecCatalogSource<'_>,
    generated_at: DateTime<Utc>,
    path_by_id: &HashMap<Uuid, &str>,
    children_by_parent: &HashMap<Uuid, Vec<Uuid>>,
) -> IndexEntry {
    let manifest = source.manifest;
    let id = manifest.id;
    let slug = manifest.slug().unwrap_or_default().to_string();
    let title = manifest
        .title()
        .map(str::to_string)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            if slug.is_empty() {
                id.to_string()
            } else {
                slug.clone()
            }
        });
    let summary = normalize_summary(&source.body);
    let state = manifest.state().unwrap_or_default().to_string();

    // Tags: component, state, visibility scope, and a `root` marker.
    let mut tags = Vec::new();
    if let Some(component) = manifest.component().filter(|c| !c.is_empty()) {
        tags.push(component.to_string());
    }
    if !state.is_empty() {
        tags.push(state.clone());
    }
    if let Some(scope) = manifest.scope().filter(|s| !s.is_empty()) {
        tags.push(format!("scope:{scope}"));
    }
    if parent_uuid(manifest).is_none() {
        tags.push("root".to_string());
    }
    normalize_tags(&mut tags);

    let keywords = keywords_for(&title, &slug);

    // Hierarchy relations (the headline feature). Parent and children are typed
    // IndexRefs; relations are excluded from the digest, so they never affect
    // stability.
    let mut relations = IndexRelations::default();
    if let Some(parent_id) = parent_uuid(manifest) {
        if let Some(parent_path) = path_by_id.get(&parent_id) {
            relations.parent = Some(IndexRef {
                canonical_path: (*parent_path).to_string(),
                entry_id: parent_id,
                relation_kind: RelationKind::Parent,
                content_kind: ContentKind::Spec,
                digest: String::new(),
                anchor: None,
            });
        }
    }
    if let Some(child_ids) = children_by_parent.get(&id) {
        for child_id in child_ids {
            if let Some(child_path) = path_by_id.get(child_id) {
                relations.children.push(IndexRef {
                    canonical_path: (*child_path).to_string(),
                    entry_id: *child_id,
                    relation_kind: RelationKind::Child,
                    content_kind: ContentKind::Spec,
                    digest: String::new(),
                    anchor: None,
                });
            }
        }
    }

    // Scope / non-goals extracted from the spec body section bodies (enriches
    // the digest and the Tier-2 LOD view; `None` when the heading is absent).
    let scope = extract_section(&source.body, "Scope");
    let non_goals = extract_section(&source.body, "Non-goals")
        .or_else(|| extract_section(&source.body, "Non-Goals"))
        .or_else(|| extract_section(&source.body, "Non Goals"));

    IndexEntry {
        id,
        kind: ContentKind::Spec,
        source_path: source.source_path.clone(),
        title,
        summary,
        keywords,
        scope,
        non_goals,
        relations,
        digest: String::new(),
        tags,
        generated_at,
        source_modified_at: None,
    }
}

/// Collapse a spec body into a single normalized summary line.
///
/// Takes the first non-empty, non-heading, non-fence text block, strips leading
/// markdown markers, collapses internal whitespace, and truncates to 200 chars.

#[path = "store_index_render.rs"]
mod store_index_render;
use store_index_render::*;

#[cfg(test)]
#[path = "store_index_tests.rs"]
mod tests;
