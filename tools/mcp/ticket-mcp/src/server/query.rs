use ticket_api::storage::ticket_fs::TicketFs;

use super::{
    types::*,
    *,
};

impl TicketServer {
    pub(crate) async fn health_tool(&self) -> Result<CallToolResult, McpError> {
        match self
            .with_store("default", |store| store.list(None, None, Some(0)))
            .await
        {
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

    pub(crate) async fn list_workspaces_tool(
        &self
    ) -> Result<CallToolResult, McpError> {
        let active_path = self.index_root.to_string_lossy().to_string();
        Self::json_result(&serde_json::json!({
            "workspaces": ["default", active_path],
            "active": "default",
            "active_path": self.index_root,
        }))
    }

    pub(crate) async fn list_tickets_tool(
        &self,
        input: ListTicketsInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace.clone();
        let items = if let Some(query) = input.query.as_deref() {
            let limit = input.limit.unwrap_or(100).min(1000);
            self.with_store(&workspace, |store| {
                search_ticket_summaries(store, query, limit)
            })
            .await?
        } else {
            self.with_store(&workspace, |store| {
                listed_ticket_summaries(store, &input)
            })
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
        let workspace = input.workspace.unwrap_or_else(|| "default".to_string());
        let id_str = input.id;

        self.with_store_ext(&workspace.clone(), move |store| {
            let id = Self::resolve_uuid_for_read(store, &id_str)?;
            let path = store
                .get_indexed(&id)
                .map_err(Self::store_err)?
                .map(|ticket| ticket.path.display().to_string());
            let manifest = store.get(&id).map_err(Self::store_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "ticket": TicketDetail {
                    id: manifest.id.to_string(),
                    path,
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
        let workspace = input.workspace.unwrap_or_else(|| "default".to_string());
        let id_str = input.id;

        self.with_store_ext(&workspace.clone(), move |store| {
            let id = Self::resolve_uuid_for_read(store, &id_str)?;
            let indexed = store
                .get_indexed(&id)
                .map_err(Self::store_err)?
                .ok_or_else(|| {
                    McpError::invalid_params(
                        format!("ticket not found: {id}"),
                        None,
                    )
                })?;

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
        let workspace = input.workspace;
        let items = self
            .with_store(&workspace, |store| store.list_all_edges())
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
            "workspace": workspace,
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
            effort: None,
            updated_at: indexed_updated_at(store, &result.id),
        })
        .collect())
}

fn listed_ticket_summaries(
    store: &TicketStore,
    input: &ListTicketsInput,
) -> Result<Vec<TicketSummary>, ticket_api::error::StorageError> {
    let limit = input.limit.map(|value| value.min(1000));
    let tickets =
        store.list(input.state.as_deref(), input.type_id.as_deref(), limit)?;
    let model = ticket_api::workflow::WorkflowModel::build(
        store,
        tickets.clone(),
        store.list_all_edges()?,
    )?;
    Ok(tickets
        .into_iter()
        .map(|ticket| TicketSummary {
            id: ticket.id.to_string(),
            type_id: ticket.type_id,
            title: ticket.title,
            state: ticket.state,
            effort: model.effort(&ticket.id),
            updated_at: ticket.updated_at,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::Value;
    use ticket_api::storage::store::TicketStore;

    use super::*;

    fn extract_text(result: &CallToolResult) -> String {
        result
            .content
            .iter()
            .find_map(|content| {
                if let rmcp::model::RawContent::Text(text) = &content.raw {
                    Some(text.text.clone())
                } else {
                    None
                }
            })
            .expect("text content")
    }

    #[tokio::test]
    async fn get_ticket_tool_returns_authoritative_ticket_folder_path() {
        let dir = tempfile::tempdir().expect("tempdir");
        let server = TicketServer::new(dir.path().to_path_buf());
        let store = TicketStore::init(dir.path()).expect("open store");
        let id = store
            .create(
                None,
                "tracker-improvement",
                Some("path output regression"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create ticket");
        let indexed = store
            .get_indexed(&id)
            .expect("indexed get")
            .expect("indexed ticket");

        let result = server
            .get_ticket_tool(TicketRefInput {
                workspace: None,
                id: id.to_string(),
            })
            .await
            .expect("get_ticket_tool ok");
        let text = extract_text(&result);
        let json: Value = serde_json::from_str(&text).expect("valid json");

        assert!(json.get("workspace").is_none());
        assert_eq!(
            json["ticket"]["path"].as_str(),
            Some(indexed.path.display().to_string().as_str())
        );
        // No default-type assumption: the type field is retained in output.
        assert_eq!(json["ticket"]["fields"]["type"], "tracker-improvement");
    }

    #[tokio::test]
    async fn get_ticket_tool_reads_an_indexed_descendant_from_parent_workspace() {
        let dir = tempfile::tempdir().expect("tempdir");
        let parent = TicketStore::init(dir.path()).expect("parent store");
        let child_root = dir.path().join("child").join(".ticket");
        let child = TicketStore::init(&child_root).expect("child store");
        let id = child
            .create(
                None,
                "tracker-improvement",
                Some("descendant ticket"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create child ticket");
        parent
            .add_scan_root(ticket_api::model::filesystem::ScanRoot {
                path: child_root.join("tickets"),
                label: "child".to_string(),
            })
            .expect("register child scan root");
        parent.scan(true).expect("scan child store");

        let server = TicketServer::new(dir.path().to_path_buf());
        let result = server
            .get_ticket_tool(TicketRefInput {
                workspace: None,
                id: id.to_string(),
            })
            .await
            .expect("get indexed descendant ticket");
        let json: Value = serde_json::from_str(&extract_text(&result))
            .expect("valid json");

        assert_eq!(json["ticket"]["id"].as_str(), Some(id.to_string().as_str()));
    }

    #[tokio::test]
    async fn get_ticket_tool_reports_searched_workspaces_when_prefix_is_missing() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = TicketStore::init(dir.path()).expect("open store");
        let expected_root = store.index_root.display().to_string();
        drop(store);
        let server = TicketServer::new(dir.path().to_path_buf());

        let error = server
            .get_ticket_tool(TicketRefInput {
                workspace: None,
                id: "deadbeef".to_string(),
            })
            .await
            .expect_err("missing prefix should fail");

        assert!(error.to_string().contains("searched workspaces"));
        assert!(error.to_string().contains(&expected_root));
    }

    #[tokio::test]
    async fn list_tickets_tool_omits_default_workspace_but_retains_type() {
        let dir = tempfile::tempdir().expect("tempdir");
        let server = TicketServer::new(dir.path().to_path_buf());
        let store = TicketStore::init(dir.path()).expect("open store");
        store
            .create(
                None,
                "tracker-improvement",
                Some("default output shaping"),
                Some("new"),
                BTreeMap::new(),
                None,
                None,
            )
            .expect("create ticket");

        let result = server
            .list_tickets_tool(ListTicketsInput {
                workspace: "default".to_string(),
                state: None,
                type_id: None,
                query: None,
                limit: None,
            })
            .await
            .expect("list_tickets_tool ok");
        let text = extract_text(&result);
        let json: Value = serde_json::from_str(&text).expect("valid json");

        assert!(json.get("workspace").is_none());
        assert_eq!(json["items"][0]["type"], "tracker-improvement");
    }
}

fn indexed_updated_at(
    store: &TicketStore,
    id: &Uuid,
) -> chrono::DateTime<chrono::Utc> {
    store
        .get_indexed(id)
        .ok()
        .flatten()
        .map(|ticket| ticket.updated_at)
        .unwrap_or_else(|| {
            chrono::DateTime::<chrono::Utc>::from(
                std::time::SystemTime::UNIX_EPOCH,
            )
        })
}
