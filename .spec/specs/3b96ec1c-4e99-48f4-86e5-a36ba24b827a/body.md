# Summary

`memory-api` needs its own repo-local rule workspace so the repo README and local usage guides are authored next to the crates and tools they describe. The local target config should stay manageable by using a file/folder tree layout that groups generated outputs by domain and tool type.

## Problem

The current generated rule targets live at the parent repo level. That leaves `memory-api` without a local rule workspace, without a local target config, and without a documented repo-local flow for generating its own README.

## Scope

This spec defines the desired local rule workspace for `memory-api/`:

- a repo-local `.rule/` store
- a repo-local `rule-targets.yaml`
- canonical local rule entries for the `memory-api` README and repo-local tool READMEs
- a documented local generation workflow

## Local Target Layout

The repo-local `rule-targets.yaml` should support an explicit file/folder tree:

- root `files:` entries for repo-root outputs such as `README.md`
- nested `folders:` entries for grouped tool surfaces such as `tools/cli`, `tools/mcp`, and `tools/http`
- one file node per generated artifact so the config mirrors the runtime path layout and remains easy to scan by domain and type

## README User Story

As a maintainer of `memory-api/`, I need the README to be generated from local rule entries so crate layout, tool surfaces, setup steps, and validation commands stay consistent with the code and can be regenerated instead of hand-edited.

## Expected README Coverage

The generated README should, at minimum, cover:

- purpose and repository scope
- crate groups (`memory-api`, `rule-api`, `spec-api`, `ticket-api`, `audit-api`)
- tool groups (CLI, MCP, HTTP)
- local development and validation commands
- where to find repo-local specs and generated rule outputs

## Local Usage Guide

1. Add or edit README rule entries under `memory-api/.rule/rules/**`.
2. Keep `rule-targets.yaml` grouped by output path and tool type so README targets remain easy to find and maintain.
3. Preview the README target with `rule explain-target --config rule-targets.yaml --target memory-api-readme`.
4. Regenerate with `rule sync-targets --config rule-targets.yaml`.
5. Validate the generated `README.md` and any README-target tests before review.

## Acceptance Criteria

- `memory-api/` contains a repo-local rule workspace and `rule-targets.yaml`.
- The README target is defined locally and renders to `memory-api/README.md`.
- The local target config can be expressed as a file/folder tree with outputs grouped by root files, CLI tools, MCP tools, and HTTP tools.
- The generated README is fully reproducible from local rule entries.
- The implementation includes validation coverage for the target config, generated output tracking, and any local README-specific rendering rules.
