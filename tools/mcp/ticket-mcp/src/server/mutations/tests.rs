use super::*;

#[tokio::test]
async fn create_ticket_tool_rejects_default_workspace_alias() {
    let tmp = tempfile::TempDir::new().expect("tempdir");
    let server = TicketServer::new(tmp.path().to_path_buf());

    let result = server
        .create_ticket_tool(CreateTicketInput {
            workspace: "default".to_string(),
            type_id: "tracker-improvement".to_string(),
            title: Some("Wrong workspace".to_string()),
            state: None,
            fields: vec![],
            description: None,
        })
        .await;

    assert!(result.is_err());
}
