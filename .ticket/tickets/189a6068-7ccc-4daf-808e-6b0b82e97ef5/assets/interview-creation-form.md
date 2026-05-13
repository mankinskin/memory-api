# Interview: Ticket Creation — Dynamic Schema Fields

**Date:** 2026-04-08
**Applies to:** `3e069173` (Ticket creation form), `189a6068` (Schema endpoint)

## Question

Should the creation form auto-populate from the schema endpoint (dynamic field generation) or use a hardcoded form for the known `tracker-improvement` type? Should creation support templates?

## Answer

**We should build the fields dynamically from the schema endpoint.**

## Implications

- The creation form must fetch the ticket type schema at runtime from the `GET /api/schema` endpoint
- Fields rendered dynamically based on schema definition: required vs optional, field types, allowed values
- Support for multiple ticket types — the form adapts when new types are added to the schema registry
- Schema must include field metadata: label, type (string, enum, number, boolean), required flag, default value, allowed values
- The Schema endpoint (`189a6068`) becomes a hard prerequisite for the creation form
- Consider caching the schema in the frontend store with a TTL or SSE-based invalidation
- Template support can be built on top: a template is a partial field pre-fill that layers on the schema defaults
