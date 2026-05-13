# spec-http: HTTP Endpoints

## Objective

Add HTTP endpoints for spec-api, either as part of ticket-http or as a separate spec-http crate. Routes follow the same pattern as ticket-http.

## Endpoints

```
POST   /api/specs                   → create spec
GET    /api/specs/:id               → get spec (id or slug)
GET    /api/specs/:id/full          → get spec with sections
PATCH  /api/specs/:id               → update spec fields/state
DELETE /api/specs/:id               → soft-delete
GET    /api/specs                   → list specs (?state=&component=&query=)
GET    /api/specs/search?q=...      → full-text search
GET    /api/specs/:id/tree          → hierarchy subtree
GET    /api/specs/:id/refs          → code references
POST   /api/specs/:id/refs/validate → validate code refs
POST   /api/specs/:id/sections      → add section
GET    /api/specs/:id/sections      → list sections
GET    /api/specs/:id/sections/:name → get section content
GET    /api/specs/toc               → table of contents
POST   /api/specs/skill/generate    → generate skill files
GET    /api/specs/health            → health check
```

## Acceptance Criteria

- [ ] All endpoints implemented with proper error handling
- [ ] JSON request/response format
- [ ] Slug resolution in :id parameter
- [ ] CORS headers for browser access
- [ ] Integration tests