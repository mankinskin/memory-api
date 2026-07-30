## One-way semantics

- Forward transitions are linear (`open → planned → in-implementation → in-review → done`, with type-specific variations).
- Backward transitions are explicit and audited: `ticket update --undo` rewinds to the previous history record; there is no "set state back to X" operation that bypasses history.
- Entities created directly in a non-initial state cannot be `--undo`'d because there is no prior history record. Authoring tools should create entities in the initial state and then transition them forward.