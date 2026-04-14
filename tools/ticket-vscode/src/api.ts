// Typed HTTP client for the ticket-viewer REST API.
// Response shapes mirror tools/viewer/ticket-viewer/frontend/src/types.ts.

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
