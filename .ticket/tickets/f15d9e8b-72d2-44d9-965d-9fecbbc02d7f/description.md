# Problem

Agent-facing markdown guidance is duplicated across context-engine, memory-viewers, memory-api, and viewer-api. The duplicated files are currently copy-pasted and several are byte-identical. This creates multiple editable copies of the same guidance and no stable way to trace a generated paragraph back to a canonical instruction source.

# Decision Record

- `memory-api` remains the shared storage foundation for memory tools.
- The new domain API for canonical instruction/rule content is `rule-api`.
- `rule-api` must provide the shared storage integration used by memory tools alongside `ticket-api`, `spec-api`, and `audit-api`.
- Paragraphs are the canonical rule unit, with optional sentence anchors when finer tracing is needed.
- All agent-facing markdown files will eventually be generated.
- Manual edits to generated files are not part of the long-term workflow; edits must pass through `rule-api` tools.
- Phase one must include both CLI and MCP surfaces.
- The first generated slice includes the byte-identical shared files and the path-scoped instruction files.
- Generated files are committed to git and validated by git hooks.
- Provenance inside generated markdown uses HTML-style markdown comments.
- The canonical provenance comments are:
  - `<!-- rule-api:file generated=true -->` at file scope
  - `<!-- rule-api:entry id=<uuid> slug=<slug> -->` immediately before each generated paragraph block
- Ratings and feedback stay attached to instruction entries as indexed metadata plus attached assets, not as separate entity types.
- Manual session references in phase one require at least `session_id` and `agent_or_user_id`.
- Automatic usage capture by tools is out of scope for phase one and deferred until the basic storage, generation, and feedback loop is implemented and tested.
- Rule-entry lifecycle states are schema-enforced.
- Default git-hook enforcement in this repo uses versioned hooks under `.githooks/`, specifically `.githooks/pre-commit`. `.github/hooks/` remains advisory Copilot hook infrastructure, not the git gate.

# Goal

Replace copy-pasted markdown guidance with a canonical `rule-api` snippet registry plus deterministic generation of repo-local `AGENTS.md` and `.github` markdown files.

# Scope

This tracker coordinates:

- the `rule-api` storage and domain model
- the CLI and MCP authoring/import/generation tooling surface
- the entry-attached ratings and feedback model
- the migration of duplicated markdown files into generated outputs
- pre-commit validation that prevents drift and direct edits

# Acceptance Criteria

- Shared guidance is authored once through `rule-api`, not by editing four copied markdown trees.
- Repo-local `AGENTS.md`, shared `.github` files, and path-scoped instruction files are generated deterministically from canonical rule entries plus repo-local overlays.
- Generated paragraphs can be traced back to stable rule entry ids through markdown comments.
- Agents can record ratings and feedback notes against rule entries through CLI and MCP tools.
- Generated outputs are committed to git and validated by `.githooks/pre-commit`.
- Migration of the four affected repositories preserves repo-specific guidance while removing copy-paste ownership of shared text.

# Open Questions

None at the tracker level for phase one. Remaining details are implementation-level within the child tickets.