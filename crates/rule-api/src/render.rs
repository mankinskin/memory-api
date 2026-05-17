use crate::manifest::RuleManifest;

pub const GENERATED_FILE_COMMENT: &str =
    "<!-- rule-api:file generated=true -->";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineEnding {
    Lf,
    Crlf,
}

impl LineEnding {
    fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::Crlf => "\r\n",
        }
    }
}

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

pub fn prepare_generated_output(
    rendered: &str,
    existing: Option<&str>,
) -> String {
    let normalized = normalize_newlines_to_lf(rendered);
    existing
        .map(|text| apply_existing_line_endings(&normalized, text))
        .unwrap_or(normalized)
}

fn starts_with_yaml_frontmatter(body: &str) -> bool {
    body.lines()
        .next()
        .is_some_and(|line| line.trim_end_matches('\r') == "---")
}

fn normalize_newlines_to_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn apply_existing_line_endings(
    rendered: &str,
    existing: &str,
) -> String {
    let endings = collect_line_endings(existing);
    if endings.is_empty() || endings.iter().all(|ending| *ending == LineEnding::Lf)
    {
        return rendered.to_string();
    }

    let fallback = dominant_line_ending(&endings);
    let mut adapted = String::with_capacity(
        rendered.len()
            + endings
                .iter()
                .filter(|ending| **ending == LineEnding::Crlf)
                .count(),
    );
    let bytes = rendered.as_bytes();
    let mut segment_start = 0usize;
    let mut ending_index = 0usize;
    let mut index = 0usize;

    while index < bytes.len() {
        if bytes[index] == b'\n' {
            adapted.push_str(&rendered[segment_start..index]);
            adapted.push_str(
                endings
                    .get(ending_index)
                    .copied()
                    .unwrap_or(fallback)
                    .as_str(),
            );
            segment_start = index + 1;
            ending_index += 1;
        }
        index += 1;
    }

    adapted.push_str(&rendered[segment_start..]);
    adapted
}

fn collect_line_endings(text: &str) -> Vec<LineEnding> {
    let bytes = text.as_bytes();
    let mut endings = Vec::new();
    let mut index = 0usize;

    while index < bytes.len() {
        match bytes[index] {
            b'\r' if index + 1 < bytes.len() && bytes[index + 1] == b'\n' => {
                endings.push(LineEnding::Crlf);
                index += 2;
            },
            b'\n' => {
                endings.push(LineEnding::Lf);
                index += 1;
            },
            _ => {
                index += 1;
            },
        }
    }

    endings
}

fn dominant_line_ending(endings: &[LineEnding]) -> LineEnding {
    let crlf_count = endings
        .iter()
        .filter(|ending| **ending == LineEnding::Crlf)
        .count();
    if crlf_count > endings.len().saturating_sub(crlf_count) {
        LineEnding::Crlf
    } else {
        LineEnding::Lf
    }
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
        let prepared = prepare_generated_output(
            "first\r\nsecond\r\nthird\n",
            None,
        );

        assert_eq!(prepared, "first\nsecond\nthird\n");
    }
}
