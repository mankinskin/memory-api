# Summary

`rule-api` already supports canonical rule storage in `.rule/rules/**`, repo-scoped filtering, hierarchical target outlines, and deterministic generation from a single `rule-targets.yaml`. The next step is to make rule workspaces first-class at nested repo boundaries so each submodule repository can own its local rules while parent repositories can compose child rules into parent targets.

## Repositories In Scope

- `memory-viewers/`
- `memory-viewers/memory-api/`
- `memory-viewers/viewer-api/`

## Problem

Today the active rule workflow is effectively single-workspace:

- one `.rule/` store root
- one `rule-targets.yaml` per generation run
- one `RuleStore::open(index_root)` call per CLI or MCP invocation

That works for the top-level `context-engine` workspace, but it does not let nested repositories carry their own local rule workspace, local targets, or local authoring workflow without routing everything back through the parent repository.

## User Stories

### memory-viewers maintainer

As the maintainer of `memory-viewers/`, I need a repo-local rule workspace that can generate parent-level docs while reusing rules authored inside child repos such as `memory-api/` and `viewer-api/`.

### memory-api maintainer

As the maintainer of `memory-api/`, I need a repo-local rule workspace so README content, usage guides, and subsystem guidance can be authored beside the code they describe instead of in the parent repo.

### viewer-api maintainer

As the maintainer of `viewer-api/`, I need a repo-local rule workspace so viewer-specific guidance can evolve independently and still participate in parent-level documentation when needed.

## Proposed Workspace Model

Each in-scope repository should support the same local rule workspace shape:

- `.rule/rules/<uuid>/rule.toml`
- `.rule/rules/<uuid>/description.md`
- optional repo-local generated target config in `rule-targets.yaml`

A parent repo workspace may aggregate child workspaces discovered in nested submodule or subfolder roots, but local authoring remains owned by the repo that contains the rule entry.

## Usage Guide

1. Author or update local rules from the repo that owns the documentation concern.
2. Preview composition with `rule explain-target --config rule-targets.yaml --target <name>`.
3. Regenerate repo-local outputs with `rule sync-targets --config rule-targets.yaml`.
4. From a parent workspace, generate parent outputs that intentionally include descendant rules where the target configuration calls for them.

## Non-Goals

- Replacing the existing top-level `context-engine/.rule` workflow immediately.
- Allowing parent repos to mutate child rule entries in place.
- Hiding rule provenance when multiple workspaces contribute to one target.

## Acceptance Criteria

- The nested rule workspace topology is documented for `memory-viewers`, `memory-api`, and `viewer-api`.
- User stories and local authoring flows are defined for each repo.
- Child implementation specs can build on this spec without redefining repository ownership or workflow vocabulary.
- The final implementation must preserve deterministic target generation and explicit rule provenance.
