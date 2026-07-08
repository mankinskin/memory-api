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
    BTreeSet,
    HashMap,
};
use std::path::Path;

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
    let readme_markdown = render_catalog_markdown(&sidecar, &tree_paths, &extras);
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
        let collapsed =
            stripped.split_whitespace().collect::<Vec<_>>().join(" ");
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

/// Render `.spec/README.md` as a table of contents linking into the tree pages.
fn render_catalog_markdown(
    sidecar: &IndexSidecar,
    tree_paths: &HashMap<Uuid, String>,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
) -> String {
    let by_id: HashMap<Uuid, &IndexEntry> =
        sidecar.entries.iter().map(|e| (e.id, e)).collect();
    let children_by_parent = children_from_sidecar(sidecar, extras);
    let roots_by_group = roots_by_group(sidecar, extras);

    let mut out = String::new();
    out.push_str(SPEC_INDEX_FILE_COMMENT);
    out.push('\n');

    for (group, root_ids) in roots_by_group {
        out.push_str("\n## ");
        out.push_str(&group);
        out.push('\n');

        render_readme_tree_lines(
            &mut out,
            &root_ids,
            0,
            &by_id,
            &children_by_parent,
            tree_paths,
            extras,
        );
    }

    out
}

fn render_readme_tree_lines(
    out: &mut String,
    ids: &[Uuid],
    depth: usize,
    by_id: &HashMap<Uuid, &IndexEntry>,
    children_by_parent: &HashMap<Uuid, Vec<Uuid>>,
    tree_paths: &HashMap<Uuid, String>,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
) {
    for id in ids {
        let Some(entry) = by_id.get(id) else {
            continue;
        };
        let slug = extras
            .get(id)
            .map(|e| e.slug.clone())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| entry.title.clone());
        let path = tree_paths
            .get(id)
            .cloned()
            .unwrap_or_else(|| format!(".spec/{SPEC_INDEX_TREE_DIR}/README.md"));
        let rel = rel_from_readme(&path);
        out.push_str(&format!(
            "{}- [{}]({})\n",
            "  ".repeat(depth),
            slug,
            rel
        ));

        if let Some(children) = children_by_parent.get(id) {
            render_readme_tree_lines(
                out,
                children,
                depth + 1,
                by_id,
                children_by_parent,
                tree_paths,
                extras,
            );
        }
    }
}

fn rel_from_readme(path: &str) -> String {
    if let Some(stripped) = path.strip_prefix(".spec/") {
        format!("./{stripped}")
    } else {
        format!("./{path}")
    }
}

fn build_tree_paths(
    sidecar: &IndexSidecar,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
    store_dir: &str,
) -> HashMap<Uuid, String> {
    let children_by_parent = children_from_sidecar(sidecar, extras);
    let by_id: HashMap<Uuid, &IndexEntry> =
        sidecar.entries.iter().map(|e| (e.id, e)).collect();

    let mut roots: Vec<Uuid> = sidecar
        .entries
        .iter()
        .filter(|e| {
            e.relations
                .parent
                .as_ref()
                .map(|p| !by_id.contains_key(&p.entry_id))
                .unwrap_or(true)
        })
        .map(|e| e.id)
        .collect();
    sort_ids_by_slug_then_id(&mut roots, extras);

    let mut assigned = HashMap::new();
    let mut visited = BTreeSet::new();

    for root_id in &roots {
        assign_tree_path(
            *root_id,
            None,
            store_dir,
            &children_by_parent,
            extras,
            &mut assigned,
            &mut visited,
        );
    }

    // Cycles or orphaned nodes still get deterministic paths.
    let mut leftovers: Vec<Uuid> = sidecar
        .entries
        .iter()
        .map(|e| e.id)
        .filter(|id| !assigned.contains_key(id))
        .collect();
    sort_ids_by_slug_then_id(&mut leftovers, extras);
    for id in leftovers {
        assign_tree_path(
            id,
            None,
            store_dir,
            &children_by_parent,
            extras,
            &mut assigned,
            &mut visited,
        );
    }

    assigned
}

fn assign_tree_path(
    id: Uuid,
    parent_dir: Option<&str>,
    store_dir: &str,
    children_by_parent: &HashMap<Uuid, Vec<Uuid>>,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
    assigned: &mut HashMap<Uuid, String>,
    visiting: &mut BTreeSet<Uuid>,
) {
    if assigned.contains_key(&id) || visiting.contains(&id) {
        return;
    }
    visiting.insert(id);

    let segments = entry_dir_segments(id, extras);
    let base_dir = match parent_dir {
        Some(parent) => join_slash(parent, &segments.join("/")),
        None => join_slash(
            &join_slash(store_dir, SPEC_INDEX_TREE_DIR),
            &segments.join("/"),
        ),
    };
    let readme_path = join_slash(&base_dir, "README.md");
    assigned.insert(id, readme_path);

    if let Some(children) = children_by_parent.get(&id) {
        for child_id in children {
            assign_tree_path(
                *child_id,
                Some(&base_dir),
                store_dir,
                children_by_parent,
                extras,
                assigned,
                visiting,
            );
        }
    }
    visiting.remove(&id);
}

