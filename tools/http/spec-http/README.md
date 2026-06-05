<!-- rule-api:file generated=true -->

<!-- rule-api:entry id=b8535c8a-4097-4042-8f2a-745123d269ee slug=memory-api/readme/tools/parent-readme/l1 -->
Back to [memory-api/README.md](../../../README.md).

<!-- rule-api:entry id=986d5653-8f63-44bd-9ee0-ad917d06bd74 slug=memory-api/readme/tools/http/spec-http/l1 -->
# spec-http

HTTP interface for `spec-api`.

## Interface

`spec-http` is the standalone Axum server for listing specs, inspecting trees and sections, validating references, and mutating spec content over HTTP.

Runtime options:

- `--host <addr>`: bind host, default `127.0.0.1`
- `--port <port>`: bind port, default `4001`
- `--index-root <path>`: open a specific spec store instead of using environment or workspace discovery

Store discovery:

- `SPEC_INDEX_ROOT`
- fallback to `TICKET_INDEX_ROOT`
- fallback to the nearest `.spec` workspace

Read routes:

- `/healthz`
- `/api/specs`, `/api/specs/search`, `/api/specs/graph`, `/api/specs/health`, `/api/specs/stream`
- `/api/specs/{id}`, `/api/specs/{id}/full`, `/api/specs/{id}/tree`, `/api/specs/{id}/refs`
- `/api/specs/{id}/sections`, `/api/specs/{id}/sections/{name}`

Write routes include spec create/update/delete, reference validation, section add/delete, scan, and add-root. CORS is enabled for browser clients.

## Usage

Run the server from a checkout of `memory-viewers/memory-api`:

```bash
cargo run -p spec-http -- --host 127.0.0.1 --port 4001
```

## Examples

```bash
# Liveness probe
curl http://127.0.0.1:4001/healthz

# List current specs
curl http://127.0.0.1:4001/api/specs

# Run store health checks
curl http://127.0.0.1:4001/api/specs/health
```
