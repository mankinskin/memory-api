<!-- aligned-structure:v1 -->

# Summary

Ticket discovery is split across several partial query mechanisms with inconsistent ordering. This spec defines one canonical, expressive query and ordering contract for ticket discovery so that the shared `memory-api` query layer, `ticket-api` workflow surfaces, and the CLI / MCP / HTTP adapters speak a single query language instead of inventing per-surface filter flags.

## Behavior Story

Ticket discovery is split across several partial query mechanisms with inconsistent ordering. This spec defines one canonical, expressive query and ordering contract for ticket discovery so that the shared `memory-api` query layer, `ticket-api` workflow surfaces, and the CLI / MCP / HTTP adapters speak a single query language instead of inventing per-surface filter flags.

## Provided Surface Contracts

- Define provided contracts for this behavior slice.

## Required Validation

- Triangulate behavior with executable checks, natural-language clauses, and code/schema/API references when available.

## Related Implementation Tickets

- No related implementation ticket is linked yet.

## Background Knowledge References

- Prefer entity references and context rendering over embedding fully expanded payloads in this spec body.

## Legacy Content (Preserved)

# Summary

Ticket discovery is split across several partial query mechanisms with
inconsistent ordering. This spec defines one canonical, expressive query and
ordering contract for ticket discovery so that the shared `memory-api` query
layer, `ticket-api` workflow surfaces, and the CLI / MCP / HTTP adapters speak
a single query language instead of inventing per-surface filter flags.

The contract is intentionally close to a Rust iterator / pipeline mental model:
**select first, then order, then truncate**. Selection narrows the candidate
set; ordering arranges the surviving candidates; an optional limit truncates the
ordered stream. Every stage has deterministic, fully specified behavior.

This spec owns the *query language and ordering composition* contract. It does
**not** redefine the workflow ranking already specified by the best-next and
blocker-tree specs; it defines how caller-supplied ordering composes with that
existing workflow ranking, and where workflow ranking stays authoritative.

## Goals

- Define one canonical query AST and surface-independent grammar for ticket
  discovery.
- Specify the full operator set: logical combinators, comparison operators,
  deep-field path addressing, text search, and existence / null handling.
- Specify explicit ordering: order keys, direction, multi-key lexicographic
  chaining, and deterministic tie-breaks.
- Specify exactly how explicit ordering composes with the existing workflow
  ranking on `next` and `board` surfaces, and where workflow ranking remains
  authoritative.
- Define a transport-safe representation so CLI, MCP, and HTTP express the same
  query without divergent field names.
- Provide a validation matrix spanning parser, storage/search evaluation,
  adapter parity, and workflow-surface regression.

## Non-Goals

- Redefining the workflow convergence ranking comparator. That is owned by
  `ticket-api/workflow/best-next-ordering` and
  `ticket-api/workflow/blocker-trees-and-recently-unblocked-ordering`.
- Changing board-aware exclusion / WIP semantics.
- Introducing a new persistence format or a second search index.
- SQL-like joins across tickets. The query model addresses a single ticket's
  fields and its derived workflow facts only.

## Current State

- `memory-api` query AST (`Expr`) already supports `And`, `Or`, `Not`, `Fts`,
  and `Field { key, value }` where `value` is `ValueExpr::Text` or
  `ValueExpr::Range { start, end }`.
- The shared parser supports space-separated `AND`, `OR` groups, dash-prefixed
  `NOT`, quoted phrases, bare full-text tokens, `key:value` field predicates,
  and `[start TO end]` ranges. Strict mode validates known fields and the
  `x_<type>_<field>` dynamic namespace.
- Tantivy translation in `storage/search.rs` maps `And`/`Or`/`Not`/`Fts`/`Field`
  to boolean queries, but `ValueExpr::Range` currently degrades to `AllQuery`
  (range is parsed but not evaluated), and only `state`, `type`, `id`, `title`
  map to indexed fields.
- `ticket list --where key=value` supports only repeated equality filters.
- `ticket next` / `board show` apply workflow convergence ranking and have no
  caller-specified ordering.

This spec extends that foundation with comparison operators, deep-field
addressing, evaluated ranges, and an explicit ordering clause.

