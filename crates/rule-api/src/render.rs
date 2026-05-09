use crate::manifest::RuleManifest;

pub const GENERATED_FILE_COMMENT: &str =
    "<!-- rule-api:file generated=true -->";

pub fn render_markdown_file(rules: &[RuleManifest]) -> String {
    let mut rendered = String::from(GENERATED_FILE_COMMENT);

    for rule in rules {
        rendered.push_str("\n\n");
        rendered.push_str(&format!(
            "<!-- rule-api:entry id={} slug={} -->\n",
            rule.id,
            rule.slug().unwrap_or_default()
        ));
        rendered.push_str(rule.body().unwrap_or_default().trim_end());
    }

    rendered.push('\n');
    rendered
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
}
