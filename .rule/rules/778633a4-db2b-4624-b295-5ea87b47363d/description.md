# ticket-http

HTTP interface for `ticket-api`.

## Interface

`ticket-http` is the standalone Axum server for ticket reads, graph traversal, and authenticated write operations.

Runtime options:

- `--host <addr>`: bind host, default `127.0.0.1`
- `--port <port>`: bind port, default `4000`
- `--index-root <path>`: open a specific ticket store instead of resolving the nearest `.ticket`
- `--workspace <name>`: accepted for launcher compatibility with viewer flows

Read routes:

- `/healthz`
- `/api/workspaces`
- `/api/tickets`, `/api/tickets/{id}`, `/api/tickets/{id}/description`, `/api/tickets/{id}/history`, `/api/tickets/{id}/files`, `/api/tickets/{id}/asset`
- `/api/edges`
- `/api/schema`, `/api/schema/{type_id}`
- `/api/graph/subgraph`, `/api/graph/topgraph`, `/api/graph/health`
- `/api/stream`

Write routes are auth-gated and include `/api/batch`, ticket create/update/delete, close/cancel/undo/revert, and edge add/remove.

## Usage

Run the server from a checkout of `memory-viewers/memory-api`:

```bash
cargo run -p ticket-http -- --host 127.0.0.1 --port 4000
```

`ticket-http` resolves its store from `--index-root` or the nearest `.ticket` workspace.

## Examples

```bash
# Liveness probe
curl http://127.0.0.1:4000/healthz

# Discover workspaces
curl http://127.0.0.1:4000/api/workspaces

# Inspect registered schemas
curl http://127.0.0.1:4000/api/schema
```
