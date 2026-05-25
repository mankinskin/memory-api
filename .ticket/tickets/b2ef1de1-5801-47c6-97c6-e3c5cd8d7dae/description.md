# Problem

Even with generated body/section APIs, there is no supported command that evaluates the declared rule targets for a spec and updates the spec store consistently. Running `rule-api` generation directly against files would bypass the spec-oriented workflow and leave indexing, history, and validation behavior underspecified.

## Desired outcome

A maintainer can run one supported command to regenerate all declared spec artifacts from rule targets.

## Proposed direction

- Add `spec sync-generated` or an equivalent orchestration surface.
- For each descriptor entry, resolve the configured target through `rule-api`, render the markdown output, then write the result through the spec-owned generated body/section paths.
- Refresh search/index/history state after generated files change and define how provenance comments should be handled by spec-facing queries.

## Acceptance criteria

- A focused CLI command regenerates `body.md` and named sections from descriptor entries.
- The command fails clearly when target resolution, rendering, or artifact writing fails.
- Generated artifact writes preserve newline conventions and do not bypass spec bookkeeping.
- Focused validation covers at least one successful sync and one failure mode.
