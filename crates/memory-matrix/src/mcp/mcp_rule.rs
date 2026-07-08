use super::*;

pub(super) fn dispatch_rule_mcp(
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
