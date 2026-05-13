# Hierarchical DAG

## Objective

Implement parent-child spec relationships as a DAG (no duplication of specification content). Each spec declares its parent; the system builds a tree with cross-references via edges.

## Rules

1. **Single parent**: Each spec has at most one `parent` field
2. **No cycles**: Parent chain is acyclic (enforced by edge system)
3. **No duplication**: Child specs can reference parent content without repeating it
4. **Cross-refs via edges**: Specs in different subtrees linked with `linked` or `depends_on` edges

## Operations

- `spec tree` — display full hierarchy
- `spec tree <slug>` — display subtree from given root
- `spec ancestors <slug>` — show parent chain
- `spec children <slug>` — list direct children

## Acceptance Criteria

- [ ] Parent-child relationships stored in spec manifest
- [ ] Tree traversal operations (subtree, ancestors, children)
- [ ] `parent_of` edge kind enforced as acyclic
- [ ] CLI tree rendering with indentation