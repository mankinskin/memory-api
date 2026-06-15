//! Domain index generators for memory-api store domains.
//!
//! Each sub-module provides a generator function that:
//! 1. Reads the relevant domain store (tickets, specs, rules, audit findings, workspaces).
//! 2. Converts each entity into a sealed [`IndexEntry`].
//! 3. Returns an [`IndexSidecar`] ready to write to disk.
//!
//! Generators are intentionally dependency-free: they accept pre-opened store
//! handles and a workspace root path so callers (CLI `store-index` subcommands
//! and git hooks) control the open/write lifecycle.
//!
//! # Generator contract
//!
//! - Every returned `IndexSidecar` has all entries sorted by id (stable diff output).
//! - Every entry is sealed (digest populated).
//! - `source_path` values are workspace-relative with `/` separators.
//! - `generated_at` is set to `Utc::now()` at call time; callers may override it
//!   if they need deterministic timestamps in tests.

pub mod audit;
pub mod spec;
pub mod ticket;
pub mod util;
pub mod workspace;

pub use audit::generate_audit_sidecar;
pub use spec::generate_spec_sidecar;
pub use ticket::generate_ticket_sidecar;
pub use util::{
    deterministic_uuid,
    to_relative_slash,
};
pub use workspace::generate_workspace_sidecar;
