1. Documentation: use doc-viewer MCP tools to locate relevant module docs.
2. Known issues/plans: use ticket-mcp tools before duplicating work.
3. Board state: check active WIP, stale entries, and file ownership before touching
   implementation files — `mcp_ticket-mcp_board_show` with `{"workspace": "default"}` or:
   ```bash
   ./target/debug/ticket.exe board show --json
   ```
4. Test failures: use log-viewer MCP tools (`get_log`, `search_all_logs`, `query_logs`).
5. Graph/workspace behavior: use context-mcp tools for context-engine operations.