use memory_fixtures::{
    run_startup_matrix_for,
    startup_matrix_succeeded,
    StartupMatrixClass,
};

#[test]
fn storeless_startup_matrix_leaves_every_mcp_server_unchanged() {
    let results = run_startup_matrix_for(Some(StartupMatrixClass::McpServer))
        .expect("run MCP startup matrix");
    assert!(
        startup_matrix_succeeded(&results),
        "storeless MCP startup matrix failures:\n{results:#?}",
    );
}

#[test]
fn viewer_startup_matrix_keeps_client_log_state_with_path_lazy() {
    let results = run_startup_matrix_for(Some(StartupMatrixClass::Viewer))
        .expect("run viewer startup matrix");
    assert!(
        startup_matrix_succeeded(&results),
        "storeless viewer startup matrix failures:\n{results:#?}",
    );
}
