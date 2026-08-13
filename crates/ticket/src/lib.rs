#![recursion_limit = "256"]

extern crate self as viewer_api;

#[cfg(feature = "http")]
pub mod auth;
#[cfg(feature = "cli")]
pub mod cli;
#[cfg(feature = "http")]
pub mod error;
#[cfg(feature = "http")]
pub mod middleware;
pub use ticket_api::*;
#[cfg(feature = "http")]
pub mod serve;
#[cfg(feature = "mcp")]
pub mod server;

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn reexports_the_internal_api() {
        // Verify the public API is accessible and the store still requires an explicit root.
        let _ = storage::TicketStore::open as fn(&Path) -> _;
    }
}
