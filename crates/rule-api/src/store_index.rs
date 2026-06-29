//! Rule store catalog generator (ticket `9336a096`).
//!
//! Reads rule manifests and produces the three committed catalog artifacts:
//!
//! - `.rule/README.md` — a human-browsable catalog grouped by slug prefix (D4).
//! - `.rule/index.toon` — the machine-readable [`IndexSidecar`] (D8).
//! - `.agents/rules-catalog.md` — an agent-hook pointer at the catalog (D1).
//!
//! Per the `thin-generator-architecture` spec (Q1.1) this normalization lives in
//! the owning domain crate (`rule-api`), not in `memory-api`.
//!
//! # Determinism
//!
//! All artifacts are byte-stable when the underlying rule data is unchanged.
//! Generated artifacts carry a fixed epoch `generated_at` (never wall-clock or
//! source mtime) so a re-scan that merely touches `updated_at` does not cause
//! spurious drift; every entry is sealed with the digest contract; and the
//! markdown never embeds a timestamp. This lets the pre-commit drift check
//! (`--check`) compare rendered output against the working tree without churn.

use std::collections::BTreeMap;

use chrono::{
    DateTime,
    Utc,
};

use memory_api::{
    ContentKind,
    IndexEntry,
    IndexRef,
    IndexRelations,
    IndexSidecar,
    RelationKind,
};

use crate::manifest::RuleManifest;

/// Fixed, reproducible generation timestamp embedded in every artifact.
///
/// Using a constant (rather than wall-clock or source mtime) keeps the rendered
/// `index.toon` byte-identical across runs whenever the rule content is
/// unchanged, even if a re-scan updates each entity's `updated_at`.
fn epoch() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).expect("epoch is valid")
}

/// Provenance comment written at the top of `.rule/README.md`.
///
/// Uses a `-catalog` suffixed prefix so index/catalog files are never confused
/// with rule *content* files (which carry `rule-api:*` provenance) — decision
/// Q2.1 of the `rendering-pipeline-integration` spec.
pub const RULE_CATALOG_FILE_COMMENT: &str =
    "<!-- rule-catalog:file generated=true -->";

/// Per-entry provenance prefix (Q2.1). Each entry marker also carries a digest
/// prefix (Q4.1): `<!-- rule-catalog:entry id=<uuid> slug=<slug> digest=<hex12> -->`.
pub const RULE_CATALOG_ENTRY_PREFIX: &str = "rule-catalog:entry";

/// Provenance comment for the generated agent-hook file.
pub const RULE_CATALOG_AGENT_HOOK_COMMENT: &str =
    "<!-- rule-catalog:agent-hook generated=true -->";

/// Repository-relative path of the generated agent-hook file (D1).
pub const RULE_CATALOG_AGENT_HOOK_PATH: &str = ".agents/rules-catalog.md";

/// One joined rule source: the manifest plus its resolved on-disk location.
///
/// The generator is pure: callers (the `rule store-index` CLI) join the rule
/// manifest list with the indexed entity paths and pass the result here.
pub struct RuleCatalogSource<'a> {
    /// The rule manifest carrying slug, title, body, tags, and feedback counts.
    pub manifest: &'a RuleManifest,
    /// Workspace-relative path to the canonical rule entry (`/` separators).
    pub source_path: String,
}

/// The generated rule catalog artifacts, ready for the caller to write or diff.
pub struct RuleCatalogArtifacts {
    /// Sidecar for `.rule/index.toon`. Entries are sealed and sorted by id.
    pub sidecar: IndexSidecar,
    /// Rendered `.rule/README.md` catalog (LF newlines, single trailing newline).
    pub readme_markdown: String,
    /// Rendered `.agents/rules-catalog.md` agent-hook content.
    pub agent_hook_markdown: String,
}

