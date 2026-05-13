# Ticket Integration

## Objective

Link specs to tickets bidirectionally. When a ticket implements a spec feature, or a bug is found against a spec, the relationship is tracked.

## Edge Types

- `spec_implements` — ticket implements features described in spec
- `spec_validates` — ticket validates/tests spec acceptance criteria
- `spec_bugfix` — ticket fixes a bug recorded in spec's features.bugs

## Operations

- `spec link --to-ticket <ticket-id> --kind spec_implements`
- `spec tickets <spec-id>` — list linked tickets
- `ticket specs <ticket-id>` — list linked specs (requires ticket-cli update)

## Acceptance Criteria

- [ ] Edge types registered in memory-api edge system
- [ ] Spec-to-ticket linking via CLI and MCP
- [ ] Reverse lookup from ticket to spec
- [ ] Feature status auto-updated when linked ticket closes