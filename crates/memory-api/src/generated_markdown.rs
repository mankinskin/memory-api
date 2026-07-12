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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedGeneratedMarkdownEntry {
    pub id: String,
    pub slug: Option<String>,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParsedGeneratedMarkdownArtifact {
    pub entries: Vec<ParsedGeneratedMarkdownEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParseGeneratedMarkdownError {
    MissingGeneratedFileComment,
    MalformedEntryComment(String),
    NoEntriesFound,
}

impl std::fmt::Display for ParseGeneratedMarkdownError {
    fn fmt(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        match self {
            Self::MissingGeneratedFileComment => write!(
                f,
                "input is not a generated artifact (missing generated file comment)"
            ),
            Self::MalformedEntryComment(line) =>
                write!(f, "malformed generated entry comment: {line}"),
            Self::NoEntriesFound =>
                write!(f, "generated artifact contains no entry comments"),
        }
    }
}

impl std::error::Error for ParseGeneratedMarkdownError {}

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

pub fn parse_generated_artifact(
    content: &str,
    config: &GeneratedMarkdownConfig<'_>,
) -> Result<ParsedGeneratedMarkdownArtifact, ParseGeneratedMarkdownError> {
    let normalized = normalize_newlines_to_lf(content);
    let mut remainder = normalized.as_str();
    let frontmatter = if config.skip_provenance_for_yaml_frontmatter {
        if let Some((frontmatter, rest)) = split_yaml_frontmatter(remainder) {
            remainder = rest;
            Some(frontmatter.trim_end().to_string())
        } else {
            None
        }
    } else {
        None
    };

    remainder = remainder.trim_start_matches('\n');
    if !remainder.starts_with(config.file_comment.as_ref()) {
        return Err(ParseGeneratedMarkdownError::MissingGeneratedFileComment);
    }

    remainder = &remainder[config.file_comment.len()..];
    let markers = parse_entry_markers(remainder, &config.entry_prefix)?;
    if markers.is_empty() {
        return Err(ParseGeneratedMarkdownError::NoEntriesFound);
    }

    let mut entries = Vec::with_capacity(markers.len());
    for (index, marker) in markers.iter().enumerate() {
        let body_end = markers
            .get(index + 1)
            .map(|next| next.line_start)
            .unwrap_or(remainder.len());
        let mut body = remainder[marker.body_start..body_end]
            .trim_end()
            .to_string();
        if index == 0 {
            if let Some(frontmatter) = frontmatter.as_deref() {
                body = reattach_frontmatter(frontmatter, &body);
            }
        }

        entries.push(ParsedGeneratedMarkdownEntry {
            id: marker.id.clone(),
            slug: marker.slug.clone(),
            body,
        });
    }

    Ok(ParsedGeneratedMarkdownArtifact { entries })
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EntryMarker {
    line_start: usize,
    body_start: usize,
    id: String,
    slug: Option<String>,
}

fn parse_entry_markers(
    content: &str,
    entry_prefix: &str,
) -> Result<Vec<EntryMarker>, ParseGeneratedMarkdownError> {
    let mut markers = Vec::new();
    let mut cursor = 0usize;

    while cursor <= content.len() {
        let line_end_rel = content[cursor..]
            .find('\n')
            .map(|offset| offset + cursor)
            .unwrap_or(content.len());
        let line = &content[cursor..line_end_rel];
        if let Some((id, slug)) = parse_entry_marker_line(line, entry_prefix)? {
            let body_start = if line_end_rel < content.len() {
                line_end_rel + 1
            } else {
                line_end_rel
            };
            markers.push(EntryMarker {
                line_start: cursor,
                body_start,
                id,
                slug,
            });
        }

        if line_end_rel == content.len() {
            break;
        }
        cursor = line_end_rel + 1;
    }

    Ok(markers)
}

fn parse_entry_marker_line(
    line: &str,
    entry_prefix: &str,
) -> Result<Option<(String, Option<String>)>, ParseGeneratedMarkdownError> {
    let trimmed = line.trim();
    if !trimmed.starts_with("<!--") || !trimmed.ends_with("-->") {
        return Ok(None);
    }

    let inner = trimmed
        .trim_start_matches("<!--")
        .trim_end_matches("-->")
        .trim();
    if !inner.starts_with(entry_prefix) {
        return Ok(None);
    }

    let attrs = inner[entry_prefix.len()..].trim();
    if attrs.is_empty() {
        return Err(ParseGeneratedMarkdownError::MalformedEntryComment(
            line.to_string(),
        ));
    }

    let mut id = None;
    let mut slug = None;
    for token in attrs.split_whitespace() {
        if let Some(value) = token.strip_prefix("id=") {
            if value.is_empty() {
                return Err(ParseGeneratedMarkdownError::MalformedEntryComment(
                    line.to_string(),
                ));
            }
            id = Some(value.to_string());
        } else if let Some(value) = token.strip_prefix("slug=") {
            if !value.is_empty() {
                slug = Some(value.to_string());
            }
        }
    }

    let Some(id) = id else {
        return Err(ParseGeneratedMarkdownError::MalformedEntryComment(
            line.to_string(),
        ));
    };
    Ok(Some((id, slug)))
}

fn reattach_frontmatter(
    frontmatter: &str,
    body: &str,
) -> String {
    if body.is_empty() {
        format!("{}\n", frontmatter.trim_end())
    } else {
        format!("{}\n{}", frontmatter.trim_end(), body)
    }
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
        ParseGeneratedMarkdownError,
        GeneratedMarkdownConfig,
        GeneratedMarkdownSnippet,
        parse_generated_artifact,
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

    #[test]
    fn parse_generated_artifact_restores_frontmatter_to_first_entry() {
        let rendered = render_markdown_file(
            &[GeneratedMarkdownSnippet::new(
                "prompt",
                Some("context-engine/prompts/spec"),
                "---\nname: spec\n---\nCreate a new spec entry.\n",
            )],
            &rule_like_config(),
        );

        let parsed = parse_generated_artifact(&rendered, &rule_like_config())
            .expect("parse generated artifact");
        assert_eq!(parsed.entries.len(), 1);
        assert_eq!(parsed.entries[0].id, "prompt");
        assert_eq!(
            parsed.entries[0].body,
            "---\nname: spec\n---\nCreate a new spec entry."
        );
    }

    #[test]
    fn parse_generated_artifact_rejects_non_generated_file() {
        let error =
            parse_generated_artifact("# not generated", &rule_like_config())
                .expect_err("must fail");
        assert_eq!(
            error,
            ParseGeneratedMarkdownError::MissingGeneratedFileComment
        );
    }

    #[test]
    fn parse_generated_artifact_handles_crlf_input_and_trimmed_bodies() {
        let input = concat!(
            "<!-- generated:file true -->\r\n\r\n",
            "<!-- generated:entry id=one slug=shared/a -->\r\n",
            "alpha\r\n\r\n",
            "<!-- generated:entry id=two slug=shared/b -->\r\n",
            "beta\r\n",
        );

        let parsed = parse_generated_artifact(input, &rule_like_config())
            .expect("parse generated artifact");
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[0].body, "alpha");
        assert_eq!(parsed.entries[1].body, "beta");
    }
}