## Query Model

### Pipeline semantics

A ticket query is evaluated as a pipeline:

1. **Scope** — surface-specific candidate set (e.g. all tickets for `list`,
   actionable tickets for `next`, a subgraph for tree surfaces).
2. **Select** — apply the boolean predicate tree to retain matching tickets.
3. **Order** — arrange survivors by the effective order keys (see Ordering).
4. **Limit** — truncate to `limit` if supplied.

Selection never reorders; ordering never filters; limit never reorders. A
ticket either matches the predicate or it does not — partial matches do not
exist.

### Predicate AST

The canonical AST is an extension of the existing `Expr`:

```
Expr ::= And(Vec<Expr>)
       | Or(Vec<Expr>)
       | Not(Box<Expr>)
       | Fts(String)
       | Field { path: FieldPath, op: CompareOp, value: ValueExpr }

CompareOp ::= Eq | Contains | Gt | Gte | Lt | Lte | Range | Exists
ValueExpr ::= Text(String) | Range { start, end } | Empty   // Empty used by Exists
```

`Field.key` is generalized to `FieldPath` (see Deep Fields). The existing
`Expr::Field { key, value }` shape is preserved as the `Eq` case so prior
parser output and tests remain valid; new operators add variants rather than
replacing the equality form.

### Logical combinators

- `and` — implicit between space-separated terms; all sub-expressions must
  match. Empty `And` matches every ticket (identity).
- `or` — the `OR` keyword (case-insensitive) separates disjunction groups; at
  least one group must match. Each group is itself an implicit `And`. `OR` must
  separate two non-empty expressions.
- `not` — a leading `-` on a term, or the explicit `NOT`/`not` keyword,
  negates the immediately following term. `not` over a non-matching ticket
  yields a match; `not` over an absent field follows the existence rules below.
- Precedence: `not` binds tightest, then implicit `and`, then `or` (lowest).
  Parentheses are **not** required for the supported surfaces; disjunction is
  expressed at the top level via `OR` groups. If grouped parentheses are later
  added, they bind tighter than `or` and looser than a single term.

### Comparison operators

Field predicates use `key<op>value` token forms. The canonical operator tokens
and their `key:value`-compatible spellings are:

| Operator | Token form        | Meaning                                             |
|----------|-------------------|-----------------------------------------------------|
| `Eq`     | `key:value`       | Exact (case-insensitive for enum/string fields).    |
| `Contains` | `key:~value` or `key:*value*` | Substring match on the field's text value. |
| `Gt`     | `key:>value`      | Strictly greater than.                              |
| `Gte`    | `key:>=value`     | Greater than or equal.                              |
| `Lt`     | `key:<value`      | Strictly less than.                                 |
| `Lte`    | `key:<=value`     | Less than or equal.                                 |
| `Range`  | `key:[a TO b]`    | Inclusive range `a <= field <= b`.                  |
| `Exists` | `key:?`           | Field is present and non-empty.                     |

- The canonical token form for `Contains` is `key:~value`. `key:*value*` is an
  accepted alias. `~` was chosen so substring stays distinct from the existing
  full-text `Fts` clause.
- Comparison ordering for a field is determined by its declared type:
  - **temporal** fields (`created_at`, `became_actionable_at`,
    `last_blocker_progress_at`, dynamic `x_*` date fields): compared as RFC3339
    timestamps, chronologically.
  - **numeric** fields (effort/token budget, `dependee_count`, dynamic numeric
    `x_*` fields): compared numerically.
  - **ordinal enum** fields (`state` via schema state index, `priority` via the
    `critical > high > medium > low > none` order): compared by their declared
    rank, not lexically.
  - **string** fields: compared by case-insensitive lexicographic order.
- A comparison against a value that cannot be parsed for the field's type is a
  query error with a deterministic message, not a silent no-match.

### Text search

- A bare token (or quoted phrase) is an `Fts` clause: full-text + substring
  search over title and body (plus id substring), matching current behavior.
- `Fts` is distinct from `Contains`: `Fts` searches indexed text fields and
  tokenizes; `Contains` is a substring predicate on one named field's raw
  value.
