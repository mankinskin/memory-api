# Backup and Restore: Index and History

## Current State (post Phase 1 audit)

The `.ticket/` directory has two distinct layers:

### Source of truth (text, track in git):
- `.ticket/tickets/<uuid>/ticket.toml` — machine-writable TOML manifest
- `.ticket/tickets/<uuid>/description.md` — free-form body text

### Derived artifacts (binary, DO NOT track in git):
- `.ticket/tickets.redb` — redb KV index; rebuilt by `ticket scan --reindex`
- `.ticket/search_index/` — Tantivy FTS index; rebuilt by `ticket scan --reindex`

## Recommended Persistence Strategy

### For a single project (current):
1. Add `.ticket/.gitignore` excluding `tickets.redb`, `search_index/`, `**/.ticket-lock`
2. Track only `tickets/**` in the main repo
3. On fresh checkout: run `ticket scan --reindex` to rebuild the derived indexes

### For cross-project or multi-team sharing:
- Extract `.ticket/` into its own git repo and reference it as a submodule
- This enables independent push/pull history for tickets vs. code
- Each `ticket update` or `ticket create` becomes an atomic commit to the tickets repo
- A CI hook can auto-commit ticket changes with a standard message like `ticket: update <uuid> → <state>`

### Alternative: dedicated remote without submodule complexity
- Keep `.ticket/` in the main repo but configure a separate push remote for it:
  ```bash
  git subtree push --prefix=.ticket origin tickets-branch
  ```
- Simpler than submodules, still enables independent ticket history

## Acceptance Criteria (refined)

1. `.ticket/.gitignore` excludes `tickets.redb`, `search_index/`, `**/.ticket-lock` ✅ (done)
2. `ticket scan --reindex` correctly rebuilds all indexes from text files on disk ✅ (done — stale-entry bug fixed)
3. On fresh clone: documented onboarding step `ticket scan --reindex` in `ticket-system.prompt.md`
4. Optional: `ticket git-commit` subcommand that atomically commits the last ticket change to a configurable remote

## Open work

- [ ] Document `ticket scan --reindex` as the fresh-checkout onboarding step in the skill file
- [ ] Evaluate whether a `ticket git-commit` convenience command is worth implementing vs. leaving to the user's git workflow
