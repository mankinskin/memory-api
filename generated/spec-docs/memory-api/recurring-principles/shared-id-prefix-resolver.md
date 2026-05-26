<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=76027bee-ca0b-46f6-a2b7-0a97b32433d1 slug=memory-api/recurring-principles/shared-id-prefix-resolver/shared-id-prefix-resolver/l1 -->
# Shared id/prefix resolver

All `memory-api` stores share one id-or-prefix resolver. Whether the caller is `ticket-cli`, `spec-mcp`, `rule-http`, or a viewer, the rules for accepting `<full-uuid>` versus `<8-char-prefix>` versus a slug are identical.

<!-- rule-api:entry id=e44fa332-6828-4ea1-a944-e0f9345a0bca slug=memory-api/recurring-principles/shared-id-prefix-resolver/shared-id-prefix-resolver/inputs-accepted/l5 -->
## Inputs accepted

- A full canonical UUID (lower-case, hyphenated): always matches exactly one entity if it exists.
- A prefix of the UUID (4 characters or more): must resolve to exactly one entity in the store, otherwise the resolver fails with `code: not_found` (no matches) or `code: conflict` (multiple matches).
- A hierarchical slug (specs and rules only): matches the entity whose `slug` field equals the input.

<!-- rule-api:entry id=6965bd9b-be61-438d-b1f0-38401922051a slug=memory-api/recurring-principles/shared-id-prefix-resolver/shared-id-prefix-resolver/failure-modes/l11 -->
## Failure modes

- Inputs shorter than the minimum prefix length are rejected with `code: invalid_request`.
- Ambiguous prefixes are rejected with `code: conflict` and `details` listing the matching ids so callers can disambiguate.
- The resolver is read-only; it never mutates the store and never creates entities on miss.

<!-- rule-api:entry id=7a00552c-63d4-48c9-916d-8bf870dacbda slug=memory-api/recurring-principles/shared-id-prefix-resolver/shared-id-prefix-resolver/why-this-matters/l17 -->
## Why this matters

Without a shared resolver, every CLI/MCP/HTTP surface drifts in subtle ways (different minimum prefix lengths, different handling of slugs, different error codes). Pinning the resolver to `<x>-api` keeps the public contract identical across all transports.
