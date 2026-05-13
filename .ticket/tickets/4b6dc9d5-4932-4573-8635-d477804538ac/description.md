# Spec Schema: Lifecycle State Machine

## Objective

Define the specification type schema with a full lifecycle from draft to verified implementation.

## State Machine

```
draft → reviewed → approved → implemented → verified
  ↓        ↓          ↓           ↓
cancelled cancelled  cancelled  cancelled
                      ↓
                   deprecated
```

### States

| State | Meaning |
|-------|---------|
| `draft` | Initial authoring, incomplete or unreviewed |
| `reviewed` | Peer-reviewed for technical accuracy |
| `approved` | Approved as target specification |
| `implemented` | Implementation matches the spec |
| `verified` | Tests confirm implementation meets spec |
| `deprecated` | Spec is outdated, superseded by another |
| `cancelled` | Spec was abandoned |

### Required States

`required_states = ["reviewed", "approved"]` — specs cannot reach `verified` without passing through review and approval.

## Schema TOML

```toml
type_id = "specification"
states = ["draft", "reviewed", "approved", "implemented", "verified", "deprecated", "cancelled"]
required_states = ["reviewed", "approved"]

[[transitions]]
from = "draft"
to = "reviewed"
# ... full transition list

[fields.title]
field_type = "string"
required = true

[fields.slug]
field_type = "string"
required = true

[fields.component]
field_type = "string"
required = false

[fields.scope]
field_type = "string"
required = false

[fields.parent]
field_type = "string"
required = false

[edge_rules.depends_on]
directed = true
acyclic_enforced = true

[edge_rules.linked]
directed = false
acyclic_enforced = false

[edge_rules.parent_of]
directed = true
acyclic_enforced = true
```

## Acceptance Criteria

- [ ] Schema TOML file at `crates/spec-api/schemas/specification.toml`
- [ ] State machine enforces required_states before terminal
- [ ] All transitions defined and tested
- [ ] Edge rules support depends_on, linked, and parent_of