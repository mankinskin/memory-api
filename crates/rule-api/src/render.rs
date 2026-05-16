use crate::manifest::RuleManifest;

pub const GENERATED_FILE_COMMENT: &str =
    "<!-- rule-api:file generated=true -->";

pub fn render_markdown_file(rules: &[RuleManifest]) -> String {
    let include_provenance_comments =
        !rules.first().and_then(RuleManifest::body).is_some_and(
            starts_with_yaml_frontmatter,
        );
    let mut rendered = String::new();

    if include_provenance_comments {
        rendered.push_str(GENERATED_FILE_COMMENT);
    }

    for (index, rule) in rules.iter().enumerate() {
        if include_provenance_comments {
            rendered.push_str("\n\n");
            rendered.push_str(&format!(
                "<!-- rule-api:entry id={} slug={} -->\n",
                rule.id,
                rule.slug().unwrap_or_default()
            ));
        } else if index > 0 {
            rendered.push_str("\n\n");
        }

        rendered.push_str(rule.body().unwrap_or_default().trim_end());
    }

    rendered.push('\n');
    rendered
}

fn starts_with_yaml_frontmatter(body: &str) -> bool {
    body.lines()
        .next()
        .is_some_and(|line| line.trim_end_matches('\r') == "---")
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
    fn render_markdown_file_omits_provenance_when_body_starts_with_frontmatter() {
        let prompt = RuleManifest::new(
            "context-engine/prompts/spec",
            "Spec Prompt",
            ".prompt",
            "spec-prompt",
            "---\nname: spec\n---\nCreate a new spec entry.\n",
        );

        let rendered = render_markdown_file(&[prompt]);

        assert_eq!(
            rendered,
            "---\nname: spec\n---\nCreate a new spec entry.\n"
        );
    }
}
