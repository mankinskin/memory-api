# Status: TODO

# Search, Highlighting, and Reference Discovery

## Goal

Support fast query and highlighting across:
- Ticket titles/manifests
- Rich text descriptions
- Checklists/comments
- Validation findings
- Optional attachment-derived text

## Candidate: Tantivy

### Why
- Rust-native search engine library.
- Supports indexing text fields with tokenization and scoring.
- Stored fields enable reconstructing display content.
- Segment-based architecture suitable for incremental updates.

### Source
- https://docs.rs/tantivy/latest/tantivy/

## Suggested Index Documents

Index one or more documents per ticket:
- `ticket_header` document: ID, title, status, tags, owner
- `ticket_body` document: markdown/plain text
- `ticket_checklist` document: normalized checklist text/state
- `ticket_validation` document: validation messages with severity/path

Fields to include:
- `ticket_id` (string, fast filter)
- `doc_kind` (enum/string)
- `content` (text, tokenized)
- `path` (string)
- `updated_at` (timestamp)
- `status` (keyword)
- `labels` (keyword multi)

## Update Model

- On ticket commit:
  - write to primary storage
  - emit event
  - synchronously or asynchronously reindex affected ticket docs
- Keep `index_version` for rebuild compatibility.

## Highlighting

- Use query-term highlighting/snippet generation to annotate matching text.
- For markdown, map snippet offsets back to source lines when possible.

## TODO

- TODO: Define tokenizer/stemming policy for technical text.
- TODO: Define search ranking strategy (recency + relevance weighting).
- TODO: Decide sync vs async indexing and consistency guarantees.
- TODO: Add index rebuild command for corruption or schema changes.
