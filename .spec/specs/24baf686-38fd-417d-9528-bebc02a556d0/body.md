# Summary

`memory-api` should gain a `docs` domain so humans and agents can navigate repository structure through the same family pattern already used for rules, specs, tickets, and audits. The first primitive is a generated table of contents in each repository README, derived from canonical `rule-api` content and aware of nested sub-workspaces.

## Motivation

The repository tree is increasingly organized as a family of related sub-repositories. Navigation knowledge is currently split across generated READMEs, local rule targets, and tool-specific conventions. That makes it harder for humans and agents to answer a basic question: what exists in this repo, what belongs to a child workspace, and where should a given operation start?

A dedicated `doc-api` family should expose repository documentation and navigation as a first-class surface inside `memory-viewers/memory-api/`, parallel to `rule-api`, `spec-api`, `ticket-api`, and `audit-api`.

## Scope

This spec defines a new `doc-api` family with matching transport layers:

- `crates/doc-api` for the repository documentation model and workspace-aware navigation logic
- `tools/cli/doc-cli` for local querying and generation workflows
- `tools/http/doc-http` for HTTP access to repository docs and navigation payloads
- `tools/mcp/doc-mcp` for agent-facing docs and TOC queries

The initial use case for the family is repository navigation through generated README tables of contents and related workspace-aware documentation queries.

## Intended Behavior

### Repository TOC generation

- Every repository can expose a generated table of contents in its `README.md`.
- The TOC is derived from canonical `rule-api` content instead of being maintained by hand.
- TOC entries remain stable and linkable for both humans and agents.
- Generated README structure can include local sections plus summaries from nested child workspaces without losing provenance.

### Workspace-aware docs model

- `doc-api` resolves the current repository workspace and any configured or discoverable child workspaces.
- Parent repositories can present an aggregated navigation view across child workspaces.
- Child repositories remain queryable and generatable in isolation.
- Navigation data identifies the originating workspace for entries contributed by child workspaces.

### Family transport outline

Initial surface expectations:

- `doc-cli` supports commands for repository docs inspection, table-of-contents output, and workspace-aware navigation queries.
- `doc-http` exposes endpoints for repository summary, README/TOC payloads, and workspace navigation views.
- `doc-mcp` exposes machine-oriented tools so agents can locate repo summaries, navigate sub-workspaces, and request generated TOC data without shelling out.

## Relationship to Existing Specs

This spec builds on related `rule-api` workspace behavior rather than replacing it:

- `rule-api/workspaces/nested-resolution` defines how parent and child workspaces are discovered and composed.
- `rule-api/workspaces/memory-api-readme-generation` defines local README generation for the `memory-api` repo.

`doc-api` should consume those capabilities as inputs and expose a docs-oriented read/query surface around them.

## Constraints and Non-Goals

- `rule-api` remains the canonical source of truth for generated README inputs and rule-target rendering.
- This spec does not replace rule storage, target composition, or generated-target bookkeeping.
- This spec does not require a new viewer UI or a rewrite of `doc-viewer`.
- The new family should follow the existing repository pattern for API plus transport layers instead of inventing a one-off layout.
- The design must account for sub-repositories as sub-workspaces and avoid flattening them into a single anonymous repo view.

## Acceptance Criteria

- A root `doc-api` domain is defined for the `memory-api` family of tools.
- The planned repository layout includes `crates/doc-api`, `tools/cli/doc-cli`, `tools/http/doc-http`, and `tools/mcp/doc-mcp`.
- The docs model defines how repository README tables of contents are generated from canonical `rule-api` content.
- The docs model defines how parent repositories include child-workspace summaries while preserving workspace provenance.
- The resulting family is clearly positioned as the navigation and documentation surface parallel to the existing `rule`, `spec`, `ticket`, and `audit` families.
- Related README-generation and nested-workspace specs can reference this spec instead of duplicating docs-surface behavior.
