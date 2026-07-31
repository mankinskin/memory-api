# Shared id/prefix resolver

All `memory-api` stores share one id-or-prefix resolver. Whether the caller is `ticket-cli`, `spec-mcp`, `rule-http`, or a viewer, the rules for accepting `<full-uuid>` versus `<8-char-prefix>` versus a slug are identical.

## Inputs accepted

- A full canonical UUID (lower-case, hyphenated): always matches exactly one entity if it exists.
- A prefix of the UUID (4 characters or more): must resolve to exactly one entity in the store, otherwise the resolver fails with `code: not_found` (no matches) or `code: conflict` (multiple matches).
- A hierarchical slug (specs and rules only): matches the entity whose `slug` field equals the input.

## Failure modes

- Inputs shorter than the minimum prefix length are rejected with `code: invalid_request`.
- Ambiguous prefixes are rejected with `code: conflict` and `details` listing the matching ids so callers can disambiguate.
- The resolver is read-only; it never mutates the store and never creates entities on miss.

## Why this matters

Without a shared resolver, every CLI/MCP/HTTP surface drifts in subtle ways (different minimum prefix lengths, different handling of slugs, different error codes). Pinning the resolver to `<x>-api` keeps the public contract identical across all transports.
