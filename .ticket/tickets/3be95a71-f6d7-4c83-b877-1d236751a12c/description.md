## Goal
Turn the wired-but-empty entity graph into durable, queryable content. The session-construction machinery (epic effba966) assembles per-session context from rules/specs/tickets; this epic fills those entities with real, idiomatic substance and closes the feedback ring so the system improves itself.

## Why this epic exists
Repository audit + store discovery (2026-07-10/11) confirmed: 60 specs are nearly all `draft` module-name stubs mirroring the code tree (no positions, no guards, no motivation); there is no live tracked work on Rust code-design policy or the feedback ring; the "instruction build system" pre-renders one generic guidance blob instead of per-session assembly. The scaffolding is real; the substance is missing. This epic materializes the substance.

## Depends on (consumes, does not rebuild)
- effba966 [session-bootstrap][epic] — session construction primitives (init/pin/unpin, cascade gathering, rule-URN shape, session_context schema). This epic renders enriched specs/rules/tickets through that constructor.

## Workstreams (children)
- G-A Spec-contract v2: extend `aligned-structure:v1` to require motivation→user-requirement + feedback links, dependent-expectation clause, declared guard test-collection with computed `verified` state, and per-symbol positions (implemented/partial/not-implemented/deprecated).
- G-B Rust code-design policy: typed errors (no stringly markers), trait-based contracts over runtime plumbing, trait inheritance for generic typing; mine context-stack exemplars; retro-fix MoveError::Domain marker and runtime validate_interoperability_contract.
- G-C Rule-introduces-spec policy: a governing rule must present each spec in-prompt, conditioned on implementation status (implemented / partial-with-gaps / coming-soon) — forces policy coverage for specced features.
- G-D Feedback ring: execution-outcome → spec `verified` recompute; transcript mining for tool/rule confusion; missing-rule auto-ticketing; user-interview + web-frontend feedback capture; close the ticket-entity-feedback gap.
- G-E De-submodularization (gated behind G-A..G-D): replace nested submodule links with package/install-path deps on remote releases; keep context-engine as a collection repo for now.

## Acceptance criteria
- All five workstream children exist with depends_on edges and this epic depends_on effba966.
- G-E is gated behind G-A..G-D via depends_on edges.
- Spec-contract v2 (G-A) is authored as a spec and referenced by a workflow policy explaining idiomatic use.
- G-B retro-fix tickets link the concrete first instances (INTEROP db9bad13 / TRACKER 6e72756f).