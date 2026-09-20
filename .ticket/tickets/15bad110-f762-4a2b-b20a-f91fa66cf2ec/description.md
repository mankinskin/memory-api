Prune worthless specs and consolidate ultra-granular ones. Reviewable, destructive — never run during a planning pass.

Anchor spec: agents/skill-infrastructure (a9b7ef39) — AC6. STRICTLY LAST.

Scope:
- Delete test-fixture specs: `fixture/root`, `fixture/submodule-a`, `fixture/submodule-b`, and any contentless stubs.
- Consolidate ultra-granular doorknob specs (e.g. `spec-http/error`, `spec-http/state`, arg/route-level fragments) into their parent specs.
- Do NOT touch real system specs (spec-api, ticket-api, memory-api, rule-api, feedback-api, doc-api families) except where consolidating their own doorknob children.

Method: produce the candidate delete/merge list first, get review, then execute via spec-api (spec_delete / merge into parent sections). Keep the candidate list + rationale in the ticket.

Acceptance criteria (verifiable):
- AC-1: Candidate list (delete vs consolidate) is recorded and reviewed before any deletion.
- AC-2: All `fixture/*` specs removed; `spec_list` shows none.
- AC-3: Targeted ultra-granular specs merged into parents; no orphaned dangling references remain (spec health passes).
- AC-4: No real system spec lost (before/after spec count reconciled against the candidate list).