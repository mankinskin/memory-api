# Problem

`rule-api` currently operates on one workspace root and one target config per invocation. That is enough for the top-level `context-engine` workflow, but it does not let `memory-viewers/`, `memory-api/`, and `viewer-api/` own repo-local rule workspaces while still allowing parent repositories to generate targets that intentionally include child-repo rules.

## Scope

- Add repo-local `.rule` workspaces for `memory-viewers/`, `memory-viewers/memory-api/`, and `memory-viewers/viewer-api/`.
- Support nested workspace discovery for submodule repositories and explicit subfolders.
- Extend CLI and MCP generation or explanation flows so they can resolve a local workspace root, load descendant workspaces, and report workspace provenance for matched rules.
- Preserve current single-workspace behavior when no nested workspaces exist.
- Align the implementation with specs `rule-api/workspaces` and `rule-api/workspaces/nested-resolution`.

## User Stories

- As a `memory-viewers` maintainer, I can generate parent docs that intentionally include rules authored in `memory-api` and `viewer-api`.
- As a `memory-api` maintainer, I can work entirely inside the local repo and still have those rules available to a parent generator when needed.
- As a `viewer-api` maintainer, I can add repo-local rules without editing the parent repo's `.rule` store directly.

## Usage Guide

1. From a repo root with a local `.rule` workspace, run `rule list`, `rule explain-target`, or `rule sync-targets` against the local config.
2. From a parent repo, generate targets that explicitly include descendant workspaces and inspect provenance in explain output.
3. Keep repo-prefixed slugs stable so nested aggregation remains deterministic.

## Acceptance Criteria

- Parent and child repo-local `.rule` workspaces are supported for submodule and subfolder layouts.
- `rule-cli` and `rule-mcp` can resolve a selected workspace root, load child workspaces, and explain rule provenance.
- Existing single-workspace target generation remains compatible.
- Tests cover workspace discovery order, isolated child generation, and parent generation using child rules.
