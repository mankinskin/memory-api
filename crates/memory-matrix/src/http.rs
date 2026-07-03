use super::*;

pub(super) fn dispatch_http(
    domain: &str,
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    match domain {
        "ticket" => dispatch_ticket_http(operation, ctx),
        _ => blocked(format!(
            "http transport for domain `{domain}` operation `{operation}` is not wired yet"
        )),
    }
}

fn dispatch_ticket_http(
    operation: &str,
    ctx: &MatrixCtx,
) -> CellResult {
    match operation {
        "get" => run_ticket_http_get(ctx),
        "search" => run_ticket_http_search(ctx),
        _ => blocked(format!(
            "http transport for domain `ticket` operation `{operation}` is not wired yet; \
             currently only `ticket.get@http` and `ticket.search@http` are exercised through the ticket-http router surface"
        )),
    }
}

fn run_ticket_http_get(ctx: &MatrixCtx) -> CellResult {
    let (id, workspace, app) = build_ticket_http_fixture(ctx)?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            format!("build tokio runtime for http matrix cell: {err}")
        })?;

    runtime
        .block_on(async move {
            let request = Request::builder()
                .method(Method::GET)
                .uri(format!("/api/tickets/{id}?workspace={workspace}"))
                .body(Body::empty())
                .map_err(|err| format!("build ticket get request: {err}"))?;

            let response = app
                .oneshot(request)
                .await
                .map_err(|err| format!("dispatch ticket get request: {err}"))?;

            if response.status() != StatusCode::OK {
                return Err(format!(
                    "ticket-http get returned unexpected status {}",
                    response.status()
                ));
            }

            let bytes = to_bytes(response.into_body(), 1024 * 1024)
                .await
                .map_err(|err| format!("read ticket get response body: {err}"))?;
            let payload: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|err| format!("parse ticket get response body: {err}"))?;

            let returned_id = payload["ticket"]["ticket_ref"]["id"]
                .as_str()
                .ok_or_else(|| {
                    "ticket get payload missing ticket.ticket_ref.id".to_string()
                })?;
            if returned_id != id.to_string() {
                return Err(format!(
                    "ticket-http get returned mismatched id: expected {id}, got {returned_id}"
                ));
            }

            Ok(Cell::Passed)
        })
}

fn run_ticket_http_search(ctx: &MatrixCtx) -> CellResult {
    let (id, workspace, app) = build_ticket_http_fixture(ctx)?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| {
            format!("build tokio runtime for http matrix cell: {err}")
        })?;

    runtime
        .block_on(async move {
            let request = Request::builder()
                .method(Method::GET)
                .uri(format!(
                    "/api/tickets?workspace={workspace}&query=matrix-http-ticket"
                ))
                .body(Body::empty())
                .map_err(|err| format!("build ticket search request: {err}"))?;

            let response = app
                .oneshot(request)
                .await
                .map_err(|err| format!("dispatch ticket search request: {err}"))?;

            if response.status() != StatusCode::OK {
                return Err(format!(
                    "ticket-http search returned unexpected status {}",
                    response.status()
                ));
            }

            let bytes = to_bytes(response.into_body(), 1024 * 1024)
                .await
                .map_err(|err| format!("read ticket search response body: {err}"))?;
            let payload: serde_json::Value = serde_json::from_slice(&bytes)
                .map_err(|err| format!("parse ticket search response body: {err}"))?;

            let items = payload["items"]
                .as_array()
                .ok_or_else(|| "ticket search payload missing items array".to_string())?;
            let expected_id = id.to_string();
            let found = items.iter().any(|item| {
                item["ticket_ref"]["id"]
                    .as_str()
                    .map(|candidate| candidate == expected_id)
                    .unwrap_or(false)
            });
            if !found {
                return Err(format!(
                    "ticket-http search did not return seeded ticket id {expected_id}"
                ));
            }

            Ok(Cell::Passed)
        })
}

fn build_ticket_http_fixture(
    ctx: &MatrixCtx
) -> Result<(uuid::Uuid, String, axum::Router), String> {
    let ticket_store_root = ctx.store_root(".ticket");
    let tickets_scan_root = ctx.workspace_root.join("tickets");

    std::fs::create_dir_all(&tickets_scan_root).map_err(|err| {
        format!(
            "failed to create ticket scan root `{}`: {err}",
            tickets_scan_root.display()
        )
    })?;

    let store = Arc::new(
        TicketStore::open_or_init(&ticket_store_root)
            .map_err(|err| format!("open ticket store: {err}"))?,
    );

    let has_scan_root = store
        .list_scan_roots()
        .map_err(|err| format!("list scan roots: {err}"))?
        .into_iter()
        .any(|root| root.path == tickets_scan_root);
    if !has_scan_root {
        store
            .add_scan_root(ScanRoot {
                path: tickets_scan_root,
                label: "default".into(),
            })
            .map_err(|err| format!("add ticket scan root: {err}"))?;
    }

    let id = store
        .create(
            None,
            "tracker-improvement",
            Some("matrix-http-ticket-get"),
            None,
            BTreeMap::new(),
            None,
            None,
        )
        .map_err(|err| format!("seed ticket for http get: {err}"))?;

    let state = AppState::new(
        Arc::new(WorkspaceRegistry::single_opened(Arc::clone(&store))),
        Arc::new(StreamBroker::new()),
    );
    let workspace = state.registry.primary_workspace_name().to_string();
    let app = build_router(state);

    Ok((id, workspace, app))
}
