<!-- aligned-structure:v1 -->

# Summary

Bootstrapped from source analysis.

## Behavior Story

Bootstrapped from source analysis.

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# spec-cli

Bootstrapped from source analysis.

## Create Command

`spec create --root` does not place the spec folder at the literal path the
caller passes.

The create flow delegates target-root normalization to `SpecStore::create`, so
workspace roots, the local `.spec` store root, and paths inside that store all
create under `.spec/specs`.

Targets outside the registered scan roots and outside the local `.spec` store
must be rejected.

See child specs for individual module documentation.
