# Summary

Nested rule workspaces should extend the existing `rule-api` store and target model from "one store + one config" into a discovered workspace graph. The owning repo workspace remains the unit of authoring, while generation and explanation can traverse child workspaces deterministically.

## Current Seam

Current generation opens one `RuleStore` and one config path:

- `RuleStore::open(index_root)`
- `load_render_target_config(config_path)`
- `resolve_render_target_output(config_path, target)`

The store also supports extra scan roots, but that is still a manually assembled view inside a single workspace. There is no repo-level discovery model, no child-workspace provenance, and no parent-to-child composition contract.

## Required Behavior

### Workspace discovery

- A repo-local workspace is identified by a `.rule/` store root.
- Nested workspaces may exist in submodule repositories or explicit subfolders.
- CLI and MCP operations must be able to resolve the local workspace from the current directory or an explicit workspace root.

### Aggregated read model

- Parent workspaces may read rules from descendant workspaces.
- Child workspaces remain independently queryable and generatable in isolation.
- Rule provenance must include the workspace root that supplied each matched rule.

### Target composition

- Parent targets may combine local nodes with child-workspace rules.
- Child targets do not implicitly write parent outputs.
- Generated-target bookkeeping must remain stable even when the same target name exists in different workspaces.

### Compatibility

- Existing single-workspace flows remain valid.
- Existing hierarchical target configs continue to work when no child workspaces are present.
- Repo-scoped slugs stay explicit so aggregated views do not rely on ambiguous anonymous rule identifiers.

## Usage Guide

1. Enter the repo whose workspace you want to operate on.
2. Run `rule list`, `rule explain-target`, or `rule sync-targets` against that workspace root.
3. When operating from a parent repo, review the explanation output to confirm which child workspace contributed each rule.
4. Use repo-prefixed slugs and repo scopes so aggregated generation remains deterministic and auditable.

## Test Strategy

The implementation should add or expand tests for:

- workspace discovery order across nested repos
- parent generation that includes child rules
- isolation of child-only generation
- provenance in explain output
- generated target identity keyed by workspace root + config path + target name

## Acceptance Criteria

- `memory-viewers/` can generate parent targets using rules from `memory-api/` and `viewer-api/` child workspaces.
- A nested repo can generate its own targets without loading unrelated parent outputs.
- Explain output identifies the originating workspace for every matched rule.
- Existing tests for hierarchical targets and generated target persistence remain valid or are updated with nested-workspace coverage.
