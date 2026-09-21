# Problem

The sibling ticket/spec CLIs make automation harder than necessary because their command grammar and JSON envelopes drift in incompatible ways.

In this session:

- `spec refs validate <id>` failed because the accepted order was `spec refs <id> validate`
- `ticket create --json` returned the id at `.payload.id`
- `spec create --json` returned the id at a different top-level location

These are small inconsistencies individually, but together they force every integration to special-case sibling tools that should feel closely related.

# Session Evidence

- The first `spec refs validate ...` invocation failed and had to be retried with reordered positional arguments.
- Ticket and spec creation outputs were parsed differently in the same session because the JSON envelopes did not line up.
- The mismatch was severe enough that it was called out explicitly in the postmortem findings.

# Scope

1. Normalize command grammar for sibling ticket/spec CLI flows where the current ordering is a positional trap.
2. Add backward-compatible aliases or migration handling for existing scripts where necessary.
3. Standardize JSON envelopes for create/get/update/search-style outputs, or version them explicitly if full alignment is not possible.
4. Add contract tests so ticket/spec CLI output does not drift again silently.
5. Document the canonical grammar and the compatibility story.

# Regression Validation Requirements

- **Specification / docs:** define the canonical sibling CLI grammar and the JSON envelope policy.
- **CLI:** add regression coverage for the accepted command forms and the normalized JSON shape.
- **Compatibility:** cover at least one old form and one new/canonical form if aliases are retained.
- **Automation:** include a test that extracts created ids from both tools without tool-specific ad hoc parsing.
- **Manual validation:** repeat the session flow that created and validated specs/tickets and confirm no argument-order or JSON-shape surprises remain.

# Acceptance Criteria

- The canonical ticket/spec CLI grammar is documented and test-covered.
- The `refs validate` flow no longer depends on a positional trap.
- Ticket and spec create outputs expose ids through one documented contract or a clearly versioned compatibility layer.
- Contract tests fail if ticket/spec CLI envelopes drift apart again.
- Existing users get either compatibility aliases or an explicit migration path.

# Likely Surfaces

- `tools/ticket-cli/`
- `tools/spec-cli/`
- `crates/ticket-api/`
- `crates/spec-api/`
- `memory-api/.spec/`
