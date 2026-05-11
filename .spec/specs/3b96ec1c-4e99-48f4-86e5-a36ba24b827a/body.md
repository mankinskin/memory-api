# Summary

`memory-api` needs its own repo-local rule workspace so the repo README and local usage guides are authored next to the crates and tools they describe. The first concrete generated target in that workspace should be `README.md` for the `memory-api` repo.

## Problem

The current generated rule targets live at the parent repo level. That leaves `memory-api` without a local rule workspace, without a local target config, and without a documented repo-local flow for generating its own README.

## Scope

This spec defines the desired local rule workspace for `memory-viewers/memory-api/`:

- a repo-local `.rule/` store
- a repo-local `rule-targets.yaml`
- canonical local rule entries for the `memory-api` README
- a documented local generation workflow

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
2. Preview the README target with `rule explain-target --config rule-targets.yaml --target memory-api-readme`.
3. Regenerate with `rule sync-targets --config rule-targets.yaml`.
4. Validate the generated `README.md` and any README-target tests before review.

## Acceptance Criteria

- `memory-viewers/memory-api/` contains a repo-local rule workspace and `rule-targets.yaml`.
- The README target is defined locally and renders to `memory-viewers/memory-api/README.md`.
- The generated README is fully reproducible from local rule entries.
- The implementation includes validation coverage for the target config, generated output tracking, and any local README-specific rendering rules.
