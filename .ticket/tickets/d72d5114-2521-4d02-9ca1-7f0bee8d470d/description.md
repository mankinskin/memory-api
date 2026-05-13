# Spec Search

## Objective

Full-text search across all specs using Tantivy, with field predicates matching the ticket search pattern.

## Query Syntax

```
spec search "entity storage"                    # free-text
spec search "component:ticket-api"              # field predicate
spec search "state:approved component:spec-api" # combined
spec search "scope:function symbol:create"      # code ref search
```

## Acceptance Criteria

- [ ] Tantivy search index configured for spec fields
- [ ] Free-text search across title, body, and section content
- [ ] Field predicates for component, state, scope, slug
- [ ] Search results include slug, title, state, and relevance score