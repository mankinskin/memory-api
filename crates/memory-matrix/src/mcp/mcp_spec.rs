use super::*;

pub(super) fn dispatch_spec_mcp(
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
            "create" => spec_mcp_create(&server, &workspace_root, &title, &slug).await,
            "get" => spec_mcp_get(&server, &workspace_root, &title, &slug).await,
            "search" => spec_mcp_search(&server, &workspace_root, &title, &slug).await,
            "update" => spec_mcp_update(&server, &workspace_root, &title, &slug).await,
            "delete" => spec_mcp_delete(&server, &workspace_root, &title, &slug).await,
            "scan" => spec_mcp_scan(&server, &workspace_root).await,
            _ => blocked(format!(
                "mcp transport for domain `spec` operation `{operation}` is not wired yet"
            )),
        }
    })
}

async fn spec_mcp_seed_create(
    server: &SpecServer,
    workspace_root: &str,
    title: &str,
    slug: &str,
    error_context: &str,
) -> Result<String, String> {
    let created = server
        .spec_create(Parameters(CreateSpecInput {
            workspace: workspace_root.to_string(),
            title: title.to_string(),
            slug: slug.to_string(),
            component: "matrix".to_string(),
            parent: None,
            scope: Some("internal".to_string()),
            body: Some("matrix mcp body".to_string()),
            fields: BTreeMap::new(),
        }))
        .await
        .map_err(|err| format!("{error_context}: {err}"))?;
    let created_json = extract_mcp_json(created)?;
    created_json["id"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "mcp spec create result missing id".to_string())
}

async fn spec_mcp_create(
    server: &SpecServer,
    workspace_root: &str,
    title: &str,
    slug: &str,
) -> CellResult {
    let created_id = spec_mcp_seed_create(
        server,
        workspace_root,
        title,
        slug,
        "mcp spec create call failed",
    )
    .await?;
    let result = server
        .spec_get(Parameters(GetSpecInput {
            workspace: Some(workspace_root.to_string()),
            id: created_id,
            full: false,
        }))
        .await
        .map_err(|err| format!("mcp spec create verification failed: {err}"))?;
    let json = extract_mcp_json(result)?;
    let _ = json["spec"]["id"].as_str().ok_or_else(|| {
        "mcp spec create verification missing spec.id".to_string()
    })?;
    Ok(Cell::Passed)
}

async fn spec_mcp_get(
    server: &SpecServer,
    workspace_root: &str,
    title: &str,
    slug: &str,
) -> CellResult {
    let created_id = spec_mcp_seed_create(
        server,
        workspace_root,
        title,
        slug,
        "mcp seed create for spec get failed",
    )
    .await?;

    let result = server
        .spec_get(Parameters(GetSpecInput {
            workspace: Some(workspace_root.to_string()),
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
}

async fn spec_mcp_search(
    server: &SpecServer,
    workspace_root: &str,
    title: &str,
    slug: &str,
) -> CellResult {
    let created_id = spec_mcp_seed_create(
        server,
        workspace_root,
        title,
        slug,
        "mcp seed create for spec search failed",
    )
    .await?;

    let result = server
        .spec_search(Parameters(SearchSpecsInput {
            workspace: Some(workspace_root.to_string()),
            query: title.to_string(),
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
}

async fn spec_mcp_update(
    server: &SpecServer,
    workspace_root: &str,
    title: &str,
    slug: &str,
) -> CellResult {
    let created_id = spec_mcp_seed_create(
        server,
        workspace_root,
        title,
        slug,
        "mcp seed create for spec update failed",
    )
    .await?;

    let updated = server
        .spec_update(Parameters(UpdateSpecInput {
            workspace: Some(workspace_root.to_string()),
            id: created_id,
            fields: Some(vec!["title=Matrix MCP Updated".to_string()]),
            to_state: None,
            body: None,
            force_body: false,
            field_map: None,
        }))
        .await
        .map_err(|err| format!("mcp spec update call failed: {err}"))?;
    let json = extract_mcp_json(updated)?;
    ensure_status_ok(&json, "mcp spec update returned non-ok status")?;
    Ok(Cell::Passed)
}

async fn spec_mcp_delete(
    server: &SpecServer,
    workspace_root: &str,
    title: &str,
    slug: &str,
) -> CellResult {
    let created_id = spec_mcp_seed_create(
        server,
        workspace_root,
        title,
        slug,
        "mcp seed create for spec delete failed",
    )
    .await?;

    let deleted = server
        .spec_delete(Parameters(SpecRefInput {
            workspace: Some(workspace_root.to_string()),
            id: created_id,
        }))
        .await
        .map_err(|err| format!("mcp spec delete call failed: {err}"))?;
    let json = extract_mcp_json(deleted)?;
    ensure_status_ok(&json, "mcp spec delete returned non-ok status")?;
    Ok(Cell::Passed)
}

async fn spec_mcp_scan(
    server: &SpecServer,
    workspace_root: &str,
) -> CellResult {
    let scanned = server
        .spec_scan(Parameters(SpecScanInput {
            workspace: Some(workspace_root.to_string()),
            force: false,
        }))
        .await
        .map_err(|err| format!("mcp spec scan call failed: {err}"))?;
    let json = extract_mcp_json(scanned)?;
    ensure_status_ok(&json, "mcp spec scan returned non-ok status")?;
    Ok(Cell::Passed)
}
