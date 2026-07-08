<!-- aligned-structure:v1 -->

# Summary

Establish one resilient, memory-api-owned path normalization utility kernel that emits Unix-style path strings for transport/UI while preserving typed path safety for filesystem operations, with deterministic behavior across Windows and Unix callers.

## Behavior Story

Establish one resilient, memory-api-owned path normalization utility kernel that emits Unix-style path strings for transport/UI while preserving typed path safety for filesystem operations, with deterministic behavior across Windows and Unix callers.

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# Path Normalization Utility Kernel (Design)

## Goal
Establish one resilient, memory-api-owned path normalization utility kernel that emits Unix-style path strings for transport/UI while preserving typed path safety for filesystem operations, with deterministic behavior across Windows and Unix callers.

## Problem
Path normalization behavior is currently distributed across workspace helpers and command-specific payload shaping. The current distribution fixed drive-letter and verbatim-prefix defects but leaves UNC/verbatim-UNC edge behavior incomplete, and does not expose one explicit error contract for canonicalization failures.

## Scope
- Define one kernel API in `crates/memory-api/src/workspace.rs` (or sibling module) that all CLI/MCP/HTTP path payload surfaces consume.
- Normalize Windows inputs including:
  - drive-letter (`C:\repo\x`)
  - Git Bash (`/c/repo/x`)
  - verbatim drive (`\\?\C:\repo\x`)
  - UNC (`\\server\share\x`)
  - verbatim UNC (`\\?\UNC\server\share\x`)
- Keep Unix behavior stable.
- Make strict canonicalization the default contract for all transport-facing path surfaces.
- Add clear canonicalization error signaling when strict canonicalization is requested and cannot be completed.
- Migrate existing call sites currently using ad-hoc `replace('\\', '/')`, slash collapsing, or implicit fallback behavior.

## Non-goals
- No change to storage layout (`.ticket`, `.spec`, `.rule`) semantics.
- No cross-store move workflow redesign (already covered by done ticket `21e6c015`).
- No transport schema redesign beyond path-field value normalization and explicit error payloads where strict mode is opted in.

## Proposed Kernel Contract

### API shape (design target)
- `normalize_path_for_display(path: &Path) -> String`
- `normalize_path_for_display_strict(path: &Path) -> Result<String, WorkspacePathError>`
- `canonicalize_workspace_root_strict(path: &Path) -> Result<PathBuf, WorkspacePathError>`
- `canonicalize_workspace_root_lossy(path: &Path) -> PathBuf` (current fallback semantics, explicit naming)

### Normalization invariants
- Output separators are `/`.
- Drive-letter paths normalize to a Unix-style form such as `/c/...` and never render as `C:/...` in payloads.
- UNC paths remain rooted as `//server/share/...`.
- Verbatim prefixes (`\\?\`, `//?/`) are removed before normalization.
- Verbatim UNC (`\\?\UNC\server\share\...`) is normalized to `//server/share/...`.
- No accidental collapse of semantic UNC root marker from `//` to `/`.

### Decision lock
- Prefer strict canonicalization for every transport surface, not just mutation paths.
- Surface canonicalization failures through CLI, MCP, and HTTP with structured errors.
- Use one shared UNC passing strategy in the kernel and let downstream code consume normalized output only.
- Use raw input paths only in error payloads and diagnostics to explain the failure.
- Normalize paths everywhere on the happy path; do not preserve ad-hoc raw path variants in transport payloads.

### Error model
`WorkspacePathError` (design target):
- `CanonicalizeFailed { input: String, source: io::Error }`
- `InvalidWindowsPrefix { input: String, detail: String }`
- `UnrepresentablePath { input: String, detail: String }`

Transport behavior:
- Existing lossy/default flows keep backward-compatible success behavior.
- Strict flows return structured errors with actionable messages (which root failed and why).

## Test Strategy
New targeted guard tests were added as ignored pending tests (enable after kernel implementation):
- `render_workspace_root_for_payload_preserves_unc_root`
- `render_workspace_root_for_payload_normalizes_verbatim_unc_root`
- `strip_verbatim_prefix_normalizes_verbatim_unc_prefix`
- `strip_verbatim_prefix_preserves_unc_root`
- drive-letter payload tests should assert the normalized Unix-style form, not a `C:/...` rendering.

Existing passing coverage remains for drive-letter + basic verbatim cases.

## Evidence Trail
Validation spec: `vt-spec-root-awareness-transport`
Executions:
- `exec-spec-root-awareness-transport-20260630-focused-pass` (passed)
- `exec-spec-root-awareness-transport-20260630-blocked` (blocked by Windows App Control policy on spec-cli bin target)

## Ticket Traceability
- Design + guard tests: `C:/Users/linus/git/graph_app/context-engine/memory-api/memory-api/.ticket/tickets/e3961a54-ea4c-4ce6-aee9-da67a15bf2c7`
- Implementation (roadmap + migration map): `C:/Users/linus/git/graph_app/context-engine/.ticket/tickets/e8e3ef17-313f-4cb7-aa9c-6447a18d36a3`
- Prior path fix (spec-cli root awareness): `C:/Users/linus/git/graph_app/context-engine/.ticket/tickets/59d96577-09a8-44a7-b0ea-3d51b3a6fb05`
- Related cross-worktree move (surfaced verbatim-prefix bug): `C:/Users/linus/git/graph_app/context-engine/memory-api/.ticket/tickets/21e6c015-55c6-4807-8d55-16193ed687ed`
