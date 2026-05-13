# Problem

The existing `AGENTS.md` and `.github` markdown files are duplicated across context-engine, memory-viewers, memory-api, and viewer-api. Shared text is currently owned by copy-paste instead of by a canonical rule source.

# Decision Record

- Shared rule content should be authored once through `rule-api` and rendered into each repo-local markdown tree.
- Repo-local files remain in the runtime locations expected by GitHub Copilot and related tooling, but they are generated outputs.
- Repo-specific guidance remains possible through overlays or repo applicability metadata.
- The first migration slice includes the byte-identical shared files and the path-scoped instruction files.
- Generated files are committed to git and validated by git hooks.
- Generated provenance uses HTML-style markdown comments.
- Repo-specific `applyTo` differences are represented at file-render configuration level, not duplicated in paragraph entries:
  - shared rule-entry bodies stay canonical
  - repo-local file metadata and frontmatter are rendered from repo-specific overlays/config
  - `applyTo` stays a property of the rendered instruction file, not of each paragraph block
- The regression check for phase one is `rule-api validate --staged` run from `.githooks/pre-commit`. It must fail when:
  - generated outputs drift from canonical source
  - staged edits modify generated files in ways regeneration does not reproduce
  - required provenance comments are missing or malformed
  - shared-content fingerprints appear outside canonical `rule-api` source and approved generated outputs

# Scope

Inventory the current duplicated markdown, classify each block as shared or repo-specific, import shared content into canonical rule entries, and rewrite the affected repo-local files as generated outputs.

Phase-one repos:

- context-engine
- memory-viewers
- memory-api
- viewer-api

# Acceptance Criteria

- A duplication inventory exists for the current files and identifies canonical ownership of shared rule content.
- The current byte-identical files are imported into canonical rule entries without losing traceability.
- The current path-scoped instruction files are imported or normalized into canonical rule entries without losing repo-specific applicability.
- Repo-specific instructions are preserved as overlays or scoped rule entries instead of being flattened into global text.
- The affected repo-local files can be regenerated from `rule-api` content after migration.
- Generated files remain committed and validated by `.githooks/pre-commit` after migration.

# Open Questions

None blocking phase one.