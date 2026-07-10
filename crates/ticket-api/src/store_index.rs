//! Ticket store catalog generator (ticket `c5e9bb39`).
//!
//! Reads ticket sources and produces the three committed catalog artifacts:
//!
//! - `.ticket/README.md` - a human-browsable catalog grouped by state/component.
//! - `.ticket/index.toon` - the machine-readable [`IndexSidecar`].
//! - `.agents/ticket-catalog.md` - an agent-hook pointer at the catalog.

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

/// Provenance comment written at the top of `.ticket/README.md`.
pub const TICKET_INDEX_FILE_COMMENT: &str =
    "<!-- ticket-index:file generated=true -->";

/// Per-entry provenance prefix used in `.ticket/README.md`.
pub const TICKET_INDEX_ENTRY_PREFIX: &str = "ticket-index:entry";

/// Provenance comment for the generated agent-hook file.
pub const TICKET_INDEX_AGENT_HOOK_COMMENT: &str =
    "<!-- ticket-index:agent-hook generated=true -->";

/// Repository-relative path of the generated agent-hook file.
pub const TICKET_INDEX_AGENT_HOOK_PATH: &str = ".agents/ticket-catalog.md";

/// One joined ticket source with the fields needed for deterministic generation.
pub struct TicketCatalogSource {
    /// Ticket UUID.
    pub id: uuid::Uuid,
    /// Workspace-relative canonical path to `ticket.toml` (`/` separators).
    pub source_path: String,
    /// Ticket title.
    pub title: String,
    /// Ticket state.
    pub state: String,
    /// Ticket priority field.
    pub priority: Option<String>,
    /// Ticket component field.
    pub component: Option<String>,
    /// Raw ticket description markdown.
    pub description: String,
}

/// The generated ticket catalog artifacts, ready for the caller to write or diff.
pub struct TicketCatalogArtifacts {
    /// Sidecar for `.ticket/index.toon`.
    pub sidecar: IndexSidecar,
    /// Rendered `.ticket/README.md` catalog.
    pub readme_markdown: String,
    /// Rendered `.agents/ticket-catalog.md` agent-hook content.
    pub agent_hook_markdown: String,
}

#[derive(Default)]
struct TicketDisplayExtra {
    state: String,
    component: String,
    priority: Option<String>,
}

fn epoch() -> DateTime<Utc> {
    DateTime::from_timestamp(0, 0).expect("epoch is valid")
}

/// Generate the full ticket catalog from joined sources.
pub fn generate_ticket_catalog(
    sources: &[TicketCatalogSource],
    store_dir: &str,
) -> TicketCatalogArtifacts {
    let generated_at = epoch();

    let mut entries: Vec<IndexEntry> = sources
        .iter()
        .map(|s| make_entry(s, generated_at))
        .collect();
    for entry in &mut entries {
        entry.seal();
    }

    let extras: BTreeMap<uuid::Uuid, TicketDisplayExtra> = sources
        .iter()
        .map(|source| {
            (
                source.id,
                TicketDisplayExtra {
                    state: state_key(&source.state),
                    component: component_key(source.component.as_deref()),
                    priority: source
                        .priority
                        .as_deref()
                        .map(normalize_token)
                        .filter(|v| !v.is_empty()),
                },
            )
        })
        .collect();

    let mut sidecar =
        IndexSidecar::new(ContentKind::Ticket, store_dir, entries);
    sidecar.generated_at = generated_at;
    sidecar.sort();

    let readme_markdown = render_catalog_markdown(&sidecar, &extras);
    let agent_hook_markdown = render_agent_hook(&sidecar, store_dir, &extras);

    TicketCatalogArtifacts {
        sidecar,
        readme_markdown,
        agent_hook_markdown,
    }
}

fn make_entry(
    source: &TicketCatalogSource,
    generated_at: DateTime<Utc>,
) -> IndexEntry {
    let title = if source.title.trim().is_empty() {
        source.id.to_string()
    } else {
        source.title.trim().to_string()
    };

    let state = state_key(&source.state);
    let component = component_key(source.component.as_deref());
    let priority = source
        .priority
        .as_deref()
        .map(normalize_token)
        .filter(|v| !v.is_empty());

    let mut tags = vec!["ticket".to_string(), state.clone(), component.clone()];
    if let Some(priority) = &priority {
        tags.push(priority.clone());
    }
    normalize_tokens(&mut tags);

    let mut keywords =
        vec!["ticket".to_string(), state.clone(), component.clone()];
    keywords.extend(words_for_keywords(&title));
    keywords.extend(words_for_keywords(&source.description));
    if let Some(priority) = &priority {
        keywords.push(priority.clone());
    }
    normalize_tokens(&mut keywords);

    let short_id = short_id(source.id);
    let mut relations = IndexRelations::default();
    relations.related.push(IndexRef {
        canonical_path: source.source_path.clone(),
        entry_id: source.id,
        relation_kind: RelationKind::Related,
        content_kind: ContentKind::Ticket,
        digest: String::new(),
        anchor: Some(short_id),
    });

    IndexEntry {
        id: source.id,
        kind: ContentKind::Ticket,
        source_path: source.source_path.clone(),
        title,
        summary: normalize_summary(&source.description),
        keywords,
        scope: Some(format!("state={state}, component={component}")),
        non_goals: None,
        relations,
        digest: String::new(),
        tags,
        generated_at,
        source_modified_at: None,
    }
}

