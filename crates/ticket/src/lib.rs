pub use ticket_api::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reexports_the_internal_api() {
        // Verify the public API is accessible
        let _ = storage::TicketStore::default;
    }
}
