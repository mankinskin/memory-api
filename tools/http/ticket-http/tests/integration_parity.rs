//! Cross-interface parity tests for workflow and health surfaces.
//!
//! These tests verify that the HTTP, MCP, and ticket-api (the common data
//! layer) surfaces produce equivalent results from the same shared fixture
//! store. Assertions ignore documented transport-envelope differences:
//!
//! ## Documented transport-envelope differences (not tested for parity)
//!
//! | Feature                  | ticket-api | HTTP | MCP  |
//! |--------------------------|------------|------|------|
//! | `scope` metadata field   | n/a        | ✓    | ✓    |
//!
//! Transport-local envelopes remain different, but board-aware workflow-next
//! semantics are shared across the ticket-api helper, HTTP, and MCP.
//!
//! ## Parity contract (what IS guaranteed equivalent)
//!
//! - Given the same fixture store, the set of actionable candidate IDs and
//!   their sort order are equal across ticket-api, HTTP, and MCP (when no
//!   board exclusion applies).
//! - Health finding `check` keys, `severity` values, and `ticket_id` targets
//!   are identical across ticket-api, HTTP, and MCP.
//! - `scope.active_index_root` is present and non-empty for HTTP and MCP.

#[path = "integration_parity/fixture.rs"]
mod integration_parity_fixture;

#[path = "integration_parity/tests.rs"]
mod tests;