/// Generate the full rule catalog from joined sources.
///
/// `store_dir` is the rule store folder relative to the workspace root
/// (normally `.rule`). Entries are produced one-per-rule, sealed, and sorted by
/// id; the markdown groups them by slug prefix (D4).
pub fn generate_rule_catalog(
    sources: &[RuleCatalogSource<'_>],
    store_dir: &str,
) -> RuleCatalogArtifacts {
    let generated_at = epoch();

    let mut entries: Vec<IndexEntry> = sources
        .iter()
        .map(|s| make_entry(s, generated_at))
        .collect();
    for e in &mut entries {
        e.seal();
    }

    // Per-rule display extras (section + feedback) not carried by the digest
    // schema, keyed by rule id for lookup during markdown rendering.
    let extras: BTreeMap<uuid::Uuid, RuleDisplayExtra> = sources
        .iter()
        .map(|s| (s.manifest.id, RuleDisplayExtra::from_manifest(s.manifest)))
        .collect();

    let mut sidecar = IndexSidecar::new(ContentKind::Rule, store_dir, entries);
    sidecar.generated_at = generated_at;
    sidecar.sort();

    let readme_markdown = render_catalog_markdown(&sidecar, &extras);
    let agent_hook_markdown = render_agent_hook(&sidecar, store_dir);

    RuleCatalogArtifacts {
        sidecar,
        readme_markdown,
        agent_hook_markdown,
    }
}

/// Per-rule display data surfaced in the catalog markdown but intentionally
/// excluded from the digest schema (section is rule-specific; feedback counts
/// change without the rule content changing).
#[derive(Default)]
struct RuleDisplayExtra {
    section: Option<String>,
    helpful: i64,
    mixed: i64,
    not_helpful: i64,
}

impl RuleDisplayExtra {
    fn from_manifest(manifest: &RuleManifest) -> Self {
        Self {
            section: manifest
                .section()
                .map(str::to_string)
                .filter(|s| !s.is_empty()),
            helpful: manifest.feedback_helpful_count().unwrap_or(0),
            mixed: manifest.feedback_mixed_count().unwrap_or(0),
            not_helpful: manifest.feedback_not_helpful_count().unwrap_or(0),
        }
    }

    /// Compact feedback rating label, or `None` when there is no feedback.
    fn rating_label(&self) -> Option<String> {
        if self.helpful == 0 && self.mixed == 0 && self.not_helpful == 0 {
            return None;
        }
        Some(format!(
            "helpful {} / mixed {} / not-helpful {}",
            self.helpful, self.mixed, self.not_helpful
        ))
    }
}

fn make_entry(
    source: &RuleCatalogSource<'_>,
    generated_at: DateTime<Utc>,
) -> IndexEntry {
    let manifest = source.manifest;
    let slug = manifest.slug().unwrap_or_default().to_string();
    let title = manifest
        .title()
        .map(str::to_string)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| {
            if slug.is_empty() {
                manifest.id.to_string()
            } else {
                slug.clone()
            }
        });
    let summary = normalize_summary(manifest.body().unwrap_or_default());
    let state = manifest.state().unwrap_or_default().to_string();

    let mut tags = group_segments(&slug);
    if !state.is_empty() {
        tags.push(state);
    }
    if is_low_rated(manifest) {
        tags.push("low-rated".to_string());
    }
    normalize_tags(&mut tags);

    let keywords = keywords_for(&title, &slug);

    // A `related` ref back to the canonical rule entry (ticket requirement).
    // Relations are excluded from the digest, so this never affects stability.
    let mut relations = IndexRelations::default();
    relations.related.push(IndexRef {
        canonical_path: source.source_path.clone(),
        entry_id: manifest.id,
        relation_kind: RelationKind::Related,
        content_kind: ContentKind::Rule,
        digest: String::new(),
        anchor: (!slug.is_empty()).then(|| slug.clone()),
    });

    IndexEntry {
        id: manifest.id,
        kind: ContentKind::Rule,
        source_path: source.source_path.clone(),
        title,
        summary,
        keywords,
        scope: None,
        non_goals: None,
        relations,
        digest: String::new(),
        tags,
        generated_at,
        source_modified_at: None,
    }
}

