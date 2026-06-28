# [test] Transport-layer e2e matrix — CLI/HTTP/MCP × domains

## Goal

Validate each basic operation (`get`/`search`/CRUD/`move`/`scan`) through the **real transport surfaces** of every domain, not just in-process storage, so regressions in `ticket-http`, `spec-mcp`, `test-cli`, argument parsing, JSON/TOON serialization, and HTTP/MCP error mapping are caught.

## Resolved decisions

- **D1 — scope:** CLI + MCP for every domain; HTTP **only where a `*-http` surface exists** (ticket, spec, doc today). Testing-only — we do not build missing transports here (see cancelled `57a13857`).
- **D2 — drive mode:** **mainly in-process** server/router/tool handles for speed and low flake; plus a **small** set of large tests that exercise a **real subprocess** (built CLI binary / bound HTTP port) or a **TypeScript/node e2e** path (the TS path is designed in `93b8a331`).
- **D9 — budgets:** assert **per-operation** latency budgets on the end-user call (see `01964def`).

## Scope

- Add a transport axis: `(domain, operation, transport)`, transport ∈ {cli, http, mcp}, gated by surface existence.
- In-process cells call the transport's request handler/tool dispatch directly against the representative fixture (`9138f4e7`).
- A curated handful of large cells spawn the real binary / bind a real port to prove the in-process path isn't hiding wiring/serialization faults.
- Missing `(domain, transport)` pairs are **Blocked** with a reason citing the out-of-scope D8 decision — never silently skipped.
- Record each cell as a `ValidationExecution` tagged with transport + typed provenance (`a03d8a97`).

## Brutally-honest expectations

- If a transport bug makes a cell fail, **leave it failing** and open a fault ticket — do not relax the assertion.
- Include at least one **fault-injection** test proving a broken serialization/handler actually fails a cell.

## Acceptance criteria

- [ ] Every in-scope `(domain, operation, transport)` cell drives the real surface and records an execution with transport tag + duration + provenance.
- [ ] A deliberately broken transport makes a cell fail (fault-injection proof).
- [ ] At least one large subprocess/real-port cell exists per transport kind that has a surface.
- [ ] Missing transports are Blocked-with-reason citing D8; none are silently skipped.
- [ ] A single documented command runs the matrix.

## Relationship / traceability

- Depends on provenance (`a03d8a97`), representative fixture (`9138f4e7`).
- TS/subprocess large-test path informed by design session `93b8a331`.
- Reuses the `DomainOps`/`cells` shape from `751f0e71`.
