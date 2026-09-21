## Problem

`peek --grep <pattern>` outputs bare line numbers only, with no preview of the matched line text:

```
$ peek memory-api/crates/memory-api/src/storage/entity_store.rs --grep "pub fn"
61
69
94
100
...
```

To read what those lines contain, the user must make a separate `--start N --window M` call per match. This defeats the purpose - with N matches the user needs N+1 round-trips instead of 1.

## Expected behaviour

Each match line should include the line number AND the matched line content, e.g.:

```
	61 | pub fn open(
	69 | pub fn open_with(
	94 | pub fn schema_registry(&self) -> &SchemaRegistry {
```

The existing `--window` combination should still work (show K lines of context around the **first** match), but the bare match-list output should show content.

## Scope

- Update `grep` output mode to print `{line_number:>6} | {line_content}` for every match.
- `--window` context output is unchanged.
- No other flags affected.

## Acceptance criteria

- `peek file.rs --grep "pub fn"` prints line number + content for every match, one per line.
- `peek file.rs --grep "fn foo" --window 10` still prints the context window around the first match (unchanged).
- Output format is stable (suitable for piping).