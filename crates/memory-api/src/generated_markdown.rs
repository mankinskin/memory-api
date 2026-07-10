use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedMarkdownSnippet<'a> {
    pub id: Cow<'a, str>,
    pub slug: Option<Cow<'a, str>>,
    pub body: Cow<'a, str>,
}

impl<'a> GeneratedMarkdownSnippet<'a> {
    pub fn new<I, S, B>(
        id: I,
        slug: Option<S>,
        body: B,
    ) -> Self
    where
        I: Into<Cow<'a, str>>,
        S: Into<Cow<'a, str>>,
        B: Into<Cow<'a, str>>,
    {
        Self {
            id: id.into(),
            slug: slug.map(Into::into),
            body: body.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeneratedMarkdownConfig<'a> {
    pub file_comment: Cow<'a, str>,
    pub entry_prefix: Cow<'a, str>,
    pub skip_provenance_for_yaml_frontmatter: bool,
}

impl<'a> GeneratedMarkdownConfig<'a> {
    pub fn new<I, E>(
        file_comment: I,
        entry_prefix: E,
    ) -> Self
    where
        I: Into<Cow<'a, str>>,
        E: Into<Cow<'a, str>>,
    {
        Self {
            file_comment: file_comment.into(),
            entry_prefix: entry_prefix.into(),
            skip_provenance_for_yaml_frontmatter: true,
        }
    }
}

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

pub fn render_markdown_file(
    snippets: &[GeneratedMarkdownSnippet<'_>],
    config: &GeneratedMarkdownConfig<'_>,
) -> String {
    let mut rendered = String::new();
    let first_frontmatter = if config.skip_provenance_for_yaml_frontmatter {
        snippets
            .first()
            .and_then(|snippet| split_yaml_frontmatter(&snippet.body))
    } else {
        None
    };

    if let Some((frontmatter, _)) = first_frontmatter {
        rendered.push_str(frontmatter.trim_end());
        rendered.push_str("\n\n");
    }

    rendered.push_str(config.file_comment.as_ref());

    for (index, snippet) in snippets.iter().enumerate() {
        rendered.push_str("\n\n");
        rendered.push_str(&format!(
            "<!-- {} id={} slug={} -->\n",
            config.entry_prefix,
            snippet.id,
            snippet.slug.as_deref().unwrap_or_default(),
        ));

        let body = if index == 0 {
            first_frontmatter
                .map(|(_, remainder)| remainder)
                .unwrap_or(snippet.body.as_ref())
        } else {
            snippet.body.as_ref()
        };

        rendered.push_str(body.trim_end());
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

fn split_yaml_frontmatter(body: &str) -> Option<(&str, &str)> {
    let mut lines = body.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end_matches(['\r', '\n']) != "---" {
        return None;
    }

    let mut offset = first.len();
    for line in lines {
        offset += line.len();
        if line.trim_end_matches(['\r', '\n']) == "---" {
            return Some((&body[..offset], &body[offset..]));
        }
    }

    None
}

fn normalize_newlines_to_lf(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn apply_existing_line_endings(
    rendered: &str,
    existing: &str,
) -> String {
    let endings = collect_line_endings(existing);
    if endings.is_empty()
        || endings.iter().all(|ending| *ending == LineEnding::Lf)
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
    use pretty_assertions::assert_eq;

    use super::{
        GeneratedMarkdownConfig,
        GeneratedMarkdownSnippet,
        prepare_generated_output,
        render_markdown_file,
    };

    fn rule_like_config() -> GeneratedMarkdownConfig<'static> {
        GeneratedMarkdownConfig::new(
            "<!-- generated:file true -->",
            "generated:entry",
        )
    }

    #[test]
    fn render_markdown_file_emits_provenance_comments_and_trimmed_blocks() {
        let rendered = render_markdown_file(
            &[
                GeneratedMarkdownSnippet::new(
                    "one",
                    Some("shared/agents/opening"),
                    "Start with the concrete anchor.\n",
                ),
                GeneratedMarkdownSnippet::new(
                    "two",
                    Some("shared/agents/validation"),
                    "Run the focused check next.",
                ),
            ],
            &rule_like_config(),
        );

        assert_eq!(
            rendered,
            "<!-- generated:file true -->\n\n<!-- generated:entry id=one slug=shared/agents/opening -->\nStart with the concrete anchor.\n\n<!-- generated:entry id=two slug=shared/agents/validation -->\nRun the focused check next.\n"
        );
    }

    #[test]
    fn render_markdown_file_keeps_frontmatter_first_and_emits_provenance() {
        let rendered = render_markdown_file(
            &[GeneratedMarkdownSnippet::new(
                "prompt",
                Some("context-engine/prompts/spec"),
                "---\nname: spec\n---\nCreate a new spec entry.\n",
            )],
            &rule_like_config(),
        );

        assert_eq!(
            rendered,
            "---\nname: spec\n---\n\n<!-- generated:file true -->\n\n<!-- generated:entry id=prompt slug=context-engine/prompts/spec -->\nCreate a new spec entry.\n"
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
