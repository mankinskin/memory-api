#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarkdownImportOptions {
    pub slug_prefix: String,
    pub default_section: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedRuleBlock {
    pub slug: String,
    pub title: String,
    pub section: String,
    pub body: String,
    pub order_key: i64,
    pub source_start_line: i64,
    pub source_end_line: i64,
}

pub fn import_markdown_blocks(
    content: &str,
    options: &MarkdownImportOptions,
) -> Vec<ImportedRuleBlock> {
    let chunks = markdown_chunks(content);
    let mut blocks = Vec::new();
    let mut heading_path: Vec<String> = Vec::new();
    let mut pending_heading: Option<Chunk> = None;

    for chunk in chunks {
        if is_generated_comment_chunk(&chunk) {
            continue;
        }

        let heading_info = extract_heading_info(&chunk);
        let is_heading_only =
            heading_info.is_some() && chunk.lines.iter().all(|line| is_heading_line(line));

        if let Some((level, heading_slug, _)) = &heading_info {
            update_heading_path(&mut heading_path, *level, heading_slug.clone());

            if is_heading_only {
                pending_heading = Some(chunk);
                continue;
            }
        }

        let merged = if let Some(heading_chunk) = pending_heading.take() {
            merge_chunks(heading_chunk, chunk)
        } else {
            chunk
        };

        let section = if heading_path.is_empty() {
            normalize_slug_component(&options.default_section)
        } else {
            heading_path.join("/")
        };

        blocks.push(ImportedRuleBlock {
            slug: format!(
                "{}/{}/l{}",
                trim_slashes(&options.slug_prefix),
                section,
                merged.start_line,
            ),
            title: block_title(&merged),
            section,
            body: merged.text(),
            order_key: blocks.len() as i64 + 1,
            source_start_line: merged.start_line,
            source_end_line: merged.end_line,
        });
    }

    if let Some(heading_chunk) = pending_heading {
        let section = if heading_path.is_empty() {
            normalize_slug_component(&options.default_section)
        } else {
            heading_path.join("/")
        };

        blocks.push(ImportedRuleBlock {
            slug: format!(
                "{}/{}/l{}",
                trim_slashes(&options.slug_prefix),
                section,
                heading_chunk.start_line,
            ),
            title: block_title(&heading_chunk),
            section,
            body: heading_chunk.text(),
            order_key: blocks.len() as i64 + 1,
            source_start_line: heading_chunk.start_line,
            source_end_line: heading_chunk.end_line,
        });
    }

    blocks
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Chunk {
    lines: Vec<String>,
    start_line: i64,
    end_line: i64,
}

impl Chunk {
    fn text(&self) -> String {
        self.lines.join("\n").trim_end().to_string()
    }
}

fn markdown_chunks(content: &str) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut current_lines = Vec::new();
    let mut current_start = 0i64;
    let mut current_end = 0i64;

    for (index, raw_line) in content.lines().enumerate() {
        let line_no = index as i64 + 1;
        let line = raw_line.trim_end_matches('\r');

        if line.trim().is_empty() {
            if !current_lines.is_empty() {
                chunks.push(Chunk {
                    lines: std::mem::take(&mut current_lines),
                    start_line: current_start,
                    end_line: current_end,
                });
            }
            continue;
        }

        if current_lines.is_empty() {
            current_start = line_no;
        }
        current_end = line_no;
        current_lines.push(line.to_string());
    }

    if !current_lines.is_empty() {
        chunks.push(Chunk {
            lines: current_lines,
            start_line: current_start,
            end_line: current_end,
        });
    }

    chunks
}

fn merge_chunks(first: Chunk, second: Chunk) -> Chunk {
    let mut lines = first.lines;
    lines.push(String::new());
    lines.extend(second.lines);
    Chunk {
        lines,
        start_line: first.start_line,
        end_line: second.end_line,
    }
}

fn is_generated_comment_chunk(chunk: &Chunk) -> bool {
    chunk
        .lines
        .iter()
        .all(|line| line.trim_start().starts_with("<!-- rule-api:"))
}

fn is_heading_line(line: &str) -> bool {
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
    hashes > 0 && hashes <= 6 && trimmed.chars().nth(hashes) == Some(' ')
}

fn extract_heading_info(chunk: &Chunk) -> Option<(usize, String, String)> {
    for line in &chunk.lines {
        let trimmed = line.trim_start();
        let level = trimmed.chars().take_while(|ch| *ch == '#').count();
        if level > 0 && level <= 6 && trimmed.chars().nth(level) == Some(' ') {
            let title = trimmed[level + 1..].trim();
            return Some((
                level,
                normalize_slug_component(title),
                title.to_string(),
            ));
        }
    }
    None
}

fn update_heading_path(path: &mut Vec<String>, level: usize, slug: String) {
    let keep = level.saturating_sub(1);
    path.truncate(keep);
    path.push(slug);
}

fn block_title(chunk: &Chunk) -> String {
    if let Some((_, _, title)) = extract_heading_info(chunk) {
        return title;
    }

    first_text_line(chunk).chars().take(80).collect::<String>()
}

fn first_text_line(chunk: &Chunk) -> String {
    for line in &chunk.lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if is_heading_line(trimmed) {
            let hashes = trimmed.chars().take_while(|ch| *ch == '#').count();
            return trimmed[hashes + 1..].trim().to_string();
        }
        return trimmed
            .trim_start_matches(['-', '*', '+'])
            .trim()
            .to_string();
    }
    "Imported Block".to_string()
}

fn normalize_slug_component(input: &str) -> String {
    let mut out = String::new();
    let mut last_dash = false;

    for ch in input.chars().flat_map(|ch| ch.to_lowercase()) {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
            last_dash = false;
        } else if !last_dash {
            out.push('-');
            last_dash = true;
        }
    }

    let normalized = out.trim_matches('-');
    if normalized.is_empty() {
        "section".to_string()
    } else {
        normalized.to_string()
    }
}

fn trim_slashes(value: &str) -> String {
    value.trim_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_markdown_blocks_attaches_heading_chunks_and_keeps_order() {
        let content = "# Opening\n\nStart with the concrete anchor.\n\n## Validation\n\n- Run the focused check\n- Then tests\n\nFinal note.";
        let blocks = import_markdown_blocks(
            content,
            &MarkdownImportOptions {
                slug_prefix: "shared/agents".to_string(),
                default_section: "agents".to_string(),
            },
        );

        assert_eq!(blocks.len(), 3);
        assert_eq!(blocks[0].section, "opening");
        assert_eq!(blocks[0].slug, "shared/agents/opening/l1");
        assert_eq!(blocks[0].body, "# Opening\n\nStart with the concrete anchor.");
        assert_eq!(blocks[1].section, "opening/validation");
        assert_eq!(blocks[1].body, "## Validation\n\n- Run the focused check\n- Then tests");
        assert_eq!(blocks[2].section, "opening/validation");
        assert_eq!(blocks[2].body, "Final note.");
        assert_eq!(blocks[2].order_key, 3);
    }
}