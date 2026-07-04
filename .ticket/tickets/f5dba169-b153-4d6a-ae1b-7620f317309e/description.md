# Goal
Make the move and health test surface deliberately hostile so slow or pathological behavior shows up early.

# Concrete scenarios
- Shared tracked files rewritten by multiple sequential moves.
- Missing tracked path reference files and manual-followup paths.
- Large root-store ticket catalogs with many unrelated tickets.
- Cross-worktree moves with many visible and invisible related entities.
- `health --all` over stores with wide state/component mixes and many graph edges.

# Acceptance criteria
- [ ] Failure-path scenarios are encoded in tests or fixtures rather than ad hoc debugging.
- [ ] Slow-path scenarios can be re-run locally to compare before/after optimization work.