fn normalize_summary(description: &str) -> String {
    for raw in description.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("```") {
            continue;
        }

        let clean = line
            .trim_start_matches(['-', '*', '+'])
            .trim_start_matches(|c: char| {
                c.is_ascii_digit() || c == '.' || c == ')'
            })
            .trim();
        if clean.is_empty() {
            continue;
        }

        let collapsed = clean.split_whitespace().collect::<Vec<_>>().join(" ");
        return truncate_chars(&collapsed, 200);
    }
    String::new()
}

fn truncate_chars(
    input: &str,
    max_chars: usize,
) -> String {
    let count = input.chars().count();
    if count <= max_chars {
        return input.to_string();
    }
    let take = max_chars.saturating_sub(1);
    format!("{}...", input.chars().take(take).collect::<String>())
}

fn state_key(state: &str) -> String {
    let cleaned = normalize_token(state);
    if cleaned.is_empty() {
        "unknown".to_string()
    } else {
        cleaned
    }
}

fn component_key(component: Option<&str>) -> String {
    component
        .map(normalize_token)
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "unspecified".to_string())
}

fn normalize_token(input: &str) -> String {
    input
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-")
        .replace(' ', "-")
}

fn words_for_keywords(input: &str) -> Vec<String> {
    input
        .split(|c: char| !c.is_ascii_alphanumeric())
        .map(|w| w.trim().to_ascii_lowercase())
        .filter(|w| w.len() >= 3)
        .collect()
}

fn normalize_tokens(items: &mut Vec<String>) {
    for item in items.iter_mut() {
        *item = normalize_token(item);
    }
    items.retain(|item| !item.is_empty());
    items.sort();
    items.dedup();
}

fn short_id(id: uuid::Uuid) -> String {
    id.simple().to_string()[..8].to_string()
}

