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

use std::collections::{
    BTreeMap,
    HashMap,
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
pub const SPEC_INDEX_FILE_COMMENT: &str = "<!-- spec-index:file generated=true -->";

/// Per-entry provenance prefix (Q2.1). Each entry marker also carries a digest
/// prefix (Q4.1): `<!-- spec-index:entry id=<uuid> slug=<slug> digest=<hex12> -->`.
pub const SPEC_INDEX_ENTRY_PREFIX: &str = "spec-index:entry";

/// Provenance comment for the generated agent-hook file.
pub const SPEC_INDEX_AGENT_HOOK_COMMENT: &str =
    "<!-- spec-index:agent-hook generated=true -->";

/// Repository-relative path of the generated agent-hook file (D1).
pub const SPEC_INDEX_AGENT_HOOK_PATH: &str = ".agents/spec-catalog.md";

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
        .map(|s| (s.manifest.id, SpecDisplayExtra::from_manifest(s.manifest)))
        .collect();

    let mut entries: Vec<IndexEntry> = sources
        .iter()
        .map(|s| {
            make_entry(s, generated_at, &path_by_id, &children_by_parent)
        })
        .collect();
    for e in &mut entries {
        e.seal();
    }

    let mut sidecar = IndexSidecar::new(ContentKind::Spec, store_dir, entries);
    sidecar.generated_at = generated_at;
    sidecar.sort();

    let readme_markdown = render_catalog_markdown(&sidecar, &extras);
    let agent_hook_markdown = render_agent_hook(&sidecar, store_dir, &extras);

    SpecCatalogArtifacts {
        sidecar,
        readme_markdown,
        agent_hook_markdown,
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
}

impl SpecDisplayExtra {
    fn from_manifest(manifest: &SpecManifest) -> Self {
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
fn normalize_summary(body: &str) -> String {
    for raw in body.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("```") {
            continue;
        }
        let stripped = line.trim_start_matches(['-', '*', '>', ' ']).trim();
        if stripped.is_empty() {
            continue;
        }
        let collapsed = stripped.split_whitespace().collect::<Vec<_>>().join(" ");
        return truncate_chars(&collapsed, 200);
    }
    String::new()
}

/// Extract the body text under a `## <heading>` (or `# <heading>`) section,
/// normalized to a single line. Returns `None` when the heading is absent or
/// the section is empty.
fn extract_section(
    body: &str,
    heading: &str,
) -> Option<String> {
    let mut lines = body.lines();
    // Find the heading line (any level).
    let target = heading.to_lowercase();
    let mut in_section = false;
    let mut collected: Vec<String> = Vec::new();

    for raw in lines.by_ref() {
        let line = raw.trim();
        if let Some(rest) = line.strip_prefix('#') {
            let title = rest.trim_start_matches('#').trim().to_lowercase();
            if in_section {
                // Next heading ends the section.
                break;
            }
            if title == target {
                in_section = true;
            }
            continue;
        }
        if in_section {
            if line.is_empty() || line.starts_with("```") {
                if collected.is_empty() {
                    continue;
                }
                break;
            }
            let stripped = line.trim_start_matches(['-', '*', '>', ' ']).trim();
            if !stripped.is_empty() {
                collected.push(stripped.to_string());
            }
        }
    }

    if collected.is_empty() {
        return None;
    }
    let joined = collected.join(" ");
    let collapsed = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    Some(truncate_chars(&collapsed, 280))
}

fn truncate_chars(
    text: &str,
    max: usize,
) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }
    let mut out: String = text.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// Extract lower-cased keyword terms from the title and slug leaf.
fn keywords_for(
    title: &str,
    slug: &str,
) -> Vec<String> {
    let slug_leaf = slug.rsplit('/').next().unwrap_or(slug);
    let mut keywords: Vec<String> = title
        .split_whitespace()
        .chain(slug_leaf.split(['-', '_', '/']))
        .map(|w| {
            w.to_lowercase()
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_string()
        })
        .filter(|w| w.chars().count() > 3)
        .collect();
    keywords.sort_unstable();
    keywords.dedup();
    keywords
}

fn normalize_tags(tags: &mut Vec<String>) {
    for t in tags.iter_mut() {
        *t = t.to_lowercase();
    }
    tags.sort_unstable();
    tags.dedup();
}

fn first12(digest: &str) -> &str {
    let end = digest.len().min(12);
    &digest[..end]
}

/// Markdown group header key: the spec component, or `ungrouped`.
fn group_key(extra: Option<&SpecDisplayExtra>) -> String {
    extra
        .and_then(|e| e.component.clone())
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| "ungrouped".to_string())
}

