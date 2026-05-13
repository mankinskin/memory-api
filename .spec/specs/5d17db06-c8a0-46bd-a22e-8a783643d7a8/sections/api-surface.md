All HTTP calls are made from `api.ts`. GET requests use a 6-second `AbortController` timeout; mutation requests (`POST`, `PATCH`, `DELETE`) use 10 seconds. The workspace is always passed as a query parameter (`?workspace=<name>`).

## Read Endpoints

| Method | Path | Query | Description |
|---|---|---|---|
| `GET` | `/api/workspaces` | — | List all known workspace names (`WorkspacesResponse`) |
| `GET` | `/api/tickets` | `workspace`, `limit=500`, `cursor?` | Paginated ticket list (`TicketsResponse`); cursor-based; `fetchAllTickets` drains all pages |
| `GET` | `/api/edges` | `workspace`, `kind?` | Flat edge list (`EdgesResponse`); extension always fetches with `kind=depends_on` |
| `GET` | `/api/schema` | `workspace` | List of `TypeSchema` objects (states, transitions, required_states, terminal_states) |
| `GET` | `/api/tickets/:id/description` | `workspace` | Fetch `description` string for a single ticket (`TicketDescriptionResponse`) |

## Mutation Endpoints

| Method | Path | Query | Body | Description |
|---|---|---|---|---|
| `POST` | `/api/tickets` | `workspace` | `{ type, title, description? }` | Create ticket |
| `PATCH` | `/api/tickets/:id` | `workspace` | `{ state?, fields?, description? }` | Update state or fields |
| `POST` | `/api/tickets/:id/close` | `workspace` | `{ target_state? }` | Fast-forward to done (or target_state) |
| `POST` | `/api/tickets/:id/cancel` | `workspace` | `{ reason? }` | Cancel ticket |
| `POST` | `/api/tickets/:id/undo` | `workspace` | `{}` | Undo last state transition |
| `DELETE` | `/api/tickets/:id` | `workspace` | — | Delete ticket permanently |
| `POST` | `/api/edges` | `workspace` | `{ from_id, to_id, kind, reason? }` | Add edge (dependency) |

## Key Response Types (TypeScript)

```ts
interface TicketSummary {
  id: string;
  type: string;
  title: string | null;
  state: string | null;
  created_at: string;   // ISO 8601
  updated_at: string;
  fields: Record<string, unknown>;
}

interface EdgeRecord {
  from: string;
  to: string;
  kind: string;
}

interface TypeSchema {
  type_id: string;
  states: string[];
  transitions: Array<{ from: string; to: string }>;
  required_states: string[];
  terminal_states: string[];
}
```

## Error Handling

All fetch helpers throw `Error("HTTP <status>: <body>")` on non-2xx responses. Mutation commands in `extension.ts` wrap calls in `runMutation()` which catches and shows `showErrorMessage`. The `load()` method in `TicketTreeProvider` catches errors and transitions the tree to an `'error'` state that renders an `InfoItem` with the failure message and a restart hint.
