## Task Routing

- Any requested implementation or behavior change: create or update the tracking ticket(s) first, then create or update the relevant spec before editing files.
- Simple fix (1-2 files): after the ticket/spec setup when requirements or behavior change, gather context, implement, validate, update docs, verify spec links, and move the ticket to `in-review`.
- Bug fix: after the ticket/spec setup, follow `.github/prompts/debug-test.prompt.md` when available.
- Feature or refactor (>5 files, >100 LOC, or unclear scope): use `.github/prompts/tickets.prompt.md` to establish the ticket set, then `.github/prompts/spec.prompt.md` to update the spec before implementation.
- Unfamiliar module or unclear behavior: follow `.github/prompts/research.prompt.md` when available before locking the spec or implementation plan.
- Swarm execution: use `.github/prompts/swarm-worker.prompt.md`.