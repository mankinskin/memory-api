use super::*;

pub(super) fn extract_mcp_json(
    result: CallToolResult
) -> Result<serde_json::Value, String> {
    let text = result
        .content
        .iter()
        .find_map(|content| {
            if let rmcp::model::RawContent::Text(text) = &content.raw {
                Some(text.text.clone())
            } else {
                None
            }
        })
        .ok_or_else(|| "mcp result missing text content".to_string())?;
    serde_json::from_str(&text)
        .map_err(|err| format!("parse mcp json result: {err}"))
}

pub(super) fn ensure_status_ok(
    json: &serde_json::Value,
    context: &str,
) -> Result<(), String> {
    let status = json["status"].as_str().unwrap_or_default();
    if status != "ok" {
        return Err(format!("{context}: {}", json));
    }
    Ok(())
}

pub(super) fn dispatch_ticket_mcp(
    operation: &str,
    ctx: &MatrixCtx,
    metadata: Option<&DispatchMetadata>,
) -> CellResult {
    if operation == "get" {
        return dispatch_ticket_mcp_stdio_sentinel_get(ctx, metadata);
    }

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            format!("build tokio runtime for mcp matrix cell: {err}")
        })?;

    let workspace_root = ctx.workspace_root.to_string_lossy().to_string();
    let server = TicketServer::new(ctx.store_root(".ticket"));
    let title = format!("matrix-mcp-ticket-{}", uuid::Uuid::new_v4().simple());

    runtime.block_on(async move {
        match operation {
            "create" => ticket_mcp_create(&server, &workspace_root, title).await,
            "get" => ticket_mcp_get(&server, &workspace_root, title).await,
            "search" => ticket_mcp_search(&server, &workspace_root, title).await,
            "update" => ticket_mcp_update(&server, &workspace_root, title).await,
            "delete" => ticket_mcp_delete(&server, &workspace_root, title).await,
            _ => blocked(format!(
                "mcp transport for domain `ticket` operation `{operation}` is not wired yet"
            )),
        }
    })
}

async fn ticket_mcp_seed_create(
    server: &TicketServer,
    workspace_root: &str,
    title: String,
    error_context: &str,
) -> Result<String, String> {
    let created = server
        .create_ticket(Parameters(CreateTicketInput {
            workspace: workspace_root.to_string(),
            type_id: "tracker-improvement".to_string(),
            title: Some(title),
            state: Some("open".to_string()),
            fields: vec![],
            description: None,
        }))
        .await
        .map_err(|err| format!("{error_context}: {err}"))?;
    let created_json = extract_mcp_json(created)?;
    created_json["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "mcp create result missing id".to_string())
}

async fn ticket_mcp_create(
    server: &TicketServer,
    workspace_root: &str,
    title: String,
) -> CellResult {
    let created_id = ticket_mcp_seed_create(
        server,
        workspace_root,
        title,
        "mcp ticket create call failed",
    )
    .await?;
    let result = server
        .get_ticket(Parameters(TicketRefInput {
            workspace: Some(workspace_root.to_string()),
            id: created_id,
            view: None,
            parts: None,
        }))
        .await
        .map_err(|err| {
            format!("mcp ticket create verification failed: {err}")
        })?;
    let json = extract_mcp_json(result)?;
    let _ = json["ticket"]["id"].as_str().ok_or_else(|| {
        "mcp ticket create verification missing ticket.id".to_string()
    })?;
    Ok(Cell::Passed)
}

async fn ticket_mcp_get(
    server: &TicketServer,
    workspace_root: &str,
    title: String,
) -> CellResult {
    let created_id = ticket_mcp_seed_create(
        server,
        workspace_root,
        title,
        "mcp seed create for get failed",
    )
    .await?;

    let result = server
        .get_ticket(Parameters(TicketRefInput {
            workspace: Some(workspace_root.to_string()),
            id: created_id.clone(),
            view: None,
            parts: None,
        }))
        .await
        .map_err(|err| format!("mcp ticket get call failed: {err}"))?;
    let json = extract_mcp_json(result)?;
    let returned_id = json["ticket"]["id"]
        .as_str()
        .ok_or_else(|| "mcp ticket get result missing ticket.id".to_string())?;
    if returned_id != created_id {
        return Err(format!(
            "mcp ticket get returned mismatched id: expected {created_id}, got {returned_id}"
        ));
    }
    Ok(Cell::Passed)
}

async fn ticket_mcp_search(
    server: &TicketServer,
    workspace_root: &str,
    title: String,
) -> CellResult {
    let created_id = ticket_mcp_seed_create(
        server,
        workspace_root,
        title.clone(),
        "mcp seed create for search failed",
    )
    .await?;

    let result = server
        .list_tickets(Parameters(ListTicketsInput {
            workspace: workspace_root.to_string(),
            state: None,
            type_id: None,
            query: Some(title),
            limit: Some(10),
        }))
        .await
        .map_err(|err| format!("mcp ticket list call failed: {err}"))?;
    let json = extract_mcp_json(result)?;
    let items = json["items"]
        .as_array()
        .ok_or_else(|| "mcp ticket list result missing items".to_string())?;
    let found = items.iter().any(|item| {
        item["id"]
            .as_str()
            .map(|value| value == created_id)
            .unwrap_or(false)
    });
    if !found {
        return Err(format!(
            "mcp ticket search did not return seeded ticket id {created_id}"
        ));
    }
    Ok(Cell::Passed)
}

async fn ticket_mcp_update(
    server: &TicketServer,
    workspace_root: &str,
    title: String,
) -> CellResult {
    let created_id = ticket_mcp_seed_create(
        server,
        workspace_root,
        title,
        "mcp seed create for update failed",
    )
    .await?;

    let updated = server
        .update_ticket(Parameters(UpdateTicketInput {
            workspace: workspace_root.to_string(),
            id: created_id,
            transition_states: vec![],
            to_state: Some("planned".to_string()),
            fields: None,
            field_map: None,
            undo: false,
            description_update:
                ticket_api::storage::DescriptionUpdate::Unchanged,
            author: None,
            single_hop: false,
        }))
        .await
        .map_err(|err| format!("mcp ticket update call failed: {err}"))?;
    let json = extract_mcp_json(updated)?;
    ensure_status_ok(&json, "mcp ticket update returned non-ok status")?;
    Ok(Cell::Passed)
}

async fn ticket_mcp_delete(
    server: &TicketServer,
    workspace_root: &str,
    title: String,
) -> CellResult {
    let created_id = ticket_mcp_seed_create(
        server,
        workspace_root,
        title,
        "mcp seed create for delete failed",
    )
    .await?;

    let deleted = server
        .delete_ticket(Parameters(DeleteTicketInput {
            workspace: workspace_root.to_string(),
            id: created_id,
        }))
        .await
        .map_err(|err| format!("mcp ticket delete call failed: {err}"))?;
    let json = extract_mcp_json(deleted)?;
    ensure_status_ok(&json, "mcp ticket delete returned non-ok status")?;
    Ok(Cell::Passed)
}
