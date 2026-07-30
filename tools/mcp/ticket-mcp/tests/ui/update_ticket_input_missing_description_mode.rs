// AC5 of ticket 3d952036: constructing `UpdateTicketInput` without
// `description_update` must fail to compile with a missing-field error.
fn main() {
    let _ = ticket_mcp::server::UpdateTicketInput {
        workspace: "default".to_string(),
        id: "abc".to_string(),
        transition_states: Vec::new(),
        to_state: None,
        fields: None,
        field_map: None,
        undo: false,
        author: None,
        single_hop: false,
    };
}
