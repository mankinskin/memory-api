## Payload conventions

- List responses use `payload.items: [ ... ]` plus `payload.count` and, when applicable, paging cursors.
- Single-entity responses use `payload.<entity>` (for example `payload.ticket`, `payload.spec`, `payload.rule`) carrying the canonical fields the store returns.
- Mutating commands echo `payload.id`, `payload.state`, and (when produced by the call) the canonical folder/path so traceability links can be composed without a follow-up `get`.