fn entry_dir_segments(
    id: Uuid,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
) -> [String; 2] {
    let leaf = extras
        .get(&id)
        .map(|e| e.slug.rsplit('/').next().unwrap_or(""))
        .unwrap_or("");
    let leaf = sanitize_path_segment(leaf);
    let short_id = id.to_string().chars().take(8).collect::<String>();
    [
        if leaf.is_empty() {
            "entry".to_string()
        } else {
            leaf
        },
        short_id,
    ]
}

fn sanitize_path_segment(raw: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in raw.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    out.trim_matches('-').to_string()
}

fn join_slash(
    base: &str,
    next: &str,
) -> String {
    let base = base.trim_end_matches('/');
    let next = next.trim_start_matches('/');
    if base.is_empty() {
        next.to_string()
    } else if next.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{next}")
    }
}

fn children_from_sidecar(
    sidecar: &IndexSidecar,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
) -> HashMap<Uuid, Vec<Uuid>> {
    let mut map: HashMap<Uuid, Vec<Uuid>> = HashMap::new();
    for entry in &sidecar.entries {
        let mut children: Vec<Uuid> =
            entry.relations.children.iter().map(|c| c.entry_id).collect();
        sort_ids_by_slug_then_id(&mut children, extras);
        map.insert(entry.id, children);
    }
    map
}

fn roots_by_group(
    sidecar: &IndexSidecar,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
) -> BTreeMap<String, Vec<Uuid>> {
    let ids: BTreeSet<Uuid> = sidecar.entries.iter().map(|e| e.id).collect();
    let mut groups: BTreeMap<String, Vec<Uuid>> = BTreeMap::new();
    for entry in &sidecar.entries {
        let is_root = entry
            .relations
            .parent
            .as_ref()
            .map(|p| !ids.contains(&p.entry_id))
            .unwrap_or(true);
        if is_root {
            let key = group_key(extras.get(&entry.id));
            groups.entry(key).or_default().push(entry.id);
        }
    }
    for root_ids in groups.values_mut() {
        sort_ids_by_slug_then_id(root_ids, extras);
    }
    groups
}

fn sort_ids_by_slug_then_id(
    ids: &mut Vec<Uuid>,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
) {
    ids.sort_by(|a, b| {
        let sa = extras.get(a).map(|e| e.slug.as_str()).unwrap_or("");
        let sb = extras.get(b).map(|e| e.slug.as_str()).unwrap_or("");
        sa.cmp(sb).then_with(|| a.cmp(b))
    });
}

fn render_tree_markdown(
    sidecar: &IndexSidecar,
    tree_paths: &HashMap<Uuid, String>,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
) -> BTreeMap<String, String> {
    let by_id: HashMap<Uuid, &IndexEntry> =
        sidecar.entries.iter().map(|e| (e.id, e)).collect();
    let children_by_parent = children_from_sidecar(sidecar, extras);

    let mut files = BTreeMap::new();
    for entry in &sidecar.entries {
        let Some(path) = tree_paths.get(&entry.id) else {
            continue;
        };
        let content = render_tree_entry_page(
            entry,
            path,
            &by_id,
            &children_by_parent,
            tree_paths,
            extras,
        );
        files.insert(path.clone(), content);
    }
    files
}

fn render_tree_entry_page(
    entry: &IndexEntry,
    current_path: &str,
    by_id: &HashMap<Uuid, &IndexEntry>,
    children_by_parent: &HashMap<Uuid, Vec<Uuid>>,
    tree_paths: &HashMap<Uuid, String>,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
) -> String {
    let current_dir = Path::new(current_path)
        .parent()
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();

    let parent_and_siblings = tree_parent_and_siblings(entry, by_id, children_by_parent);

    let mut out = String::new();
    render_tree_entry_front_matter(&mut out, entry, extras);
    render_tree_entry_sections(&mut out, entry, extras);
    render_tree_navigation(
        &mut out,
        entry,
        &current_dir,
        parent_and_siblings,
        children_by_parent,
        tree_paths,
        extras,
    );
    out
}

fn tree_parent_and_siblings(
    entry: &IndexEntry,
    by_id: &HashMap<Uuid, &IndexEntry>,
    children_by_parent: &HashMap<Uuid, Vec<Uuid>>,
) -> Option<(Uuid, Vec<Uuid>)> {
    let parent_ref = entry.relations.parent.as_ref()?;
    let parent_entry = by_id.get(&parent_ref.entry_id)?;
    let siblings = children_by_parent
        .get(&parent_entry.id)
        .cloned()
        .unwrap_or_default();
    Some((parent_entry.id, siblings))
}

