use crate::manifest::RuleManifest;
use memory_api::generated_markdown::{
    GeneratedMarkdownConfig,
    GeneratedMarkdownSnippet,
    prepare_generated_output as shared_prepare_generated_output,
    render_markdown_file as shared_render_markdown_file,
};

pub const GENERATED_FILE_COMMENT: &str =
    "<!-- rule-api:file generated=true -->";

const GENERATED_ENTRY_PREFIX: &str = "rule-api:entry";

pub fn render_markdown_file(rules: &[RuleManifest]) -> String {
    let config = GeneratedMarkdownConfig::new(
        GENERATED_FILE_COMMENT,
        GENERATED_ENTRY_PREFIX,
    );
    let snippets = rules
        .iter()
        .map(rule_to_generated_snippet)
        .collect::<Vec<_>>();

    shared_render_markdown_file(&snippets, &config)
}

pub fn prepare_generated_output(
    rendered: &str,
    existing: Option<&str>,
) -> String {
    shared_prepare_generated_output(rendered, existing)
}

fn rule_to_generated_snippet(
    rule: &RuleManifest
) -> GeneratedMarkdownSnippet<'_> {
    GeneratedMarkdownSnippet::new(
        rule.id.to_string(),
        rule.slug(),
        rule.body().unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_markdown_file_emits_provenance_comments_and_trimmed_blocks() {
        let first = RuleManifest::new(
            "shared/agents/opening",
            "Opening",
            "AGENTS",
            "opening",
            "Start with the concrete anchor.\n",
        );
        let second = RuleManifest::new(
            "shared/agents/validation",
            "Validation",
            "AGENTS",
            "validation",
            "Run the focused check next.",
        );

        let rendered = render_markdown_file(&[first.clone(), second.clone()]);

        assert_eq!(
            rendered,
            format!(
                "<!-- rule-api:file generated=true -->\n\n<!-- rule-api:entry id={} slug=shared/agents/opening -->\nStart with the concrete anchor.\n\n<!-- rule-api:entry id={} slug=shared/agents/validation -->\nRun the focused check next.\n",
                first.id, second.id,
            )
        );
    }

    #[test]
    fn render_markdown_file_keeps_frontmatter_first_and_emits_provenance() {
        let prompt = RuleManifest::new(
            "context-engine/prompts/spec",
            "Spec Prompt",
            ".prompt",
            "spec-prompt",
            "---\nname: spec\n---\nCreate a new spec entry.\n",
        );
        let prompt_id = prompt.id.to_string();

        let rendered = render_markdown_file(&[prompt]);

        assert_eq!(
            rendered,
            "---\nname: spec\n---\n\n<!-- rule-api:file generated=true -->\n\n<!-- rule-api:entry id=".to_string()
                + &prompt_id
                + " slug=context-engine/prompts/spec -->\nCreate a new spec entry.\n"
        );
    }

    #[test]
    fn prepare_generated_output_preserves_existing_crlf_style() {
        let prepared = prepare_generated_output(
            "first\nsecond\nthird\n",
            Some("old\r\ncontent\r\nblock\r\n"),
        );

        assert_eq!(prepared, "first\r\nsecond\r\nthird\r\n");
    }

    #[test]
    fn prepare_generated_output_reuses_existing_mixed_newline_sequence() {
        let prepared = prepare_generated_output(
            "first\nsecond\nthird\n",
            Some("old\r\ncontent\nblock\r\n"),
        );

        assert_eq!(prepared, "first\r\nsecond\nthird\r\n");
    }

    #[test]
    fn prepare_generated_output_normalizes_new_files_to_lf() {
        let prepared =
            prepare_generated_output("first\r\nsecond\r\nthird\n", None);

        assert_eq!(prepared, "first\nsecond\nthird\n");
    }
}