/// Group key + tag segments derived from a slug.
///
/// `shared/agent-rules/operating-principles/l5` → `["shared", "agent-rules", "operating-principles"]`
/// (every segment except the leaf). Used as filtering tags; the markdown group
/// header uses [`group_key`].
fn group_segments(slug: &str) -> Vec<String> {
    let segments: Vec<&str> =
        slug.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() <= 1 {
        return Vec::new();
    }
    segments[..segments.len() - 1]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Markdown group header key: the first two slug segments (or fewer).
///
/// `shared/agent-rules/operating-principles/l5` → `shared/agent-rules`.
/// A slug with no `/` groups under `ungrouped`.
fn group_key(slug: &str) -> String {
    let segments: Vec<&str> =
        slug.split('/').filter(|s| !s.is_empty()).collect();
    match segments.len() {
        0 => "ungrouped".to_string(),
        1 => segments[0].to_string(),
        _ => segments[..2].join("/"),
    }
}

/// Collapse a rule body into a single normalized summary line.
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

/// A rule is low-rated when negative feedback meets or exceeds positive
/// feedback and at least one "not helpful" rating exists.
fn is_low_rated(manifest: &RuleManifest) -> bool {
    let not_helpful = manifest.feedback_not_helpful_count().unwrap_or(0);
    let helpful = manifest.feedback_helpful_count().unwrap_or(0);
    not_helpful > 0 && not_helpful >= helpful
}

fn first12(digest: &str) -> &str {
    let end = digest.len().min(12);
    &digest[..end]
}

/// Render `.rule/README.md`: a slug-prefix-grouped catalog.
fn render_catalog_markdown(
    sidecar: &IndexSidecar,
    extras: &BTreeMap<uuid::Uuid, RuleDisplayExtra>,
) -> String {
    // Group entries by slug-prefix key, preserving id-sorted order within group.
    let mut groups: BTreeMap<String, Vec<&IndexEntry>> = BTreeMap::new();
    for entry in &sidecar.entries {
        let slug = entry_slug(entry);
        groups.entry(group_key(&slug)).or_default().push(entry);
    }

    let mut out = String::new();
    out.push_str(RULE_CATALOG_FILE_COMMENT);
    out.push('\n');

    for (group, group_entries) in &groups {
        out.push_str("\n## ");
        out.push_str(group);
        out.push('\n');
        for entry in group_entries {
            out.push('\n');
            let extra = extras.get(&entry.id);
            out.push_str(&render_entry_block(entry, extra));
        }
    }

    out
}

fn render_entry_block(
    entry: &IndexEntry,
    extra: Option<&RuleDisplayExtra>,
) -> String {
    let slug = entry_slug(entry);
    let low_rated = entry.tags.iter().any(|t| t == "low-rated");

    let mut block = String::new();
    block.push_str(&format!(
        "<!-- {prefix} id={id} slug={slug} digest={digest} -->\n",
        prefix = RULE_CATALOG_ENTRY_PREFIX,
        id = entry.id,
        slug = slug,
        digest = first12(&entry.digest),
    ));

    block.push_str("### ");
    block.push_str(&entry.title);
    if low_rated {
        block.push_str(" **[low-rated]**");
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
    if let Some(section) = extra.and_then(|e| e.section.as_deref()) {
        block.push_str(&format!("- section: `{section}`\n"));
    }
    if !entry.tags.is_empty() {
        block.push_str(&format!("- tags: {}\n", entry.tags.join(", ")));
    }
    if let Some(rating) = extra.and_then(RuleDisplayExtra::rating_label) {
        block.push_str(&format!("- feedback: {rating}\n"));
    }
    block.push_str(&format!("- ref: `{}`\n", entry.source_path));

    block
}

/// Slug recovered from an entry's `related` ref anchor (set in [`make_entry`]).
fn entry_slug(entry: &IndexEntry) -> String {
    entry
        .relations
        .related
        .first()
        .and_then(|r| r.anchor.clone())
        .unwrap_or_default()
}

/// Render the `.agents/rules-catalog.md` agent-hook pointer (D1).
fn render_agent_hook(
    sidecar: &IndexSidecar,
    store_dir: &str,
) -> String {
    let total = sidecar.entries.len();
    let low_rated = sidecar
        .entries
        .iter()
        .filter(|e| e.tags.iter().any(|t| t == "low-rated"))
        .count();

    let mut groups: BTreeMap<String, ()> = BTreeMap::new();
    for entry in &sidecar.entries {
        groups.insert(group_key(&entry_slug(entry)), ());
    }
    let group_list = groups.keys().cloned().collect::<Vec<_>>().join(", ");

    let mut out = String::new();
    out.push_str(RULE_CATALOG_AGENT_HOOK_COMMENT);
    out.push_str("\n\n# Rules Catalog\n\n");
    out.push_str(&format!(
        "The full guidance-rules catalog is generated at `{store_dir}/README.md`\n\
         (machine-readable sidecar: `{store_dir}/index.toon`\n\n"
    ));
    out.push_str(&format!("- Total rules: {total}\n"));
    if !group_list.is_empty() {
        out.push_str(&format!("- Groups: {group_list}\n"));
    }
    out.push_str(&format!(
        "- Low-rated rules needing attention: {low_rated}\n"
    ));

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::RuleManifest;

    fn rule(
        slug: &str,
        title: &str,
        body: &str,
    ) -> RuleManifest {
        RuleManifest::new(slug, title, "AGENTS", "section", body)
    }

    fn source<'a>(
        manifest: &'a RuleManifest,
        path: &str,
    ) -> RuleCatalogSource<'a> {
        RuleCatalogSource {
            manifest,
            source_path: path.to_string(),
        }
    }

    #[test]
    fn groups_by_slug_prefix() {
        assert_eq!(
            group_key("shared/agent-rules/operating/l5"),
            "shared/agent-rules"
        );
        assert_eq!(group_key("solo"), "solo");
        assert_eq!(group_key(""), "ungrouped");
        assert_eq!(
            group_segments("shared/agent-rules/operating/l5"),
            vec!["shared", "agent-rules", "operating"]
        );
    }

    #[test]
    fn summary_takes_first_text_block() {
        assert_eq!(
            normalize_summary("# Heading\n\n- Do the thing carefully.\n"),
            "Do the thing carefully."
        );
        assert_eq!(normalize_summary("## Only a heading"), "");
    }

    #[test]
    fn catalog_has_provenance_and_grouping() {
        let r1 = rule("shared/agent-rules/opening", "Opening", "Start here.");
        let r2 = rule("shared/agent-rules/closing", "Closing", "Finish here.");
        let sources = vec![
            source(&r1, ".rule/entries/r1/rule.toml"),
            source(&r2, ".rule/entries/r2/rule.toml"),
        ];

        let artifacts = generate_rule_catalog(&sources, ".rule");
        assert!(
            artifacts
                .readme_markdown
                .starts_with(RULE_CATALOG_FILE_COMMENT)
        );
        assert!(artifacts.readme_markdown.contains("## shared/agent-rules"));
        assert!(
            artifacts
                .readme_markdown
                .contains("<!-- rule-catalog:entry id=")
        );
        assert!(artifacts.readme_markdown.contains("digest="));
        // Every entry sealed and digest-valid.
        for e in &artifacts.sidecar.entries {
            assert!(e.is_digest_valid());
        }
    }

    #[test]
    fn regeneration_is_byte_stable() {
        let r1 = rule("shared/a/one", "One", "First rule.");
        let sources = vec![source(&r1, ".rule/entries/r1/rule.toml")];

        let a = generate_rule_catalog(&sources, ".rule");
        let b = generate_rule_catalog(&sources, ".rule");
        assert_eq!(a.readme_markdown, b.readme_markdown);
        assert_eq!(
            a.sidecar.encode_toon().unwrap(),
            b.sidecar.encode_toon().unwrap()
        );
        assert_eq!(a.agent_hook_markdown, b.agent_hook_markdown);
    }

    #[test]
    fn low_rated_rule_is_badged() {
        let mut r = rule("shared/a/bad", "Bad Rule", "Avoid this.");
        r.extra.insert(
            "feedback_not_helpful_count".to_string(),
            serde_json::Value::Number(3.into()),
        );
        r.extra.insert(
            "feedback_helpful_count".to_string(),
            serde_json::Value::Number(0.into()),
        );
        let sources = vec![source(&r, ".rule/entries/r/rule.toml")];

        let artifacts = generate_rule_catalog(&sources, ".rule");
        assert!(artifacts.readme_markdown.contains("**[low-rated]**"));
        assert!(artifacts.readme_markdown.contains("- section: `section`"));
        assert!(
            artifacts
                .readme_markdown
                .contains("- feedback: helpful 0 / mixed 0 / not-helpful 3")
        );
        assert!(
            artifacts.sidecar.entries[0]
                .tags
                .iter()
                .any(|t| t == "low-rated")
        );
        assert!(
            artifacts
                .agent_hook_markdown
                .contains("Low-rated rules needing attention: 1")
        );
    }
}
