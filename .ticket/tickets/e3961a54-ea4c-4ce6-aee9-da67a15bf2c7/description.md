# Summary
Implementation-ready design for a single resilient path normalization utility kernel in memory-api, focused on Unix-style canonical path rendering across Windows/Unix environments, including explicit canonicalization failure surfaces.

Governing spec: `memory-api/.spec/specs/b4833ecc-78b6-4406-9a03-9834d211f0ae/body.md` (slug `memory-api/workspace/path-normalization-kernel`).

# Delivered
- Audited path normalization call sites in memory-api and CLI/MCP/HTTP consumers.
- Added targeted UNC/verbatim-prefix regression guard tests (currently ignored, pending kernel implementation).
- Authored the design spec with the contract, normalization invariants, error model, and locked decisions.

# Decision log (locked)
- Strict canonicalization is the default contract for every transport-facing path surface.
- Canonicalization failures surface through CLI, MCP, and HTTP as structured errors.
- One shared UNC passing strategy in the kernel; downstream consumes normalized output only.
- Raw input paths appear only in error payloads/diagnostics.
- Normalize everywhere on the happy path; no ad-hoc raw path variants in transport payloads.
- Drive-letter paths normalize to a Unix-style form such as `/c/...` and never render as `C:/...`.

No open questions remain.

# Work package
- Implementation follow-on: e8e3ef17-313f-4cb7-aa9c-6447a18d36a3 (holds roadmap + migration map).
- Prior path fix (prerequisite): 59d96577-09a8-44a7-b0ea-3d51b3a6fb05
- Related cross-worktree move (surfaced the verbatim-prefix bug): 21e6c015-55c6-4807-8d55-16193ed687ed