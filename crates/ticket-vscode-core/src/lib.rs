// ticket-vscode-core
//
// Rust/WASM core for the ticket-vscode VS Code extension.
// Compiled to wasm32-unknown-unknown via wasm-pack.
//
// Design contract (frozen in spec ticket-vscode/rust-wasm-port):
// - No `vscode` or Node/browser APIs are imported here.
// - All host interaction goes through capability-object arguments passed in
//   from the JS/TS host shell at activation time.
// - All public functions exported to JS must be annotated #[wasm_bindgen]
//   under the "wasm" feature flag so the crate still compiles and tests
//   natively without the flag.

#[cfg(feature = "wasm")]
use wasm_bindgen::prelude::*;

// ── Smoke-test export ─────────────────────────────────────────────────────────

/// Returns the core library version string.
///
/// Used by the activation spike (ticket 14047b99) to confirm the WASM module
/// loads and a function is callable from both the desktop Node host and the
/// browser WebWorker host.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn core_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

// ── Ticket domain types ───────────────────────────────────────────────────────
//
// These are the data shapes that the JS host shell feeds into the core after
// fetching them from the ticket-viewer HTTP API.  They mirror the response
// shapes in `src/api.ts`.

/// Minimal ticket summary passed from the JS host to the core.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Clone)]
pub struct TicketSummary {
    id: String,
    title: String,
    state: String,
}

#[cfg_attr(feature = "wasm", wasm_bindgen)]
impl TicketSummary {
    #[cfg_attr(feature = "wasm", wasm_bindgen(constructor))]
    pub fn new(id: String, title: String, state: String) -> Self {
        Self { id, title, state }
    }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn id(&self) -> String {
        self.id.clone()
    }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn title(&self) -> String {
        self.title.clone()
    }

    #[cfg_attr(feature = "wasm", wasm_bindgen(getter))]
    pub fn state(&self) -> String {
        self.state.clone()
    }
}

// ── Feature-gate helpers ──────────────────────────────────────────────────────
//
// The core emits boolean gate decisions so the JS shell can show/hide commands
// without embedding host-detection logic in every command handler.

/// Host kind reported by `HostDetectionCapability` in the JS shell.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostKind {
    DesktopNode,
    RemoteWorkspace,
    BrowserWeb,
    Virtual,
}

/// Returns `true` when server-control features (`startServer`, binary spawn)
/// should be available for the given host kind.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn supports_server_control(host: HostKind) -> bool {
    matches!(host, HostKind::DesktopNode | HostKind::RemoteWorkspace)
}

/// Returns `true` when browser-bridge features (`bridge*` commands) should be
/// available for the given host kind.
#[cfg_attr(feature = "wasm", wasm_bindgen)]
pub fn supports_browser_bridge(host: HostKind) -> bool {
    matches!(host, HostKind::DesktopNode)
}

// ── Filtering ─────────────────────────────────────────────────────────────────

/// Returns `true` when a ticket matches both state and query filters.
/// Pure function — no I/O, no VS Code APIs, fully testable in Rust.
pub fn ticket_matches(ticket: &TicketSummary, state_filter: Option<&str>, query: Option<&str>) -> bool {
    if let Some(s) = state_filter {
        if !s.is_empty() && ticket.state != s {
            return false;
        }
    }
    if let Some(q) = query {
        if !q.is_empty() {
            let q_lower = q.to_lowercase();
            if !ticket.title.to_lowercase().contains(&q_lower)
                && !ticket.id.to_lowercase().contains(&q_lower)
            {
                return false;
            }
        }
    }
    true
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_non_empty() {
        assert!(!core_version().is_empty());
    }

    #[test]
    fn filter_by_state() {
        let t = TicketSummary::new("abc".into(), "Add feature".into(), "ready".into());
        assert!(ticket_matches(&t, Some("ready"), None));
        assert!(!ticket_matches(&t, Some("done"), None));
        assert!(ticket_matches(&t, Some(""), None));
        assert!(ticket_matches(&t, None, None));
    }

    #[test]
    fn filter_by_query_title() {
        let t = TicketSummary::new("abc".into(), "Add feature".into(), "ready".into());
        assert!(ticket_matches(&t, None, Some("feature")));
        assert!(ticket_matches(&t, None, Some("FEATURE")));
        assert!(!ticket_matches(&t, None, Some("missing")));
    }

    #[test]
    fn filter_by_query_id() {
        let t = TicketSummary::new("abc123".into(), "Add feature".into(), "ready".into());
        assert!(ticket_matches(&t, None, Some("abc")));
    }

    #[test]
    fn host_kind_gates() {
        assert!(supports_server_control(HostKind::DesktopNode));
        assert!(supports_server_control(HostKind::RemoteWorkspace));
        assert!(!supports_server_control(HostKind::BrowserWeb));
        assert!(!supports_server_control(HostKind::Virtual));

        assert!(supports_browser_bridge(HostKind::DesktopNode));
        assert!(!supports_browser_bridge(HostKind::RemoteWorkspace));
        assert!(!supports_browser_bridge(HostKind::BrowserWeb));
    }
}
