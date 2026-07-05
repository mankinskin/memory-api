# Summary
Implement the path normalization utility kernel specified in the design spec and migrate all transport-facing path surfaces to it.

Governing spec: `memory-api/.spec/specs/b4833ecc-78b6-4406-9a03-9834d211f0ae/body.md` (slug `memory-api/workspace/path-normalization-kernel`).

# Locked decisions (from design spec)
- Strict canonicalization is the default contract for every transport-facing path surface.
- Canonicalization failures surface through CLI, MCP, and HTTP as structured errors.
- One shared UNC passing strategy in the kernel; downstream code consumes normalized output only.
- Raw input paths appear only in error payloads/diagnostics to describe the failure.
- Normalize everywhere on the happy path; do not emit ad-hoc raw path variants in transport payloads.
- Drive-letter paths normalize to a Unix-style form such as `/c/...` and never render as `C:/...`.

# Implementation roadmap
1. Land the shared workspace kernel with strict and display-oriented entry points.
2. Switch CLI, MCP, and HTTP path payloads to normalized-only output and structured strict failures.
3. Remove command-local slash collapsing and any direct raw-path propagation outside error payloads.
4. Add and un-ignore regression tests for drive-letter, UNC, verbatim UNC, and failure-reporting paths.
5. Verify the end-to-end transport contract with focused validation before broader rollout.

# Migration map (call sites)
- `crates/memory-api/src/workspace.rs` — consolidate `strip_verbatim_prefix`, `normalize_working_dir_path`, and display rendering helper(s) into the kernel.
- `tools/cli/spec-cli/src/cli/commands/refs.rs` — replace command-local slash collapsing with the kernel display helper.
- `tools/cli/ticket-cli/src/cli/commands/lifecycle.rs`
- `tools/cli/session-cli/src/lib.rs`
- `tools/mcp/ticket-mcp/src/server/mutations.rs`
- `tools/mcp/session-mcp/src/server.rs`

# Acceptance criteria
- Kernel exposes the design API: `normalize_path_for_display`, `normalize_path_for_display_strict`, `canonicalize_workspace_root_strict`, `canonicalize_workspace_root_lossy`, and `WorkspacePathError`.
- Drive-letter, Git Bash, verbatim drive, UNC, and verbatim UNC inputs normalize per the spec invariants.
- The four ignored guard tests are un-ignored and pass:
  - `render_workspace_root_for_payload_preserves_unc_root`
  - `render_workspace_root_for_payload_normalizes_verbatim_unc_root`
  - `strip_verbatim_prefix_normalizes_verbatim_unc_prefix`
  - `strip_verbatim_prefix_preserves_unc_root`
- Drive-letter payload tests assert the normalized Unix-style form, not `C:/...`.
- CLI, MCP, and HTTP path surfaces emit normalized-only payloads and structured strict errors.
- Focused validation passes and is recorded under `vt-spec-root-awareness-transport` (or a successor validation spec).

# Validation
- `cargo test -p memory-api workspace::` for kernel invariants.
- spec-cli refs payload tests for transport rendering.
- Focused CLI/MCP/HTTP path-surface checks before broader rollout.