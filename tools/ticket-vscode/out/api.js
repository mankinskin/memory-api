"use strict";
// Typed HTTP client for the ticket-viewer REST API.
// Response shapes mirror tools/viewer/ticket-viewer/frontend/src/types.ts.
Object.defineProperty(exports, "__esModule", { value: true });
exports.fetchWorkspaces = fetchWorkspaces;
exports.fetchTickets = fetchTickets;
exports.fetchAllTickets = fetchAllTickets;
exports.fetchTicketDescription = fetchTicketDescription;
async function apiFetch(url) {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), 6000);
    try {
        const res = await fetch(url, { signal: controller.signal });
        if (!res.ok) {
            const body = await res.text().catch(() => res.statusText);
            throw new Error(`HTTP ${res.status}: ${body}`);
        }
        return res.json();
    }
    finally {
        clearTimeout(timeoutId);
    }
}
async function fetchWorkspaces(baseUrl) {
    const data = await apiFetch(`${baseUrl}/api/workspaces`);
    return data.workspaces;
}
async function fetchTickets(baseUrl, workspace, cursor) {
    const params = new URLSearchParams({ workspace, limit: '500' });
    if (cursor) {
        params.set('cursor', cursor);
    }
    return apiFetch(`${baseUrl}/api/tickets?${params}`);
}
/** Fetches all tickets across all cursor pages. */
async function fetchAllTickets(baseUrl, workspace) {
    const all = [];
    let cursor;
    do {
        const page = await fetchTickets(baseUrl, workspace, cursor);
        all.push(...page.items);
        cursor = page.next_cursor ?? undefined;
    } while (cursor);
    return all;
}
async function fetchTicketDescription(baseUrl, workspace, ticketId) {
    const params = new URLSearchParams({ workspace });
    const data = await apiFetch(`${baseUrl}/api/tickets/${encodeURIComponent(ticketId)}/description?${params}`);
    return data.description;
}
//# sourceMappingURL=api.js.map