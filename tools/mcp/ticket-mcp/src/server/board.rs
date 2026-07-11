use serde_json::Value;
use ticket_api::{
    BoardEntry,
    BoardEntryStatus,
    storage::board::BoardSnapshot,
};

use super::{
    types::*,
    *,
};

impl TicketServer {
    pub(crate) async fn board_show_tool(
        &self,
        input: BoardShowInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let agent_id = input.agent_id;

        let active_index_root = self
            .resolve_workspace_root(&workspace)
            .map(|path| path.display().to_string())
            .unwrap_or_default();

        self.with_store_ext(&workspace.clone(), move |store| {
            let workspace = workspace.as_str();
            let agent_ref = agent_id.as_deref();
            let snapshot =
                store.board_show(agent_ref).map_err(Self::board_err)?;
            let (heartbeat_entries, final_snapshot) =
                refresh_snapshot(store, agent_ref, snapshot)?;

            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "scope": {
                    "workspace": workspace,
                    "active_index_root": &active_index_root,
                },
                "snapshot": final_snapshot,
                "heartbeat": heartbeat_value(&heartbeat_entries),
            }))
        })
        .await
    }

    pub(crate) async fn board_history_tool(
        &self,
        input: BoardHistoryInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let agent_id = input.agent_id;

        self.with_store_ext(&workspace.clone(), move |store| {
            let snapshot = store
                .board_history(agent_id.as_deref())
                .map_err(Self::board_err)?;

            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "snapshot": snapshot,
            }))
        })
        .await
    }

    pub(crate) async fn board_check_in_tool(
        &self,
        input: BoardCheckInInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let ticket_id_str = input.ticket_id;
        let agent_id = input.agent_id;
        let intent = input.intent.unwrap_or_default();
        let files = input.files;
        let ttl_secs = input.ttl_secs.unwrap_or(3600);

        self.with_store_ext(&workspace.clone(), move |store| {
            let ticket_id = Self::resolve_uuid_with(store, &ticket_id_str)?;
            let entry = store
                .board_check_in(&ticket_id, &agent_id, ttl_secs, &intent, files)
                .map_err(Self::board_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "entry": entry,
            }))
        })
        .await
    }

    pub(crate) async fn board_check_out_tool(
        &self,
        input: BoardCheckOutInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let ticket_id_str = input.ticket_id;
        let agent_id_arg = input.agent_id;
        let reason = input.reason;

        self.with_store_ext(&workspace.clone(), move |store| {
            let ticket_id = Self::resolve_uuid_with(store, &ticket_id_str)?;
            let agent_id =
                resolve_checkout_agent(store, ticket_id, agent_id_arg.clone())?;
            let entry = store
                .board_check_out(&ticket_id, &agent_id, reason.as_deref())
                .map_err(Self::board_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "entry": entry,
            }))
        })
        .await
    }

    pub(crate) async fn board_release_lease_tool(
        &self,
        input: BoardReleaseLeaseInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let ticket_id_str = input.ticket_id;
        let requester = input.requester;

        self.with_store_ext(&workspace.clone(), move |store| {
            let ticket_id = Self::resolve_uuid_with(store, &ticket_id_str)?;
            store
                .release_lease(&ticket_id, &requester)
                .map_err(|error| match error {
                    ticket_api::error::StorageError::LeaseConflict {
                        ticket,
                        holder,
                    } => McpError::invalid_params(
                        format!(
                            "lease conflict: ticket {ticket} is held by {holder}"
                        ),
                        None,
                    ),
                    other => Self::store_err(other),
                })?;

            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "ticket_id": ticket_id,
                "requester": requester,
            }))
        })
        .await
    }

    pub(crate) async fn board_heartbeat_tool(
        &self,
        input: BoardHeartbeatInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let entry_id_str = input.entry_id;

        self.with_store_ext(&workspace.clone(), move |store| {
            let entry_id = entry_id_str.parse::<Uuid>().map_err(|_| {
                McpError::invalid_params(
                    format!(
                        "invalid UUID '{}': expected full UUID",
                        entry_id_str
                    ),
                    None,
                )
            })?;
            let entry =
                store.board_heartbeat(&entry_id).map_err(Self::board_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "entry": entry,
            }))
        })
        .await
    }

    pub(crate) async fn board_configure_tool(
        &self,
        input: BoardConfigureInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;

        self.with_store_ext(&workspace.clone(), move |store| {
            let current =
                store.board_configure(None).map_err(Self::board_err)?;
            let config = if input.max_wip.is_none()
                && input.stale_after_secs.is_none()
                && input.completed_audit_window_secs.is_none()
            {
                current
            } else {
                let updated = ticket_api::BoardConfig {
                    max_wip: input.max_wip.unwrap_or(current.max_wip),
                    stale_after_secs: input
                        .stale_after_secs
                        .unwrap_or(current.stale_after_secs),
                    completed_audit_window_secs: input
                        .completed_audit_window_secs
                        .unwrap_or(current.completed_audit_window_secs),
                };
                store
                    .board_configure(Some(updated))
                    .map_err(Self::board_err)?
            };

            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "config": config,
            }))
        })
        .await
    }

    pub(crate) async fn board_clean_preview_tool(
        &self,
        input: BoardCleanPreviewInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let include_stale = input.include_stale.unwrap_or(false);

        self.with_store_ext(&workspace.clone(), move |store| {
            let preview = store
                .board_clean_preview(include_stale)
                .map_err(Self::board_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "preview": preview,
            }))
        })
        .await
    }

    pub(crate) async fn board_clean_apply_tool(
        &self,
        input: BoardCleanApplyInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let token = input.token;
        let include_stale = input.include_stale.unwrap_or(false);

        self.with_store_ext(&workspace.clone(), move |store| {
            let result = store
                .board_clean_apply(&token, include_stale)
                .map_err(Self::board_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "result": result,
            }))
        })
        .await
    }

    pub(crate) async fn board_update_files_tool(
        &self,
        input: BoardUpdateFilesInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let ticket_id_str = input.ticket_id;
        let agent_id = input.agent_id;
        let add = input.add;
        let remove = input.remove;

        self.with_store_ext(&workspace.clone(), move |store| {
            let ticket_id = Self::resolve_uuid_with(store, &ticket_id_str)?;
            let entry = store
                .board_update_files(&ticket_id, &agent_id, add, remove)
                .map_err(Self::board_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "entry": entry,
            }))
        })
        .await
    }

    pub(crate) async fn board_rename_file_tool(
        &self,
        input: BoardRenameFileInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let ticket_id_str = input.ticket_id;
        let agent_id = input.agent_id;
        let old_path = input.old_path;
        let new_path = input.new_path;

        self.with_store_ext(&workspace.clone(), move |store| {
            let ticket_id = Self::resolve_uuid_with(store, &ticket_id_str)?;
            let entry = store
                .board_rename_file(&ticket_id, &agent_id, &old_path, &new_path)
                .map_err(Self::board_err)?;
            Self::json_result(&serde_json::json!({
                "workspace": workspace,
                "status": "ok",
                "entry": entry,
            }))
        })
        .await
    }
}

