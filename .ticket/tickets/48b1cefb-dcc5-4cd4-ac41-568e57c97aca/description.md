# Problem

A canonical `rule-api` store is not enough by itself. The team needs tools to import duplicated markdown, edit canonical rule entries, and generate repo-local files so manual file editing is no longer the source of truth.

# Decision Record

- All agent-facing markdown files will eventually be generated.
- Manual edits must pass through `rule-api` tools instead of direct edits to generated files.
- Phase one must ship both CLI and MCP surfaces.
- The first generated slice includes the byte-identical shared files and the path-scoped instruction files.
- Generated files are committed to git and validated by git hooks.
- Provenance inside generated output uses HTML-style markdown comments.
- The exact comment syntax for phase one is:
  - `<!-- rule-api:file generated=true -->` at file scope
  - `<!-- rule-api:entry id=<uuid> slug=<slug> -->` immediately before each generated paragraph block
- Phase one should provide the basic authoring and generation workflow before any automatic usage-capture features.
- Default git-hook validation in this repo runs from `.githooks/pre-commit` after `git config core.hooksPath .githooks`. No `pre-push` hook is required by default. CI may reuse the same validation command later.

# Scope

Build the first `rule-api` tooling surface for authoring and generation.

Minimum capabilities:

- import existing markdown into canonical rule entries
- create/update/reorder/move rule entries
- render repo-local `AGENTS.md` and `.github` markdown files from canonical entries
- render path-scoped instruction files from canonical entries
- validate or diff generated output against canonical source
- surface rule entry provenance for generated blocks via markdown comments
- expose the core workflow through both CLI and MCP
- integrate validation into `.githooks/pre-commit`

# Acceptance Criteria

- A `rule-api` CLI exists to create or update rule entries without hand-editing generated markdown.
- A `rule-api` MCP surface exists so agents can perform the same core operations through tool calls.
- The tools can generate repo-local markdown files in the locations expected by GitHub Copilot and related tooling.
- The tools can show a dry-run diff or validation failure when generated output diverges.
- Generated output is deterministic across runs.
- Each generated block can be traced back to a stable rule entry id via markdown comments.
- `.githooks/pre-commit` rejects drift between canonical source and committed generated outputs.

# Open Questions

None blocking phase one. Later CI integration can reuse the same validation command without changing the local default hook layout.