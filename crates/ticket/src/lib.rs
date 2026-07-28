pub use ticket_api::*;

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
