// Typed HTTP client for the ticket-viewer REST API.
// Response shapes mirror tools/viewer/ticket-viewer/frontend/ts/src/types.ts.

export interface WorkspaceInfo {
  name: string;
}

export interface WorkspacesResponse {
  request_id: string;
  workspaces: WorkspaceInfo[];
}

export interface TicketSummary {
  id: string;
  type: string;
  title: string | null;
  state: string | null;
  created_at: string;
  updated_at: string;
  fields: Record<string, unknown>;
}

export interface TicketsResponse {
  request_id: string;
  workspace: string;
  items: TicketSummary[];
  next_cursor: string | null;
}

async function apiFetch<T>(url: string): Promise<T> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), 6000);
  try {
    const res = await fetch(url, { signal: controller.signal });
    if (!res.ok) {
      const body = await res.text().catch(() => res.statusText);
      throw new Error(`HTTP ${res.status}: ${body}`);
    }
    return res.json() as Promise<T>;
  } finally {
    clearTimeout(timeoutId);
  }
}

export async function fetchWorkspaces(baseUrl: string): Promise<WorkspaceInfo[]> {
  const data = await apiFetch<WorkspacesResponse>(`${baseUrl}/api/workspaces`);
  return data.workspaces;
}

export async function fetchTickets(
  baseUrl: string,
  workspace: string,
  cursor?: string,
): Promise<TicketsResponse> {
  const params = new URLSearchParams({ workspace, limit: '500' });
  if (cursor) { params.set('cursor', cursor); }
  return apiFetch<TicketsResponse>(`${baseUrl}/api/tickets?${params}`);
}

/** Fetches all tickets across all cursor pages. */
export async function fetchAllTickets(
  baseUrl: string,
  workspace: string,
): Promise<TicketSummary[]> {
  const all: TicketSummary[] = [];
  let cursor: string | undefined;
  do {
    const page = await fetchTickets(baseUrl, workspace, cursor);
    all.push(...page.items);
    cursor = page.next_cursor ?? undefined;
  } while (cursor);
  return all;
}

export interface EdgeRecord {
  from: string;
  to: string;
  kind: string;
}

export interface EdgesResponse {
  request_id: string;
  workspace: string;
  items: EdgeRecord[];
}

/** Fetches all edges, optionally filtered by kind (e.g. "depends_on"). */
export async function fetchEdges(
  baseUrl: string,
  workspace: string,
  kind?: string,
): Promise<EdgeRecord[]> {
  const params = new URLSearchParams({ workspace });
  if (kind) { params.set('kind', kind); }
  const data = await apiFetch<EdgesResponse>(`${baseUrl}/api/edges?${params}`);
  return data.items;
}

export interface TicketDescriptionResponse {
  request_id: string;
  workspace: string;
  id: string;
  description: string | null;
}

export async function fetchTicketDescription(
  baseUrl: string,
  workspace: string,
  ticketId: string,
): Promise<string | null> {
  const params = new URLSearchParams({ workspace });
  const data = await apiFetch<TicketDescriptionResponse>(
    `${baseUrl}/api/tickets/${encodeURIComponent(ticketId)}/description?${params}`,
  );
  return data.description;
}

// ── Mutations ─────────────────────────────────────────────────────────────────

export interface TicketMutationResponse {
  request_id: string;
  workspace: string;
  ticket: { id: string; fields: Record<string, unknown>; };
}

async function apiMutate<T>(
  method: 'POST' | 'PATCH' | 'DELETE',
  url: string,
  body?: unknown,
): Promise<T> {
  const controller = new AbortController();
  const timeoutId = setTimeout(() => controller.abort(), 10000);
  try {
    const init: RequestInit = { method, signal: controller.signal };
    if (body !== undefined) {
      init.headers = { 'Content-Type': 'application/json' };
      init.body = JSON.stringify(body);
    }
    const res = await fetch(url, init);
    if (!res.ok) {
      const text = await res.text().catch(() => res.statusText);
      throw new Error(`HTTP ${res.status}: ${text}`);
    }
    return res.json() as Promise<T>;
  } finally {
    clearTimeout(timeoutId);
  }
}

export async function createTicket(
  baseUrl: string,
  workspace: string,
  typeId: string,
  title: string,
  description?: string,
): Promise<TicketMutationResponse> {
  const params = new URLSearchParams({ workspace });
  return apiMutate<TicketMutationResponse>('POST', `${baseUrl}/api/tickets?${params}`, {
    type: typeId, title, description,
  });
}

export async function updateTicket(
  baseUrl: string,
  workspace: string,
  id: string,
  body: { state?: string; fields?: Record<string, unknown>; description?: string; },
): Promise<TicketMutationResponse> {
  const params = new URLSearchParams({ workspace });
  return apiMutate<TicketMutationResponse>(
    'PATCH', `${baseUrl}/api/tickets/${encodeURIComponent(id)}?${params}`, body,
  );
}

export async function closeTicket(
  baseUrl: string,
  workspace: string,
  id: string,
  targetState?: string,
): Promise<TicketMutationResponse> {
  const params = new URLSearchParams({ workspace });
  return apiMutate<TicketMutationResponse>(
    'POST', `${baseUrl}/api/tickets/${encodeURIComponent(id)}/close?${params}`,
    targetState ? { target_state: targetState } : {},
  );
}

export async function cancelTicket(
  baseUrl: string,
  workspace: string,
  id: string,
  reason?: string,
): Promise<TicketMutationResponse> {
  const params = new URLSearchParams({ workspace });
  return apiMutate<TicketMutationResponse>(
    'POST', `${baseUrl}/api/tickets/${encodeURIComponent(id)}/cancel?${params}`, { reason },
  );
}

export async function undoTicket(
  baseUrl: string,
  workspace: string,
  id: string,
): Promise<TicketMutationResponse> {
  const params = new URLSearchParams({ workspace });
  return apiMutate<TicketMutationResponse>(
    'POST', `${baseUrl}/api/tickets/${encodeURIComponent(id)}/undo?${params}`, {},
  );
}

export async function deleteTicket(
  baseUrl: string,
  workspace: string,
  id: string,
): Promise<{ request_id: string; workspace: string; id: string; }> {
  const params = new URLSearchParams({ workspace });
  return apiMutate<{ request_id: string; workspace: string; id: string; }>(
    'DELETE', `${baseUrl}/api/tickets/${encodeURIComponent(id)}?${params}`,
  );
}

export async function addEdge(
  baseUrl: string,
  workspace: string,
  fromId: string,
  toId: string,
  kind: string,
  reason?: string,
): Promise<void> {
  const params = new URLSearchParams({ workspace });
  await apiMutate<unknown>('POST', `${baseUrl}/api/edges?${params}`, {
    from_id: fromId, to_id: toId, kind, reason,
  });
}

// ── Schema ────────────────────────────────────────────────────────────────────

export interface TypeSchema {
  type_id: string;
  states: string[];
  transitions: Array<{ from: string; to: string }>;
  required_states: string[];
  terminal_states: string[];
}

interface SchemaListResponse {
  request_id: string;
  workspace: string;
  types: TypeSchema[];
}

export async function fetchSchemas(
  baseUrl: string,
  workspace: string,
): Promise<TypeSchema[]> {
  const params = new URLSearchParams({ workspace });
  const data = await apiFetch<SchemaListResponse>(`${baseUrl}/api/schema?${params}`);
  return data.types;
}
