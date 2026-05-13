# Phase 1: Schema Improvements

## Objective

Add 7 new fields to `crates/ticket-api/schemas/tracker-improvement.toml` to replace 30+ ad-hoc fields with structured, filterable schema fields.

## New Fields

| Field | Type | Values | Purpose |
|-------|------|--------|---------|
| `doc_category` | string | `plan\|bug-report\|design\|interview\|research\|guide\|analysis\|impl-record` | Document type classification |
| `tags` | string | JSON array | Frontmatter tags for search (e.g. `["#context-api","#architecture"]`) |
| `workflow_stage` | string | `design\|plan\|implementation\|validation\|done` | Formalize existing ad-hoc field (31 tickets already use this) |
| `priority` | string | `low\|medium\|high\|critical` | Urgency axis (separate from `risk_level`) |
| `source_agent_files` | string | JSON array | Original agent file paths for provenance |
| `bug_validity` | string | `not_confirmed\|confirmed` | Bug validity status — `confirmed` when latest reproduction succeeded |
| `phase` | string | Free-form | Multi-phase plan identifier (e.g. `phase-3.1`) |

## Steps

1. Edit `crates/ticket-api/schemas/tracker-improvement.toml` — add 7 `[fields.*]` entries
2. Run `cargo check -p ticket-api` — verify schema loads correctly
3. Run `ticket scan --reindex` — verify existing tickets are unaffected
4. Verify: `ticket list --state open` still returns 44 tickets

## Risks

- **Low:** `workflow_stage` already exists as ad-hoc field on 31 tickets. Formalizing it in the schema should be transparent since `serde(flatten)` handles both schema and ad-hoc fields the same way.
