# Summary

This spec defines the canonical installation contract for the `memory-api` operator surfaces that are exposed to users through generated README documentation. The goal is to keep one synchronized install process across executable validation, `.spec` contracts, and `.rule` README generation.

## In Scope

- CLI installation and deinstallation for `rule`, `spec`, `ticket`, and `audit`
- repo-local root discovery and first-run initialization described in the generated `memory-api/README.md`
- the boundary between CLI install coverage and viewer install coverage
- the synchronization contract between install-focused spec sections, executable validation, and the README rule entry that renders the install section

## Canonical Sections

- `readme-install-flow.md` is the canonical markdown snippet for the generated README install and setup section.
- `cli-scenario-matrix.md` defines the required CLI installation scenarios for executable validation.
- `viewer-install-boundary.md` defines the current viewer lifecycle coverage and records the present uninstall gap in `viewer-ctl`.

## Synchronization Rules

1. The README rule entry for the memory-api install section must stay aligned with `readme-install-flow.md`.
2. Executable install validation must consume or validate against the same scenario definitions captured by these sections.
3. CI must treat drift between executable tests, install spec sections, and generated README rule content as a failure.

## Acceptance Criteria

- The spec entry exists under `memory-api/.spec/specs/**`.
- The spec records the exact README install snippet, the CLI scenario matrix, and the viewer coverage boundary.
- A focused executable validation checks that the README rule install section stays synchronized with this spec.
