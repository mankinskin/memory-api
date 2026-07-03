use super::*;

pub(super) fn dispatch_mcp(
    domain: &str,
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    match domain {
        "ticket" => dispatch_ticket_mcp(operation, ctx),
        "spec" => dispatch_spec_mcp(operation, ctx),
        "rule" => dispatch_rule_mcp(operation, ctx),
        _ => blocked(format!(
            "mcp transport for domain `{domain}` operation `{operation}` is not wired yet"
        )),
    }
}

fn extract_mcp_json(
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

fn dispatch_ticket_mcp(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    if operation == "get" {
        return dispatch_ticket_mcp_stdio_sentinel_get(ctx);
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
            "create" => {
                let created = server
                    .create_ticket(Parameters(CreateTicketInput {
                        workspace: workspace_root.clone(),
                        type_id: "tracker-improvement".to_string(),
                        title: Some(title),
                        state: Some("new".to_string()),
                        fields: vec![],
                        description: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp ticket create call failed: {err}"))?;
                let json = extract_mcp_json(created)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp ticket create returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "get" => {
                let created = server
                    .create_ticket(Parameters(CreateTicketInput {
                        workspace: workspace_root.clone(),
                        type_id: "tracker-improvement".to_string(),
                        title: Some(title),
                        state: Some("new".to_string()),
                        fields: vec![],
                        description: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for get failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp create result missing id".to_string())?
                    .to_string();

                let result = server
                    .get_ticket(Parameters(TicketRefInput {
                        workspace: workspace_root,
                        id: created_id.clone(),
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
            },
            "search" => {
                let created = server
                    .create_ticket(Parameters(CreateTicketInput {
                        workspace: workspace_root.clone(),
                        type_id: "tracker-improvement".to_string(),
                        title: Some(title.clone()),
                        state: Some("new".to_string()),
                        fields: vec![],
                        description: None,
                    }))
                    .await
                    .map_err(|err| {
                        format!("mcp seed create for search failed: {err}")
                    })?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp create result missing id".to_string())?
                    .to_string();

                let result = server
                    .list_tickets(Parameters(ListTicketsInput {
                        workspace: workspace_root,
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
                    item["id"].as_str().map(|value| value == created_id).unwrap_or(false)
                });
                if !found {
                    return Err(format!(
                        "mcp ticket search did not return seeded ticket id {created_id}"
                    ));
                }
                Ok(Cell::Passed)
            },
            "update" => {
                let created = server
                    .create_ticket(Parameters(CreateTicketInput {
                        workspace: workspace_root.clone(),
                        type_id: "tracker-improvement".to_string(),
                        title: Some(title),
                        state: Some("new".to_string()),
                        fields: vec![],
                        description: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for update failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp create result missing id".to_string())?
                    .to_string();

                let updated = server
                    .update_ticket(Parameters(UpdateTicketInput {
                        workspace: workspace_root,
                        id: created_id,
                        transition_states: vec![],
                        to_state: Some("ready".to_string()),
                        fields: None,
                        field_map: None,
                        undo: false,
                        description: None,
                        author: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp ticket update call failed: {err}"))?;
                let json = extract_mcp_json(updated)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp ticket update returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "delete" => {
                let created = server
                    .create_ticket(Parameters(CreateTicketInput {
                        workspace: workspace_root.clone(),
                        type_id: "tracker-improvement".to_string(),
                        title: Some(title),
                        state: Some("new".to_string()),
                        fields: vec![],
                        description: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for delete failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp create result missing id".to_string())?
                    .to_string();

                let deleted = server
                    .delete_ticket(Parameters(DeleteTicketInput {
                        workspace: workspace_root,
                        id: created_id,
                    }))
                    .await
                    .map_err(|err| format!("mcp ticket delete call failed: {err}"))?;
                let json = extract_mcp_json(deleted)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp ticket delete returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            _ => blocked(format!(
                "mcp transport for domain `ticket` operation `{operation}` is not wired yet"
            )),
        }
    })
}

struct StdioMcpClient {
    child: std::process::Child,
    next_id: u64,
}

impl StdioMcpClient {
    fn spawn_ticket_mcp(store_root: &std::path::Path) -> Result<Self, String> {
        let mcp_workspace_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let mut cmd = Command::new("cargo");
        cmd.args(["run", "-p", "ticket-mcp", "--quiet"])
            .current_dir(mcp_workspace_root)
            .env("TICKET_INDEX_ROOT", store_root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = cmd.spawn().map_err(|err| {
            format!("spawn ticket-mcp stdio sentinel process: {err}")
        })?;

        Ok(Self { child, next_id: 1 })
    }

    fn initialize(&mut self) -> Result<(), String> {
        let _ = self.request(
            "initialize",
            serde_json::json!({
                "protocolVersion": "2025-03-26",
                "capabilities": {},
                "clientInfo": {
                    "name": "memory-matrix-sentinel",
                    "version": "0.1.0"
                }
            }),
        )?;

        self.send_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized",
            "params": {}
        }))
    }

    fn request(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        let id = self.next_id;
        self.next_id += 1;

        self.send_message(&serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        }))?;

        loop {
            let message = self.read_message()?;
            if message["id"].as_u64() != Some(id) {
                continue;
            }

            if message.get("error").is_some() {
                return Err(format!(
                    "mcp `{method}` returned error: {}",
                    message["error"]
                ));
            }
            return Ok(message["result"].clone());
        }
    }

    fn send_message(
        &mut self,
        message: &serde_json::Value,
    ) -> Result<(), String> {
        let mut payload = serde_json::to_vec(message)
            .map_err(|err| format!("serialize mcp message: {err}"))?;
        payload.push(b'\n');
        let stdin = self
            .child
            .stdin
            .as_mut()
            .ok_or_else(|| "ticket-mcp stdin not available".to_string())?;
        stdin
            .write_all(&payload)
            .map_err(|err| format!("write mcp payload: {err}"))?;
        stdin
            .flush()
            .map_err(|err| format!("flush mcp payload: {err}"))
    }

    fn read_message(&mut self) -> Result<serde_json::Value, String> {
        let stdout = self
            .child
            .stdout
            .as_mut()
            .ok_or_else(|| "ticket-mcp stdout not available".to_string())?;

        let mut payload = Vec::new();
        loop {
            let mut byte = [0u8; 1];
            if let Err(err) = stdout.read_exact(&mut byte) {
                let mut stderr_tail = String::new();
                let status = self.child.wait().ok();
                if let Some(stderr) = self.child.stderr.as_mut() {
                    let _ = stderr.read_to_string(&mut stderr_tail);
                }
                let status_note = status
                    .map(|value| format!("; child status: {value}"))
                    .unwrap_or_default();
                let stderr_note = if stderr_tail.trim().is_empty() {
                    "".to_string()
                } else {
                    format!("; child stderr: {}", stderr_tail.trim())
                };
                return Err(format!(
                    "read mcp message: {err}{status_note}{stderr_note}"
                ));
            }

            if byte[0] == b'\n' {
                break;
            }
            payload.push(byte[0]);
        }

        if payload.is_empty() {
            return self.read_message();
        }

        serde_json::from_slice(&payload)
            .map_err(|err| format!("parse mcp json line: {err}"))
    }
}

impl Drop for StdioMcpClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn extract_stdio_tool_json(
    result: &serde_json::Value
) -> Result<serde_json::Value, String> {
    let text = result["content"]
        .as_array()
        .and_then(|content| {
            content.iter().find_map(|entry| {
                let is_text = entry["type"].as_str() == Some("text");
                if is_text {
                    entry["text"].as_str().map(ToString::to_string)
                } else {
                    None
                }
            })
        })
        .ok_or_else(|| {
            "mcp tools/call result missing text content".to_string()
        })?;

    serde_json::from_str(&text)
        .map_err(|err| format!("parse mcp tools/call text payload: {err}"))
}

fn dispatch_ticket_mcp_stdio_sentinel_get(ctx: &MatrixCtx) -> CellResult {
    let mut client =
        StdioMcpClient::spawn_ticket_mcp(&ctx.store_root(".ticket"))?;
    client.initialize()?;

    let workspace_root = ctx.workspace_root.to_string_lossy().to_string();
    let title =
        format!("matrix-mcp-stdio-ticket-{}", uuid::Uuid::new_v4().simple());

    let create_result = client.request(
        "tools/call",
        serde_json::json!({
            "name": "create_ticket",
            "arguments": {
                "workspace": workspace_root,
                "type": "tracker-improvement",
                "title": title,
                "state": "new",
                "fields": []
            }
        }),
    )?;
    let create_json = extract_stdio_tool_json(&create_result)?;
    if create_json["status"].as_str().unwrap_or_default() != "ok" {
        return Err(format!(
            "mcp stdio sentinel create_ticket returned non-ok status: {}",
            create_json
        ));
    }
    let created_id = create_json["id"]
        .as_str()
        .ok_or_else(|| {
            "mcp stdio sentinel create_ticket missing id".to_string()
        })?
        .to_string();

    let get_result = client.request(
        "tools/call",
        serde_json::json!({
            "name": "get_ticket",
            "arguments": {
                "workspace": workspace_root,
                "id": created_id
            }
        }),
    )?;
    let get_json = extract_stdio_tool_json(&get_result)?;
    let returned_id = get_json["ticket"]["id"].as_str().ok_or_else(|| {
        "mcp stdio sentinel get_ticket missing ticket.id".to_string()
    })?;
    if returned_id != created_id {
        return Err(format!(
            "mcp stdio sentinel get_ticket returned mismatched id: expected {created_id}, got {returned_id}"
        ));
    }

    Ok(Cell::Passed)
}

fn dispatch_spec_mcp(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            format!("build tokio runtime for mcp matrix cell: {err}")
        })?;

    let workspace_root = ctx.workspace_root.to_string_lossy().to_string();
    let server = SpecServer::new(ctx.store_root(".spec"));
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let slug = format!("matrix/mcp/spec-{suffix}");
    let title = format!("Matrix MCP Spec {suffix}");

    runtime.block_on(async move {
        match operation {
            "create" => {
                let created = server
                    .spec_create(Parameters(CreateSpecInput {
                        workspace: workspace_root,
                        title,
                        slug,
                        component: "matrix".to_string(),
                        parent: None,
                        scope: Some("internal".to_string()),
                        body: Some("matrix mcp body".to_string()),
                        fields: BTreeMap::new(),
                    }))
                    .await
                    .map_err(|err| format!("mcp spec create call failed: {err}"))?;
                let json = extract_mcp_json(created)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp spec create returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "get" => {
                let created = server
                    .spec_create(Parameters(CreateSpecInput {
                        workspace: workspace_root.clone(),
                        title,
                        slug,
                        component: "matrix".to_string(),
                        parent: None,
                        scope: Some("internal".to_string()),
                        body: Some("matrix mcp body".to_string()),
                        fields: BTreeMap::new(),
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for spec get failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp spec create result missing id".to_string())?
                    .to_string();

                let result = server
                    .spec_get(Parameters(GetSpecInput {
                        workspace: Some(workspace_root),
                        id: created_id.clone(),
                        full: false,
                    }))
                    .await
                    .map_err(|err| format!("mcp spec get call failed: {err}"))?;
                let json = extract_mcp_json(result)?;
                let returned_id = json["spec"]["id"]
                    .as_str()
                    .ok_or_else(|| "mcp spec get result missing spec.id".to_string())?;
                if returned_id != created_id {
                    return Err(format!(
                        "mcp spec get returned mismatched id: expected {created_id}, got {returned_id}"
                    ));
                }
                Ok(Cell::Passed)
            },
            "search" => {
                let created = server
                    .spec_create(Parameters(CreateSpecInput {
                        workspace: workspace_root.clone(),
                        title: title.clone(),
                        slug,
                        component: "matrix".to_string(),
                        parent: None,
                        scope: Some("internal".to_string()),
                        body: Some("matrix mcp body".to_string()),
                        fields: BTreeMap::new(),
                    }))
                    .await
                    .map_err(|err| {
                        format!("mcp seed create for spec search failed: {err}")
                    })?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp spec create result missing id".to_string())?
                    .to_string();

                let result = server
                    .spec_search(Parameters(SearchSpecsInput {
                        workspace: Some(workspace_root),
                        query: title,
                        limit: 10,
                    }))
                    .await
                    .map_err(|err| format!("mcp spec search call failed: {err}"))?;
                let json = extract_mcp_json(result)?;
                let items = json["items"]
                    .as_array()
                    .ok_or_else(|| "mcp spec search result missing items".to_string())?;
                let found = items.iter().any(|item| {
                    item["id"]
                        .as_str()
                        .map(|value| value == created_id)
                        .unwrap_or(false)
                });
                if !found {
                    return Err(format!(
                        "mcp spec search did not return seeded spec id {created_id}"
                    ));
                }
                Ok(Cell::Passed)
            },
            "update" => {
                let created = server
                    .spec_create(Parameters(CreateSpecInput {
                        workspace: workspace_root.clone(),
                        title,
                        slug,
                        component: "matrix".to_string(),
                        parent: None,
                        scope: Some("internal".to_string()),
                        body: Some("matrix mcp body".to_string()),
                        fields: BTreeMap::new(),
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for spec update failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp spec create result missing id".to_string())?
                    .to_string();

                let updated = server
                    .spec_update(Parameters(UpdateSpecInput {
                        workspace: Some(workspace_root),
                        id: created_id,
                        fields: Some(vec!["title=Matrix MCP Updated".to_string()]),
                        to_state: None,
                        body: None,
                        field_map: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp spec update call failed: {err}"))?;
                let json = extract_mcp_json(updated)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp spec update returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "delete" => {
                let created = server
                    .spec_create(Parameters(CreateSpecInput {
                        workspace: workspace_root.clone(),
                        title,
                        slug,
                        component: "matrix".to_string(),
                        parent: None,
                        scope: Some("internal".to_string()),
                        body: Some("matrix mcp body".to_string()),
                        fields: BTreeMap::new(),
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for spec delete failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp spec create result missing id".to_string())?
                    .to_string();

                let deleted = server
                    .spec_delete(Parameters(SpecRefInput {
                        workspace: Some(workspace_root),
                        id: created_id,
                    }))
                    .await
                    .map_err(|err| format!("mcp spec delete call failed: {err}"))?;
                let json = extract_mcp_json(deleted)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp spec delete returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "scan" => {
                let scanned = server
                    .spec_scan(Parameters(SpecScanInput {
                        workspace: Some(workspace_root),
                        force: false,
                    }))
                    .await
                    .map_err(|err| format!("mcp spec scan call failed: {err}"))?;
                let json = extract_mcp_json(scanned)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp spec scan returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            _ => blocked(format!(
                "mcp transport for domain `spec` operation `{operation}` is not wired yet"
            )),
        }
    })
}

fn dispatch_rule_mcp(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            format!("build tokio runtime for mcp matrix cell: {err}")
        })?;

    let workspace_root = ctx.workspace_root.to_string_lossy().to_string();
    let server = RuleServer::new(ctx.store_root(".rule"));
    let suffix = uuid::Uuid::new_v4().simple().to_string();
    let slug = format!("matrix/mcp/rule-{suffix}");
    let title = format!("Matrix MCP Rule {suffix}");

    runtime.block_on(async move {
        match operation {
            "create" => {
                let created = server
                    .rule_create(Parameters(CreateRuleInput {
                        workspace: workspace_root,
                        title,
                        slug,
                        file_kind: "markdown".to_string(),
                        section: "matrix".to_string(),
                        body: Some("matrix mcp body".to_string()),
                        repo_scope: vec![],
                        path_scope: vec![],
                        order_key: None,
                        source_repo: None,
                        source_path: None,
                        source_start_line: None,
                        source_end_line: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp rule create call failed: {err}"))?;
                let json = extract_mcp_json(created)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp rule create returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "get" => {
                let created = server
                    .rule_create(Parameters(CreateRuleInput {
                        workspace: workspace_root,
                        title,
                        slug,
                        file_kind: "markdown".to_string(),
                        section: "matrix".to_string(),
                        body: Some("matrix mcp body".to_string()),
                        repo_scope: vec![],
                        path_scope: vec![],
                        order_key: None,
                        source_repo: None,
                        source_path: None,
                        source_start_line: None,
                        source_end_line: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for rule get failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp rule create result missing id".to_string())?
                    .to_string();

                let result = server
                    .rule_get(Parameters(RuleRefInput { id: created_id.clone() }))
                    .await
                    .map_err(|err| format!("mcp rule get call failed: {err}"))?;
                let json = extract_mcp_json(result)?;
                let returned_id = json["rule"]["id"]
                    .as_str()
                    .ok_or_else(|| "mcp rule get result missing rule.id".to_string())?;
                if returned_id != created_id {
                    return Err(format!(
                        "mcp rule get returned mismatched id: expected {created_id}, got {returned_id}"
                    ));
                }
                Ok(Cell::Passed)
            },
            "search" => {
                let created = server
                    .rule_create(Parameters(CreateRuleInput {
                        workspace: workspace_root,
                        title: title.clone(),
                        slug,
                        file_kind: "markdown".to_string(),
                        section: "matrix".to_string(),
                        body: Some("matrix mcp body".to_string()),
                        repo_scope: vec![],
                        path_scope: vec![],
                        order_key: None,
                        source_repo: None,
                        source_path: None,
                        source_start_line: None,
                        source_end_line: None,
                    }))
                    .await
                    .map_err(|err| {
                        format!("mcp seed create for rule search failed: {err}")
                    })?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp rule create result missing id".to_string())?
                    .to_string();

                let result = server
                    .rule_search(Parameters(SearchRulesInput {
                        query: title,
                        state: None,
                        file_kind: None,
                        section: None,
                        repo_scope: None,
                        path_scope: None,
                        slug: None,
                        low_rated_only: false,
                        unresolved_only: false,
                        limit: 10,
                    }))
                    .await
                    .map_err(|err| format!("mcp rule search call failed: {err}"))?;
                let json = extract_mcp_json(result)?;
                let items = json["items"]
                    .as_array()
                    .ok_or_else(|| "mcp rule search result missing items".to_string())?;
                let found = items.iter().any(|item| {
                    item["id"]
                        .as_str()
                        .map(|value| value == created_id)
                        .unwrap_or(false)
                });
                if !found {
                    return Err(format!(
                        "mcp rule search did not return seeded rule id {created_id}"
                    ));
                }
                Ok(Cell::Passed)
            },
            "update" => {
                let created = server
                    .rule_create(Parameters(CreateRuleInput {
                        workspace: workspace_root,
                        title,
                        slug,
                        file_kind: "markdown".to_string(),
                        section: "matrix".to_string(),
                        body: Some("matrix mcp body".to_string()),
                        repo_scope: vec![],
                        path_scope: vec![],
                        order_key: None,
                        source_repo: None,
                        source_path: None,
                        source_start_line: None,
                        source_end_line: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp seed create for rule update failed: {err}"))?;
                let created_json = extract_mcp_json(created)?;
                let created_id = created_json["id"]
                    .as_str()
                    .ok_or_else(|| "mcp rule create result missing id".to_string())?
                    .to_string();

                let updated = server
                    .rule_update(Parameters(UpdateRuleInput {
                        id: created_id,
                        fields: Some(vec!["title=Matrix MCP Updated Rule".to_string()]),
                        field_map: None,
                        to_state: None,
                        body: None,
                    }))
                    .await
                    .map_err(|err| format!("mcp rule update call failed: {err}"))?;
                let json = extract_mcp_json(updated)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp rule update returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            "scan" => {
                let scanned = server
                    .rule_scan(Parameters(RuleScanInput { force: false }))
                    .await
                    .map_err(|err| format!("mcp rule scan call failed: {err}"))?;
                let json = extract_mcp_json(scanned)?;
                let status = json["status"].as_str().unwrap_or_default();
                if status != "ok" {
                    return Err(format!(
                        "mcp rule scan returned non-ok status: {}",
                        json
                    ));
                }
                Ok(Cell::Passed)
            },
            _ => blocked(format!(
                "mcp transport for domain `rule` operation `{operation}` is not wired yet"
            )),
        }
    })
}
