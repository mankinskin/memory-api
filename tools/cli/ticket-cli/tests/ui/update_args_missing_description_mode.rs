// AC5 of ticket 3d952036: constructing `UpdateArgs` without
// `description_update` must fail to compile with a missing-field error.
fn main() {
    let _ = ticket_cli::cli::UpdateArgs {
        id: "abc".to_string(),
        transition_states: Vec::new(),
        to_state: None,
        single_hop: false,
        fields: Vec::new(),
        undo: false,
        author: None,
        board_check_in: false,
        board_agent: None,
        board_intent: None,
        board_files: Vec::new(),
        board_ttl_secs: None,
    };
}
