// AC5 of ticket 3d952036: constructing `UpdateTicketBody` without
// `description_update` must fail to compile with a missing-field error.
fn main() {
    let _ = ticket::serve::handlers::tickets::types::UpdateTicketBody {
        fields: None,
        state: None,
        transition_states: Vec::new(),
        single_hop: false,
    };
}
