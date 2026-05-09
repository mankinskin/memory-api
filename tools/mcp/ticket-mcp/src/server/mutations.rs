use std::collections::BTreeMap;

use serde_json::Value;
use ticket_api::{model::edge::EdgeRecord, model::ticket::TicketManifest};

use super::{types::*, *};

impl TicketServer {
    pub(crate) async fn update_ticket_tool(
        &self,
        input: UpdateTicketInput,
    ) -> Result<CallToolResult, McpError> {
        if input.undo {
            return self.undo_ticket_update(input).await;
        }

        let patch = parse_field_patch(&input.fields)?;
        let workspace = input.workspace;
        let id_str = input.id;
        let from_state = input.from_state;
        let to_state = input.to_state;
        let description = input.description;
        let author = input.author;
        let manifest = self
            .with_store_ext(move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                store
                    .update(
                        &id,
                        patch,
                        from_state.as_deref(),
                        to_state.as_deref(),
                        description.as_deref(),
                        author.as_deref(),
                    )
                    .map_err(Self::store_err)
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "ticket": detail_from_manifest(manifest),
        }))
    }

    pub(crate) async fn close_ticket_tool(
        &self,
        input: CloseTicketInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let author = input.author;
        let target_state = input.to_state.clone();
        let (manifest, path) = self
            .with_store_ext(move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                store
                    .close(&id, &input.to_state, author.as_deref())
                    .map_err(Self::store_err)
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "id": manifest.id.to_string(),
            "target_state": target_state,
            "traversed_states": path,
        }))
    }

    pub(crate) async fn cancel_ticket_tool(
        &self,
        input: CancelTicketInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let author = input.author;
        let (manifest, path) = self
            .with_store_ext(move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                store
                    .close(&id, "cancelled", author.as_deref())
                    .map_err(Self::store_err)
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "id": manifest.id.to_string(),
            "traversed_states": path,
        }))
    }

    pub(crate) async fn create_ticket_tool(
        &self,
        input: CreateTicketInput,
    ) -> Result<CallToolResult, McpError> {
        let extra = parse_field_patch(&input.fields)?;
        let workspace = input.workspace;
        let type_id = input.type_id;
        let title = input.title;
        let state = input.state;
        let description = input.description;
        let (ticket_id, manifest) = self
            .with_store_ext(move |store| {
                let id = store
                    .create(
                        None,
                        &type_id,
                        title.as_deref(),
                        state.as_deref(),
                        extra,
                        None,
                        description.as_deref(),
                    )
                    .map_err(Self::store_err)?;
                let manifest = store.get(&id).map_err(Self::store_err)?;
                Ok((id, manifest))
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "id": ticket_id.to_string(),
            "ticket": detail_from_manifest(manifest),
        }))
    }

    pub(crate) async fn delete_ticket_tool(
        &self,
        input: DeleteTicketInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let id = self
            .with_store_ext(move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                store.delete(&id).map_err(Self::store_err)?;
                Ok(id)
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "id": id.to_string(),
            "deleted": true,
        }))
    }

    pub(crate) async fn add_edge_tool(
        &self,
        input: AddEdgeInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let from_str = input.from;
        let to_str = input.to;
        let kind = input.kind;

        self.with_store_ext(move |store| {
            let from = Self::resolve_uuid_with(store, &from_str)?;
            let to = Self::resolve_uuid_with(store, &to_str)?;
            let edge = EdgeRecord {
                from,
                to,
                kind: kind.clone(),
                created_at: chrono::Utc::now(),
            };
            store.add_edge(edge).map_err(Self::store_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "edge": EdgeItem {
                    from: from.to_string(),
                    to: to.to_string(),
                    kind,
                },
            }))
        })
        .await
    }

    pub(crate) async fn remove_edge_tool(
        &self,
        input: RemoveEdgeInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let from_str = input.from;
        let to_str = input.to;
        let kind = input.kind;

        self.with_store_ext(move |store| {
            let from = Self::resolve_uuid_with(store, &from_str)?;
            let to = Self::resolve_uuid_with(store, &to_str)?;
            let edge = EdgeRecord {
                from,
                to,
                kind: kind.clone(),
                created_at: chrono::Utc::now(),
            };
            store.remove_edge(edge).map_err(Self::store_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "removed": EdgeItem {
                    from: from.to_string(),
                    to: to.to_string(),
                    kind,
                },
            }))
        })
        .await
    }

    async fn undo_ticket_update(
        &self,
        input: UpdateTicketInput,
    ) -> Result<CallToolResult, McpError> {
        if input.to_state.is_some() || !input.fields.is_empty() {
            return Err(McpError::invalid_params(
                "undo cannot be combined with to_state or fields",
                None,
            ));
        }

        let workspace = input.workspace;
        let id_str = input.id;
        let (previous_rev, new_rev, updated) = self
            .with_store_ext(move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                let revisions = store.get_history(&id).map_err(Self::store_err)?;
                if revisions.len() < 2 {
                    return Err(Self::store_err(ticket_api::error::StorageError::Database(
                        "cannot undo: not enough history revisions".into(),
                    )));
                }
                let previous = &revisions[revisions.len() - 2];
                let new_rev = store
                    .apply_revert(&id, previous.fields.clone(), None)
                    .map_err(Self::store_err)?;
                let updated = store.get(&id).map_err(Self::store_err)?;
                Ok((previous.rev, new_rev, updated))
            })
            .await?;

        Self::json_result(&serde_json::json!({
            "workspace": workspace,
            "status": "ok",
            "undo": true,
            "reverted_to": previous_rev,
            "new_rev": new_rev,
            "ticket": detail_from_manifest(updated),
        }))
    }
}

fn parse_field_patch(fields: &[String]) -> Result<BTreeMap<String, Value>, McpError> {
    let mut patch = BTreeMap::new();

    for raw in fields {
        let (key, value) = raw.split_once('=').ok_or_else(|| {
            McpError::invalid_params(format!("invalid field format '{raw}', expected key=value"), None)
        })?;
        patch.insert(key.trim().to_string(), Value::String(value.trim().to_string()));
    }

    Ok(patch)
}

fn detail_from_manifest(manifest: TicketManifest) -> TicketDetail {
    TicketDetail {
        id: manifest.id.to_string(),
        created_at: manifest.created_at,
        fields: manifest.extra,
    }
}