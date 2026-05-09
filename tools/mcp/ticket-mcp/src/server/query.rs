use ticket_api::{storage::ticket_fs::TicketFs, workspace::WorkspaceConfig};

use super::{types::*, *};

impl TicketServer {
    pub(crate) async fn health_tool(&self) -> Result<CallToolResult, McpError> {
        match self.with_store(|store| store.list(None, None, Some(0))).await {
            Ok(_) => Self::json_result(&serde_json::json!({
                "status": "ok",
                "service": "ticket-mcp",
                "mode": "direct",
            })),
            Err(error) => Self::json_result(&serde_json::json!({
                "status": "error",
                "error": error.to_string(),
            })),
        }
    }

    pub(crate) async fn list_workspaces_tool(&self) -> Result<CallToolResult, McpError> {
        let config = WorkspaceConfig::load();
        let workspaces = if config.workspaces.is_empty() {
            vec!["default".to_string()]
        } else {
            config.workspaces.keys().cloned().collect()
        };

        Self::json_result(&serde_json::json!({
            "workspaces": workspaces,
            "active": config.active,
        }))
    }

    pub(crate) async fn list_tickets_tool(
        &self,
        input: ListTicketsInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        let items = if let Some(query) = input.query.as_deref() {
            let limit = input.limit.unwrap_or(100).min(1000);
            self.with_store(|store| search_ticket_summaries(store, query, limit))
                .await?
        } else {
            self.with_store(|store| listed_ticket_summaries(store, &input))
                .await?
        };

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "items": items,
        }))
    }

    pub(crate) async fn get_ticket_tool(
        &self,
        input: TicketRefInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;

        self.with_store_ext(move |store| {
            let id = Self::resolve_uuid_with(store, &id_str)?;
            let manifest = store.get(&id).map_err(Self::store_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "ticket": TicketDetail {
                    id: manifest.id.to_string(),
                    created_at: manifest.created_at,
                    fields: manifest.extra,
                },
            }))
        })
        .await
    }

    pub(crate) async fn get_ticket_description_tool(
        &self,
        input: TicketRefInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;

        self.with_store_ext(move |store| {
            let id = Self::resolve_uuid_with(store, &id_str)?;
            let indexed = store
                .get_indexed(&id)
                .map_err(Self::store_err)?
                .ok_or_else(|| McpError::invalid_params(format!("ticket not found: {id}"), None))?;

            if indexed.deleted {
                return Err(McpError::invalid_params(format!("ticket deleted: {id}"), None));
            }

            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "id": id.to_string(),
                "description": TicketFs::read_description(&indexed.path),
            }))
        })
        .await
    }

    pub(crate) async fn list_edges_tool(
        &self,
        input: ListEdgesInput,
    ) -> Result<CallToolResult, McpError> {
        let items = self
            .with_store(|store| store.list_all_edges())
            .await?
            .into_iter()
            .filter(|edge| match &input.kind {
                Some(kind) => kind == "all" || edge.kind == *kind,
                None => true,
            })
            .map(|edge| EdgeItem {
                from: edge.from.to_string(),
                to: edge.to.to_string(),
                kind: edge.kind,
            })
            .collect::<Vec<_>>();

        Self::json_result(&serde_json::json!({
            "workspace": input.workspace,
            "items": items,
        }))
    }
}

fn search_ticket_summaries(
    store: &TicketStore,
    query: &str,
    limit: usize,
) -> Result<Vec<TicketSummary>, ticket_api::error::StorageError> {
    let results = store.search_tickets(query, limit)?;
    Ok(results
        .into_iter()
        .map(|result| TicketSummary {
            id: result.id.to_string(),
            type_id: result.ticket_type.unwrap_or_default(),
            title: result.title,
            state: result.state,
            updated_at: indexed_updated_at(store, &result.id),
        })
        .collect())
}

fn listed_ticket_summaries(
    store: &TicketStore,
    input: &ListTicketsInput,
) -> Result<Vec<TicketSummary>, ticket_api::error::StorageError> {
    let limit = input.limit.map(|value| value.min(1000));
    Ok(store
        .list(input.state.as_deref(), input.type_id.as_deref(), limit)?
        .into_iter()
        .map(|ticket| TicketSummary {
            id: ticket.id.to_string(),
            type_id: ticket.type_id,
            title: ticket.title,
            state: ticket.state,
            updated_at: ticket.updated_at,
        })
        .collect())
}

fn indexed_updated_at(store: &TicketStore, id: &Uuid) -> chrono::DateTime<chrono::Utc> {
    store
        .get_indexed(id)
        .ok()
        .flatten()
        .map(|ticket| ticket.updated_at)
        .unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::from(std::time::SystemTime::UNIX_EPOCH))
}