fn render_catalog_markdown(
    sidecar: &IndexSidecar,
    extras: &BTreeMap<uuid::Uuid, TicketDisplayExtra>,
) -> String {
    let mut by_group: BTreeMap<(String, String), Vec<&IndexEntry>> =
        BTreeMap::new();
    for entry in &sidecar.entries {
        let extra = extras.get(&entry.id).expect("entry has display data");
        by_group
            .entry((extra.state.clone(), extra.component.clone()))
            .or_default()
            .push(entry);
    }

    let mut out = String::new();
    out.push_str(TICKET_INDEX_FILE_COMMENT);
    out.push_str("\n\n# Ticket Catalog\n\n");
    out.push_str(
        "Generated ticket index grouped by state and component. Use this before scanning raw `.ticket/tickets/` folders.\n",
    );

    let mut current_state = String::new();
    for ((state, component), entries) in by_group {
        if state != current_state {
            out.push_str(&format!("\n## State: {}\n", state));
            current_state = state.clone();
        }
        out.push_str(&format!("\n### Component: {}\n\n", component));

        let mut sorted = entries;
        sorted.sort_by(|a, b| {
            a.title.cmp(&b.title).then_with(|| a.id.cmp(&b.id))
        });

        for entry in sorted {
            let digest_prefix =
                entry.digest.get(0..12).unwrap_or(&entry.digest);
            out.push_str(&format!(
                "<!-- {TICKET_INDEX_ENTRY_PREFIX} id={} slug={}/{} digest={} -->\n",
                entry.id, state, component, digest_prefix
            ));
            out.push_str(&render_entry_block(entry, extras.get(&entry.id)));
            out.push('\n');
        }
    }

    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn render_entry_block(
    entry: &IndexEntry,
    extra: Option<&TicketDisplayExtra>,
) -> String {
    let mut block = String::new();
    block.push_str(&format!("#### [{}] {}\n", short_id(entry.id), entry.title));

    if let Some(priority) = extra.and_then(|e| e.priority.as_deref()) {
        block.push_str(&format!("- priority: `{priority}`\n"));
    }
    if !entry.summary.is_empty() {
        block.push_str(&format!("- summary: {}\n", entry.summary));
    }
    block.push_str(&format!("- ref: `{}`\n", entry.source_path));
    block
}

fn render_agent_hook(
    sidecar: &IndexSidecar,
    store_dir: &str,
    extras: &BTreeMap<uuid::Uuid, TicketDisplayExtra>,
) -> String {
    let total = sidecar.entries.len();

    let mut state_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut component_counts: BTreeMap<String, usize> = BTreeMap::new();
    for entry in &sidecar.entries {
        if let Some(extra) = extras.get(&entry.id) {
            *state_counts.entry(extra.state.clone()).or_insert(0) += 1;
            *component_counts.entry(extra.component.clone()).or_insert(0) += 1;
        }
    }

    let mut out = String::new();
    out.push_str(TICKET_INDEX_AGENT_HOOK_COMMENT);
    out.push_str("\n\n# Ticket Catalog\n\n");
    out.push_str(&format!(
        "The full ticket catalog is generated at `{store_dir}/README.md`\n\
         (machine-readable sidecar: `{store_dir}/index.toon`).\n\n"
    ));
    out.push_str(&format!("- Total tickets: {total}\n"));
    if !state_counts.is_empty() {
        out.push_str("- States: ");
        out.push_str(
            &state_counts
                .iter()
                .map(|(state, count)| format!("{state} ({count})"))
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push('\n');
    }
    if !component_counts.is_empty() {
        out.push_str("- Components: ");
        out.push_str(
            &component_counts
                .iter()
                .map(|(component, count)| format!("{component} ({count})"))
                .collect::<Vec<_>>()
                .join(", "),
        );
        out.push('\n');
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(
        id: uuid::Uuid,
        path: &str,
        title: &str,
        state: &str,
        priority: Option<&str>,
        component: Option<&str>,
        description: &str,
    ) -> TicketCatalogSource {
        TicketCatalogSource {
            id,
            source_path: path.to_string(),
            title: title.to_string(),
            state: state.to_string(),
            priority: priority.map(str::to_string),
            component: component.map(str::to_string),
            description: description.to_string(),
        }
    }

    #[test]
    fn summary_takes_first_text_line() {
        assert_eq!(
            normalize_summary("# Heading\n\nFirst summary line.\nSecond."),
            "First summary line."
        );
        assert_eq!(normalize_summary("## Heading only\n\n```rs"), "");
    }

    #[test]
    fn catalog_groups_by_state_and_component() {
        let id_a =
            uuid::Uuid::parse_str("aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa")
                .unwrap();
        let id_b =
            uuid::Uuid::parse_str("bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb")
                .unwrap();
        let sources = vec![
            source(
                id_a,
                ".ticket/tickets/a/ticket.toml",
                "Fix bug",
                "in-review",
                Some("high"),
                Some("ticket-api"),
                "Bug summary.",
            ),
            source(
                id_b,
                ".ticket/tickets/b/ticket.toml",
                "Write docs",
                "new",
                Some("low"),
                Some("docs"),
                "Docs summary.",
            ),
        ];

        let artifacts = generate_ticket_catalog(&sources, ".ticket");
        assert!(artifacts.readme_markdown.contains("## State: in-review"));
        assert!(artifacts.readme_markdown.contains("## State: new"));
        assert!(
            artifacts
                .readme_markdown
                .contains("### Component: ticket-api")
        );
        assert!(artifacts.readme_markdown.contains("### Component: docs"));
    }

    #[test]
    fn entry_relations_reference_canonical_ticket_path() {
        let id = uuid::Uuid::parse_str("cccccccc-cccc-4ccc-8ccc-cccccccccccc")
            .unwrap();
        let sources = vec![source(
            id,
            ".ticket/tickets/c/ticket.toml",
            "Implement command",
            "in-implementation",
            Some("high"),
            Some("ticket-api"),
            "Command summary.",
        )];

        let artifacts = generate_ticket_catalog(&sources, ".ticket");
        let entry = &artifacts.sidecar.entries[0];
        assert_eq!(entry.relations.related.len(), 1);
        assert_eq!(
            entry.relations.related[0].canonical_path,
            ".ticket/tickets/c/ticket.toml"
        );
        assert_eq!(entry.relations.related[0].entry_id, id);
        assert!(entry.is_digest_valid());
    }

    #[test]
    fn generation_is_deterministic_for_same_input() {
        let id = uuid::Uuid::parse_str("dddddddd-dddd-4ddd-8ddd-dddddddddddd")
            .unwrap();
        let sources = vec![source(
            id,
            ".ticket/tickets/d/ticket.toml",
            "Stable output",
            "ready",
            Some("medium"),
            Some("ticket-api"),
            "Deterministic summary text.",
        )];

        let a = generate_ticket_catalog(&sources, ".ticket");
        let b = generate_ticket_catalog(&sources, ".ticket");

        assert_eq!(a.readme_markdown, b.readme_markdown);
        assert_eq!(a.agent_hook_markdown, b.agent_hook_markdown);
        assert_eq!(
            a.sidecar.encode_toon().unwrap(),
            b.sidecar.encode_toon().unwrap()
        );
    }
}