- Quoted phrases preserve spaces and are matched as a phrase.

### Deep fields and field paths

- A `FieldPath` is one or more dot-separated segments: `segment(.segment)*`.
- The first segment selects a top-level addressable field or a dynamic
  namespace root.
- **Dynamic / structured metadata** uses the existing `x_<type>_<field>`
  namespace as the canonical flat form. Dotted addressing `x.<type>.<field>` is
  an accepted equivalent that normalizes to the same flat key, so storage and
  index keys stay stable.
- Unknown top-level fields fail in strict mode with the existing deterministic
  hint pointing at known fields and the dynamic namespace.
- Deep paths that address a missing intermediate segment are treated as a
  missing field (see Null Handling), never a parse error, once the leading
  segment is a valid namespace.

### Null / missing-field handling

- `key:value` (`Eq`) against a missing field does **not** match.
- `key:?` (`Exists`) matches only when the field is present and non-empty.
- `-key:?` / `not key:?` matches tickets where the field is absent or empty.
- Comparison operators (`>`, `>=`, `<`, `<=`, range) against a missing field do
  **not** match (a missing value is neither greater nor less than anything).
- `Not` over a predicate matches every ticket for which the inner predicate
  does not match, including tickets missing the field — except `not key:?`,
  which is the canonical "field absent" form described above.

## Ordering

### Explicit order clause

- An explicit order is a sequence of order keys: `order:field[:dir]` repeated,
  or a comma list `order:f1:asc,f2:desc`.
- `dir` is `asc` or `desc`; default is `asc` for string/temporal/numeric and
  ascending-by-rank for ordinal enums (lowest rank first) unless `desc`.
- Multi-key ordering composes **lexicographically**: tickets are compared by the
  first key; ties fall through to the second key; and so on. This mirrors Rust's
  tuple `Ord` / chained `then_with`.
- Comparison per key uses the same field type rules as the comparison operators
  (temporal, numeric, ordinal enum, string).
- The final, always-applied deterministic tie-break is ticket `id`, ensuring a
  stable total order even when all user keys tie.

### Default ordering per surface

- `list`: when no explicit order is given, default ordering is ascending effort
  (token budget), preserving today's behavior, then the `id` tie-break.
- `search`: default ordering is relevance score descending, then `id`.
- `next` and `board` recommendations: default ordering is the **workflow
  ranking comparator** owned by `best-next-ordering` /
  `blocker-trees-and-recently-unblocked-ordering`. This spec does not change
  those keys.

### Composition of explicit order with workflow ranking

The key design decision this spec locks:

- On **non-workflow surfaces** (`list`, `search`), an explicit order clause is
  fully authoritative and replaces the default ordering. Workflow ranking is not
  involved.
- On **workflow surfaces** (`next`, `board` recommendations, MCP `next_tickets`,
  and any HTTP workflow-next surface):
  - The default remains the workflow comparator.
  - An explicit order clause is treated as a **secondary refinement that runs
    after** the workflow comparator: candidates are first ordered by the
    authoritative workflow comparator, and the explicit keys act only as
    additional tie-breakers *before* the final `id` tie-break.
  - Callers cannot use explicit ordering to override convergence-pressure
    ranking on workflow surfaces; doing so would silently defeat the dependency
    contract. If a caller needs to fully reorder actionable tickets by a raw
    field, they should query the `list` surface scoped to actionable state
    instead.
- The selection (predicate) part of the query is always fully honored on every
  surface; only the *ordering* portion is constrained on workflow surfaces.

This keeps the system dependency-first where that is the contract, while still
letting callers add deterministic secondary ordering and use the full
expressive predicate language everywhere.

## Transport-Safe Representation

The same query must be expressible across CLI, MCP, and HTTP without each
adapter inventing field names.

- **Canonical string form**: the surface-independent grammar above
  (`key:op value`, `OR`, `-`, `order:f:dir`, `[a TO b]`). Every adapter MUST
  accept this string form for the query and the order clause.
- **CLI**: `--query "<canonical string>"` is the primary surface. The legacy
  `--where key=value` repeated flag remains an equality-only shorthand that
  lowers into `And(Field{Eq})` for backward compatibility. `--order f:dir`
  repeatable or `--order f1:asc,f2:desc`.
