<!-- aligned-structure:v1 -->

# Summary

Adopt the shared README schema in the aggregate `memory-viewers` repo root and normalize its child blocks after the `memory-api` and `viewer-api` child roots settle.

## Behavior Story

Adopt the shared README schema in the aggregate `memory-viewers` repo root and normalize its child blocks after the `memory-api` and `viewer-api` child roots settle.

## Provided Surface Contracts

- README surfaces in scope follow an explicit rule-backed contract instead of one-off rollout prose.
- Parent and child README navigation stays repo-internal and mechanically derivable.
- README completeness and rollout status are verified by mechanical validation rather than manual review.

## Required Validation

- Contract clause validation: The migrated spec names the intended README structure and navigation behavior as explicit contract properties.
- Contract clause validation: The migrated spec names the validation path that checks the README contract mechanically.
- Contract clause validation: The migrated spec records enough evidence to tell whether the README contract is satisfied or blocked.
- The authored spec body documents the README contract, scope boundaries, and navigation expectations for this migration slice.
- The authored spec body documents the mechanical validation path required to prove this migration slice.
- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# Summary

Adopt the shared README schema in the aggregate `memory-viewers` repo root and normalize its child blocks after the `memory-api` and `viewer-api` child roots settle.

## Problem

`memory-viewers` already aggregates the child repos through imported targets, but its root README still hard-codes its own structure and risks drifting from the child surfaces it is meant to point at.

## Scope

This spec covers:

- adoption of the shared schema for `memory-viewers/README.md`
- normalization of the child blocks that point to `memory-api` and `viewer-api`
- preservation of aggregate-only sections such as screenshots or dependency graphs

## Intended Behavior

- The aggregate `memory-viewers` root README inherits the shared schema.
- Its child blocks reflect the final root surfaces of `memory-api` and `viewer-api`.
- It continues to own aggregate-only sections without flattening child repository ownership.

## Assumptions To Prove

- The aggregate root can follow the child repos rather than redefining them.
- The shared schema is flexible enough to keep optional aggregate-only sections.
- The aggregate README should normalize last in the generated-repo branch.

## Test Strategy

1. Migrate the root `memory-viewers` README target to the shared schema.
2. Refresh its child blocks after the child repo migrations land.
3. Re-run explain and sync checks from the aggregate repo root.

## Acceptance Criteria

- `memory-viewers/README.md` uses the shared README schema.
- Its child blocks align with the final child repo root shapes.
- `sync-targets --check` passes from the `memory-viewers` repo root.

## Traceability

- [26f570e2 memory-viewers rollout ticket](C:/Users/linus_behrbohm/git/SECOND_CHECKOUT/graph_app/.workflow-tools/ticket/tickets/26f570e2-6a2f-4604-9347-a3ac7d0314c3/ticket.toml)