fn render_tree_entry_front_matter(
    out: &mut String,
    entry: &IndexEntry,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
) {
    let slug = extras
        .get(&entry.id)
        .map(|e| e.slug.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| entry.id.to_string());
    let component = extras
        .get(&entry.id)
        .and_then(|e| e.component.clone())
        .unwrap_or_else(|| "ungrouped".to_string());
    let visibility = extras
        .get(&entry.id)
        .and_then(|e| e.visibility.clone());
    let state = entry
        .tags
        .iter()
        .find(|t| t.as_str() != "root" && !t.starts_with("scope:"))
        .cloned()
        .unwrap_or_default();

    out.push_str(SPEC_INDEX_TREE_ENTRY_COMMENT);
    out.push('\n');
    out.push_str(&format!(
        "<!-- {} id={} slug={} digest={} -->\n\n",
        SPEC_INDEX_ENTRY_PREFIX,
        entry.id,
        slug,
        first12(&entry.digest),
    ));
    out.push_str(&format!("# {}\n\n", entry.title));
    out.push_str(&format!("- slug: `{slug}`\n"));
    out.push_str(&format!("- component: {component}\n"));
    if let Some(scope) = visibility {
        out.push_str(&format!("- scope: {scope}\n"));
    }
    if !state.is_empty() {
        out.push_str(&format!("- state: {state}\n"));
    }
    out.push_str(&format!("- index_ref: `{}`\n\n", entry.source_path));
}

fn render_tree_entry_sections(
    out: &mut String,
    entry: &IndexEntry,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
) {
    if !entry.summary.is_empty() {
        out.push_str("## Summary\n\n");
        out.push_str(&entry.summary);
        out.push_str("\n\n");
    }

    if let Some(acceptance) = extras
        .get(&entry.id)
        .and_then(|e| e.acceptance_criteria.as_deref())
    {
        out.push_str("## Acceptance Criteria Excerpt\n\n");
        out.push_str(acceptance);
        out.push_str("\n\n");
    }
}

fn render_tree_navigation(
    out: &mut String,
    entry: &IndexEntry,
    current_dir: &str,
    parent_and_siblings: Option<(Uuid, Vec<Uuid>)>,
    children_by_parent: &HashMap<Uuid, Vec<Uuid>>,
    tree_paths: &HashMap<Uuid, String>,
    extras: &BTreeMap<Uuid, SpecDisplayExtra>,
) {
    out.push_str("## Navigation\n\n");
    if let Some((parent_id, siblings)) = parent_and_siblings {
        if let Some(parent_path) = tree_paths.get(&parent_id) {
            let rel = relative_link(current_dir, parent_path);
            let parent_slug = extras
                .get(&parent_id)
                .map(|e| e.slug.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| parent_id.to_string());
            out.push_str(&format!("- Parent: [{parent_slug}]({rel})\n"));
        }

        let mut sibling_lines = Vec::new();
        for sibling_id in siblings {
            if sibling_id == entry.id {
                continue;
            }
            let Some(path) = tree_paths.get(&sibling_id) else {
                continue;
            };
            let rel = relative_link(current_dir, path);
            let label = extras
                .get(&sibling_id)
                .map(|e| e.slug.clone())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| sibling_id.to_string());
            sibling_lines.push(format!("[{label}]({rel})"));
        }
        if !sibling_lines.is_empty() {
            out.push_str(&format!("- Siblings: {}\n", sibling_lines.join(", ")));
        }
    } else {
        out.push_str("- Parent: _(root)_\n");
    }

    if let Some(children) = children_by_parent.get(&entry.id) {
        if children.is_empty() {
            out.push_str("- Children: _(none)_\n");
        } else {
            let links = children
                .iter()
                .filter_map(|child_id| {
                    let path = tree_paths.get(child_id)?;
                    let rel = relative_link(current_dir, path);
                    let label = extras
                        .get(child_id)
                        .map(|e| e.slug.clone())
                        .filter(|s| !s.is_empty())
                        .unwrap_or_else(|| child_id.to_string());
                    Some(format!("[{label}]({rel})"))
                })
                .collect::<Vec<_>>();
            out.push_str(&format!("- Children: {}\n", links.join(", ")));
        }
    } else {
        out.push_str("- Children: _(none)_\n");
    }
}

fn relative_link(
    from_dir: &str,
    to_path: &str,
) -> String {
    let from_parts: Vec<&str> = from_dir.split('/').filter(|s| !s.is_empty()).collect();
    let to_parts: Vec<&str> = to_path.split('/').filter(|s| !s.is_empty()).collect();

    let mut common = 0usize;
    while common < from_parts.len()
        && common < to_parts.len()
        && from_parts[common] == to_parts[common]
    {
        common += 1;
    }

    let mut rel = Vec::new();
    for _ in common..from_parts.len() {
        rel.push("..");
    }
    for part in &to_parts[common..] {
        rel.push(part);
    }

    if rel.is_empty() {
        "./README.md".to_string()
    } else {
        rel.join("/")
    }
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
         `{store_dir}/index.toon`).\n\n"
    ));
    out.push_str(&format!("- Total specs: {total}\n"));
    out.push_str(&format!("- Root specs (no parent): {root_count}\n"));
    if !group_list.is_empty() {
        out.push_str(&format!("- Components: {group_list}\n"));
    }

    out
}


#[cfg(test)]
#[path = "store_index_tests.rs"]
mod tests;
