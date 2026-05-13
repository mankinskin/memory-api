# Table of Contents

## Objective

Auto-generate a table of contents index showing all specs organized by component and hierarchy.

## Output Format

```markdown
# Specification Table of Contents

## ticket-api (14 specs, 85% implemented)
- ticket-api ← crate overview [approved]
  - ticket-api/model/ticket [implemented]
  - ticket-api/model/schema [implemented]  
  - ticket-api/storage/store [approved]
    - ...

## spec-api (8 specs, 30% implemented)
- spec-api [draft]
  - ...
```

## Acceptance Criteria

- [ ] `spec toc` generates hierarchical TOC
- [ ] Organized by component
- [ ] Shows state and implementation coverage
- [ ] JSON output for programmatic use