fn refresh_snapshot(
    store: &TicketStore,
    agent_id: Option<&str>,
    snapshot: BoardSnapshot,
) -> Result<(Vec<BoardEntry>, BoardSnapshot), McpError> {
    if agent_id.is_none() || snapshot.caller_entries.is_empty() {
        return Ok((Vec::new(), snapshot));
    }

    let mut refreshed = Vec::new();
    for entry in &snapshot.caller_entries {
        if let Ok(updated) = store.board_heartbeat(&entry.entry_id) {
            refreshed.push(updated);
        }
    }

    let final_snapshot = store
        .board_show(agent_id)
        .map_err(TicketServer::board_err)?;
    Ok((refreshed, final_snapshot))
}

fn heartbeat_value(entries: &[BoardEntry]) -> Value {
    if entries.is_empty() {
        Value::Null
    } else {
        serde_json::to_value(entries).unwrap_or(Value::Null)
    }
}

fn resolve_checkout_agent(
    store: &TicketStore,
    ticket_id: Uuid,
    agent_id: Option<String>,
) -> Result<String, McpError> {
    if let Some(agent_id) = agent_id {
        return Ok(agent_id);
    }

    let snapshot = store.board_show(None).map_err(TicketServer::board_err)?;
    snapshot
        .entries
        .iter()
        .find(|entry| {
            entry.ticket_id == ticket_id
                && matches!(entry.status, BoardEntryStatus::Active)
        })
        .map(|entry| entry.agent_id.clone())
        .ok_or_else(|| {
            McpError::invalid_params(
                format!("no active board entry found for ticket {ticket_id}"),
                None,
            )
        })
}
