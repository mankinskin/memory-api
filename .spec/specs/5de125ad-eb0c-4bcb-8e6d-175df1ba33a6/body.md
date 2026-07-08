<!-- aligned-structure:v1 -->

# Summary

Nested rule workspaces should extend the existing `rule-api` store and target model from "one store + one config" into an explicitly scanned workspace graph. The owning repo workspace remains the unit of authoring, while generation and explanation can reuse persisted child-workspace scan roots deterministically.

## Behavior Story

Nested rule workspaces should extend the existing `rule-api` store and target model from "one store + one config" into an explicitly scanned workspace graph. The owning repo workspace remains the unit of authoring, while generation and explanation can reuse persisted child-workspace scan roots deterministically.

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# Summary

Nested rule workspaces should extend the existing `rule-api` store and target model from "one store + one config" into an explicitly scanned workspace graph. The owning repo workspace remains the unit of authoring, while generation and explanation can reuse persisted child-workspace scan roots deterministically.

## Current Seam

Current generation opens one `RuleStore` and one config path:

- `RuleStore::open(index_root)`
- `load_render_target_config(config_path)`
- `resolve_render_target_output(config_path, target)`

The store already supports extra scan roots, but ordinary read and render commands should treat those roots as persisted state, not as a trigger to walk the repo tree on every invocation. Child-workspace discovery belongs to explicit maintenance flows so repeated `get`, `list`, `search`, `generate-target`, `explain-target`, and `sync-targets` runs can reuse the stored aggregate view instead of rescanning the filesystem.

## Required Behavior

### Workspace discovery

- A repo-local workspace is identified by a `.rule/` store root.
- Nested workspaces may exist in submodule repositories or explicit subfolders.
- CLI and MCP operations must be able to resolve the local workspace from the current directory or an explicit workspace root.
- Descendant child workspaces must only be discovered by explicit maintenance commands such as `rule scan` or explicit root registration.
- Discovered child workspaces must be persisted as scan roots so later read and render commands can reuse them without another filesystem walk.

### Aggregated read model

- Parent workspaces may read rules from descendant workspaces after an explicit scan or explicit root registration.
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
2. Run `rule scan` when you want that workspace to discover child `.rule/` stores and persist them for reuse.
3. Run `rule list`, `rule explain-target`, or `rule sync-targets` against that workspace root after the scan roots you need are already persisted.
4. When operating from a parent repo, review the explanation output to confirm which child workspace contributed each rule.
5. Use repo-prefixed slugs and repo scopes so aggregated generation remains deterministic and auditable.

## Test Strategy

The implementation should add or expand tests for:

- explicit scan discovery order across nested repos
- parent generation that includes child rules only after a scan persists the child workspace roots
- isolation of child-only generation
- provenance in explain output
- generated target identity keyed by workspace root + config path + target name

## Acceptance Criteria

- `memory-viewers/` can generate parent targets using rules from `memory-api/` and `viewer-api/` child workspaces after `rule scan` persists those child roots.
- A nested repo can generate its own targets without loading unrelated parent outputs.
- Before a scan persists child roots, parent `get`, `list`, `search`, and render commands do not walk the repo tree to discover child workspaces implicitly.
- Explain output identifies the originating workspace for every matched rule.
- Existing tests for hierarchical targets and generated target persistence remain valid or are updated with nested-workspace coverage.
