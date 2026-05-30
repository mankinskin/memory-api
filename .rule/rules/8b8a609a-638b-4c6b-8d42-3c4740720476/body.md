## Implications for callers

- Time travel and audit queries are served by replaying history, not by querying the index.
- `update --undo` requires at least one prior history record; tickets created directly in a non-initial state cannot be undone.
- Any field that needs to be queryable must be projected into the materialised index — adding a field to the model is a two-step change (history first, index projection second).