- **MCP**: a single `query` string field plus an optional `order` string field
  on ticket query / workflow request types. No per-operator structured fields;
  the string grammar is the contract.
- **HTTP**: a `q` (or `query`) query-string parameter carrying the canonical
  string, plus an optional `order` parameter. Range and comparison tokens must
  be URL-safe; adapters are responsible for standard percent-decoding before
  parsing.
- Field names in the query are the canonical field keys (e.g. `state`,
  `priority`, `created_at`, `x_<type>_<field>`); adapters MUST NOT expose
  aliases that are not defined here.

## Validation Matrix

| Layer | What is validated | Example evidence |
|-------|-------------------|------------------|
| Parser | Combinators, comparison tokens, deep-field paths, ranges, existence, error messages | `cargo test -p ticket-cli --test contracts_query_parser` |
| Storage / search | Tantivy translation of each operator incl. evaluated ranges, comparisons, deep fields, null handling | `cargo test -p memory-api search` / storage eval tests |
| Ordering | Multi-key lexicographic order, direction, type-aware comparison, `id` tie-break | `cargo test -p ticket-cli` ordering tests |
| Adapter parity | Same canonical string yields same result set + order via CLI, MCP, HTTP | `cargo test -p ticket-cli`, `cargo test -p ticket-mcp`, `cargo test -p ticket-http` |
| Workflow composition | Explicit order refines but does not override workflow comparator on `next`/`board`; predicate fully honored | `cargo test -p ticket-cli` next/board regression |

## Acceptance Criteria

- The spec names every supported logical operator, comparison operator,
  deep-field path rule, text-search behavior, and null/existence rule.
- The spec defines explicit ordering semantics (keys, direction, lexicographic
  multi-key chaining, deterministic `id` tie-break) and how they compose with
  the existing workflow ranking, including which surfaces keep workflow ranking
  authoritative.
- The spec identifies which existing surfaces reuse the shared query model
  (`list`, `search`, `next`, `board`, MCP, HTTP) versus which retain
  specialized default ordering (workflow surfaces).
- The spec defines the transport-safe canonical string form and per-adapter
  representation so CLI, MCP, and HTTP express the same query without divergent
  field names.
- The spec includes a validation matrix spanning parser, storage/search,
  adapter parity, and workflow-surface regression.

## Traceability

- Tracker ticket: `.ticket/tickets/7fc7a10d-64a1-4c67-a5a9-5b45d8e03047/ticket.toml`
- Spec ticket (this contract): `.ticket/tickets/f6aa9048-c300-4f64-bf20-157d439dd7ca/ticket.toml`
- Implementation ticket: `.ticket/tickets/8ab31960-f3fa-4a2b-b2ac-f807e1a15fdc/ticket.toml`
- Prerequisite ticket: `.ticket/tickets/68e3c713-3c35-4d7e-af0c-b4a55a3253c0/ticket.toml`

## Related Specs

- `memory-api/model/query` — the query AST and parser this contract extends.
- `memory-api/storage/search` — the Tantivy translation that must evaluate the
  new operators (notably real range/comparison evaluation).
- `ticket-api/workflow/best-next-ordering` — authoritative workflow comparator
  for `next` / `board`; this spec defers to it.
- `ticket-api/workflow/blocker-trees-and-recently-unblocked-ordering` — workflow
  tree ordering; unchanged by this spec.

## Code References

- `crates/memory-api/src/model/query.rs` — `Expr`, `ValueExpr`, `parse_query`,
  `parse_query_strict`, `is_valid_dynamic_field_key`.
- `crates/memory-api/src/storage/search.rs` — `expr_to_query`,
  `field_expr_to_query`, `search_field_for_key` (range currently degrades to
  `AllQuery`; must evaluate comparisons/ranges).
- `tools/cli/ticket-cli/src/cli/commands/ops/next.rs` — `sort_candidates`
  (workflow comparator that explicit order must defer to).
- `tools/cli/ticket-cli/tests/contracts_query_parser.rs` — parser contract
  tests to extend.
