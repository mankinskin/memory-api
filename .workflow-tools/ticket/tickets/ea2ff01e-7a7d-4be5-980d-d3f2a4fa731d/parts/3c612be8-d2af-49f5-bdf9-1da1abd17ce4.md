## Validation Plan

```bash
cargo test -p pdf-api --lib create_programmatic
cargo test -p pdf-api --lib create_from_typst
```
The typst-path test must explicitly detect `typst-cli` availability in the test itself and report a skip (not a silent pass) when unavailable, per this ticket's acceptance criteria.