/// Render `.spec/README.md`: a component-grouped catalog surfacing hierarchy.
fn render_catalog_markdown(
    sidecar: &IndexSidecar,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
) -> String {
    // slug lookup by id (for parent/child slug rendering).
    let slug_by_id: HashMap<Uuid, String> = extras
        .iter()
        .map(|(id, e)| (*id, e.slug.clone()))
        .collect();

    // Group entries by component, preserving id-sorted order within group.
    let mut groups: BTreeMap<String, Vec<&IndexEntry>> = BTreeMap::new();
    for entry in &sidecar.entries {
        let key = group_key(extras.get(&entry.id));
        groups.entry(key).or_default().push(entry);
    }

    let mut out = String::new();
    out.push_str(SPEC_INDEX_FILE_COMMENT);
    out.push('\n');

    for (group, group_entries) in &groups {
        out.push_str("\n## ");
        out.push_str(group);
        out.push('\n');
        for entry in group_entries {
            out.push('\n');
            out.push_str(&render_entry_block(
                entry,
                extras.get(&entry.id),
                &slug_by_id,
            ));
        }
    }

    out
}

fn render_entry_block(
    entry: &IndexEntry,
    extra: Option<&SpecDisplayExtra>,
    slug_by_id: &HashMap<Uuid, String>,
) -> String {
    let slug = extra.map(|e| e.slug.clone()).unwrap_or_default();
    let is_root = entry.tags.iter().any(|t| t == "root");

    let mut block = String::new();
    block.push_str(&format!(
        "<!-- {prefix} id={id} slug={slug} digest={digest} -->\n",
        prefix = SPEC_INDEX_ENTRY_PREFIX,
        id = entry.id,
        slug = slug,
        digest = first12(&entry.digest),
    ));

    block.push_str("### ");
    block.push_str(&entry.title);
    if is_root {
        block.push_str(" _(root)_");
    }
    block.push('\n');

    if !entry.summary.is_empty() {
        block.push('\n');
        block.push_str(&entry.summary);
        block.push('\n');
    }

    // Bullet metadata (Q2.2 skeleton: heading, summary, then bullets).
    block.push('\n');
    if !slug.is_empty() {
        block.push_str(&format!("- slug: `{slug}`\n"));
    }
    if let Some(visibility) = extra.and_then(|e| e.visibility.as_deref()) {
        block.push_str(&format!("- scope: {visibility}\n"));
    }
    // Hierarchy bullets — the headline feature.
    if let Some(parent_ref) = &entry.relations.parent {
        let parent_slug = slug_by_id
            .get(&parent_ref.entry_id)
            .filter(|s| !s.is_empty())
            .cloned()
            .unwrap_or_else(|| parent_ref.entry_id.to_string());
        block.push_str(&format!("- parent: `{parent_slug}`\n"));
    }
    if !entry.relations.children.is_empty() {
        let child_slugs: Vec<String> = entry
            .relations
            .children
            .iter()
            .map(|c| {
                slug_by_id
                    .get(&c.entry_id)
                    .filter(|s| !s.is_empty())
                    .cloned()
                    .unwrap_or_else(|| c.entry_id.to_string())
            })
            .collect();
        block.push_str(&format!(
            "- children ({}): {}\n",
            child_slugs.len(),
            child_slugs
                .iter()
                .map(|s| format!("`{s}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !entry.tags.is_empty() {
        block.push_str(&format!("- tags: {}\n", entry.tags.join(", ")));
    }
    block.push_str(&format!("- ref: `{}`\n", entry.source_path));

    block
}

/// Render the `.agents/spec-catalog.md` agent-hook pointer (D1).
fn render_agent_hook(
    sidecar: &IndexSidecar,
    store_dir: &str,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
) -> String {
    let total = sidecar.entries.len();
    let root_count = sidecar
        .entries
        .iter()
        .filter(|e| e.tags.iter().any(|t| t == "root"))
        .count();

    let mut groups: BTreeMap<String, ()> = BTreeMap::new();
    for entry in &sidecar.entries {
        groups.insert(group_key(extras.get(&entry.id)), ());
    }
    let group_list = groups.keys().cloned().collect::<Vec<_>>().join(", ");

    let mut out = String::new();
    out.push_str(SPEC_INDEX_AGENT_HOOK_COMMENT);
    out.push_str("\n\n# Spec Catalog\n\n");
    out.push_str(&format!(
        "The full specification catalog is generated at `{store_dir}/README.md`\n\
         (machine-readable sidecar with parent/child relations:\n\
         `{store_dir}/index.toon`). Browse it before scanning raw `{store_dir}/`\n\
         entry files.\n\n"
    ));
    out.push_str(&format!("- Total specs: {total}\n"));
    out.push_str(&format!("- Root specs (no parent): {root_count}\n"));
    if !group_list.is_empty() {
        out.push_str(&format!("- Components: {group_list}\n"));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::SpecManifest;

    fn spec(
        slug: &str,
        title: &str,
        component: &str,
    ) -> SpecManifest {
        let mut m = SpecManifest::new(slug, title, component);
        m.set_scope("internal");
        m
    }

    fn source<'a>(
        manifest: &'a SpecManifest,
        path: &str,
        body: &str,
    ) -> SpecCatalogSource<'a> {
        SpecCatalogSource {
            manifest,
            source_path: path.to_string(),
            body: body.to_string(),
        }
    }

    #[test]
    fn summary_takes_first_text_block() {
        assert_eq!(
            normalize_summary("# Heading\n\nThe contract goal.\n"),
            "The contract goal."
        );
        assert_eq!(normalize_summary("## Only a heading"), "");
    }

    #[test]
    fn extract_section_reads_named_section() {
        let body = "# Goal\n\nDo X.\n\n## Scope\n\nThe API surface.\n\n## Non-goals\n\nNot the UI.\n";
        assert_eq!(extract_section(body, "Scope").as_deref(), Some("The API surface."));
        assert_eq!(extract_section(body, "Non-goals").as_deref(), Some("Not the UI."));
        assert_eq!(extract_section(body, "Missing"), None);
    }

    #[test]
    fn hierarchy_relations_are_populated() {
        let parent = spec("root", "Root", "comp-a");
        let parent_id = parent.id.to_string();
        let mut child = spec("root/child", "Child", "comp-a");
        child.set_parent(&parent_id);

        let sources = vec![
            source(&parent, ".spec/specs/root/spec.toml", "Root body."),
            source(&child, ".spec/specs/child/spec.toml", "Child body."),
        ];

        let artifacts = generate_spec_catalog(&sources, ".spec");
        let by_id: std::collections::HashMap<_, _> = artifacts
            .sidecar
            .entries
            .iter()
            .map(|e| (e.id, e))
            .collect();

        let parent_entry = by_id[&parent.id];
        let child_entry = by_id[&child.id];

        // Parent has one child ref; child has a parent ref.
        assert_eq!(parent_entry.relations.children.len(), 1);
        assert_eq!(parent_entry.relations.children[0].entry_id, child.id);
        assert_eq!(parent_entry.relations.children[0].relation_kind, RelationKind::Child);
        assert!(parent_entry.tags.iter().any(|t| t == "root"));

        let parent_ref = child_entry.relations.parent.as_ref().unwrap();
        assert_eq!(parent_ref.entry_id, parent.id);
        assert_eq!(parent_ref.relation_kind, RelationKind::Parent);
        assert!(!child_entry.tags.iter().any(|t| t == "root"));
    }

    #[test]
    fn catalog_has_provenance_grouping_and_hierarchy_bullets() {
        let parent = spec("root", "Root", "comp-a");
        let parent_id = parent.id.to_string();
        let mut child = spec("root/child", "Child", "comp-a");
        child.set_parent(&parent_id);

        let sources = vec![
            source(&parent, ".spec/specs/root/spec.toml", "Root body."),
            source(&child, ".spec/specs/child/spec.toml", "Child body."),
        ];

        let artifacts = generate_spec_catalog(&sources, ".spec");
        let md = &artifacts.readme_markdown;
        assert!(md.starts_with(SPEC_INDEX_FILE_COMMENT));
        assert!(md.contains("## comp-a"));
        assert!(md.contains("<!-- spec-index:entry id="));
        assert!(md.contains("digest="));
        assert!(md.contains("_(root)_"));
        assert!(md.contains("- parent: `root`"));
        assert!(md.contains("- children (1): `root/child`"));
        for e in &artifacts.sidecar.entries {
            assert!(e.is_digest_valid());
        }
    }

    #[test]
    fn regeneration_is_byte_stable() {
        let parent = spec("root", "Root", "comp-a");
        let sources = vec![source(&parent, ".spec/specs/root/spec.toml", "Body.")];

        let a = generate_spec_catalog(&sources, ".spec");
        let b = generate_spec_catalog(&sources, ".spec");
        assert_eq!(a.readme_markdown, b.readme_markdown);
        assert_eq!(
            a.sidecar.encode_toon().unwrap(),
            b.sidecar.encode_toon().unwrap()
        );
        assert_eq!(a.agent_hook_markdown, b.agent_hook_markdown);
    }
}
