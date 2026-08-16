use serde_json::{
    Value,
    json,
};
use ticket_api::{
    model::ticket::TicketManifestExt,
    storage::ticket_fs::TicketFs,
};
use uuid::Uuid;

use super::{
    types::*,
    *,
};

fn parse_part_id(value: &str) -> Result<Uuid, McpError> {
    value.parse::<Uuid>().map_err(|error| {
        McpError::invalid_params(
            format!("invalid part_id '{value}': {error}"),
            None,
        )
    })
}

fn part_json(
    part: &ticket_api::storage::ticket_fs::LoadedPart,
    with_content: bool,
) -> Value {
    let mut item = json!({
        "id": part.id,
        "kind": part.kind,
        "path": part.path,
        "frozen": part.frozen,
        "created_at": part.created_at,
        "supersedes": part.supersedes,
        "implicit": part.implicit,
    });
    if with_content {
        item["content"] = Value::String(part.content.clone());
    }
    item
}

fn find_part_in_manifest(
    manifest: &ticket_api::model::ticket::TicketManifest,
    part_id: Uuid,
) -> Option<Value> {
    manifest
        .parts()
        .into_iter()
        .find(|p| p.id == part_id)
        .map(|p| {
            json!({
                "id": p.id,
                "kind": p.kind,
                "path": p.path,
                "frozen": p.frozen,
                "created_at": p.created_at,
                "supersedes": p.supersedes,
            })
        })
}

impl TicketServer {
    pub(crate) async fn list_parts_tool(
        &self,
        input: ListPartsInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let with_content = input.with_content;
        let (id, report) = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = Self::resolve_uuid_for_read(store, &id_str)?;
                let indexed = store
                    .get_indexed(&id)
                    .map_err(Self::store_err)?
                    .ok_or_else(|| {
                        Self::store_err(
                            ticket_api::error::StorageError::NotFound(id),
                        )
                    })?;
                let manifest =
                    TicketFs::read(&indexed.path).map_err(Self::store_err)?;
                let report = TicketFs::load_parts(&indexed.path, &manifest)
                    .map_err(Self::store_err)?;
                Ok((id, report))
            })
            .await?;

        let parts: Vec<Value> = report
            .parts
            .iter()
            .map(|part| part_json(part, with_content))
            .collect();
        let orphans: Vec<String> = report
            .orphans
            .iter()
            .map(|path| path.display().to_string())
            .collect();

        Self::json_result(&json!({
            "workspace": workspace,
            "status": "ok",
            "id": id.to_string(),
            "count": parts.len(),
            "parts": parts,
            "orphans": orphans,
        }))
    }

    pub(crate) async fn get_part_tool(
        &self,
        input: GetPartInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let part_id = parse_part_id(&input.part_id)?;
        let (id, part) = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = Self::resolve_uuid_for_read(store, &id_str)?;
                let indexed = store
                    .get_indexed(&id)
                    .map_err(Self::store_err)?
                    .ok_or_else(|| {
                        Self::store_err(
                            ticket_api::error::StorageError::NotFound(id),
                        )
                    })?;
                let manifest =
                    TicketFs::read(&indexed.path).map_err(Self::store_err)?;
                let report = TicketFs::load_parts(&indexed.path, &manifest)
                    .map_err(Self::store_err)?;
                let part = report.find(part_id).cloned().ok_or_else(|| {
                    McpError::invalid_params(
                        format!(
                            "part '{part_id}' was not found on ticket '{id}'"
                        ),
                        None,
                    )
                })?;
                Ok((id, part))
            })
            .await?;

        Self::json_result(&json!({
            "workspace": workspace,
            "status": "ok",
            "id": id.to_string(),
            "part": part_json(&part, true),
        }))
    }

    pub(crate) async fn write_part_tool(
        &self,
        input: WritePartInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let kind = input.kind;
        let content = input.content;
        let author = input.author;
        let part_id = match input.part_id.as_deref() {
            Some(value) => parse_part_id(value)?,
            None => Uuid::new_v4(),
        };

        let (id, manifest) = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                let manifest = store
                    .write_part(
                        &id,
                        part_id,
                        &kind,
                        &content,
                        author.as_deref(),
                    )
                    .map_err(Self::store_err)?;
                Ok((id, manifest))
            })
            .await?;

        Self::json_result(&json!({
            "workspace": workspace,
            "status": "ok",
            "id": id.to_string(),
            "part": find_part_in_manifest(&manifest, part_id),
        }))
    }

    pub(crate) async fn write_amendment_tool(
        &self,
        input: WriteAmendmentInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let content = input.content;
        let author = input.author;
        let supersedes = parse_part_id(&input.supersedes)?;
        let part_id = match input.part_id.as_deref() {
            Some(value) => parse_part_id(value)?,
            None => Uuid::new_v4(),
        };

        let (id, manifest) = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                let manifest = store
                    .write_amendment_part(
                        &id,
                        part_id,
                        &content,
                        supersedes,
                        author.as_deref(),
                    )
                    .map_err(Self::store_err)?;
                Ok((id, manifest))
            })
            .await?;

        Self::json_result(&json!({
            "workspace": workspace,
            "status": "ok",
            "id": id.to_string(),
            "supersedes": supersedes.to_string(),
            "part": find_part_in_manifest(&manifest, part_id),
        }))
    }

    pub(crate) async fn undo_part_tool(
        &self,
        input: UndoPartInput,
    ) -> Result<CallToolResult, McpError> {
        let workspace = input.workspace;
        let id_str = input.id;
        let author = input.author;
        let part_id = parse_part_id(&input.part_id)?;

        let (id, manifest) = self
            .with_store_ext(&workspace.clone(), move |store| {
                let id = Self::resolve_uuid_with(store, &id_str)?;
                let manifest = store
                    .undo_part(&id, part_id, author.as_deref())
                    .map_err(Self::store_err)?;
                Ok((id, manifest))
            })
            .await?;

        Self::json_result(&json!({
            "workspace": workspace,
            "status": "ok",
            "id": id.to_string(),
            "part": find_part_in_manifest(&manifest, part_id),
        }))
    }
}
