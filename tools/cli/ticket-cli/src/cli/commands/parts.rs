use serde_json::{
    Value,
    json,
};
use ticket_api::{
    model::ticket::TicketManifest,
    storage::{
        TicketStore,
        ticket_fs::{
            PartsLoadReport,
            TicketFs,
        },
    },
};
use uuid::Uuid;

use crate::cli::{
    CliRunError,
    GetPartArgs,
    ListPartsArgs,
    UndoPartArgs,
    WriteAmendmentArgs,
    WritePartArgs,
};

fn resolve_content(
    content: Option<String>,
    content_file: Option<std::path::PathBuf>,
) -> Result<String, CliRunError> {
    match (content, content_file) {
        (Some(text), None) => Ok(text),
        (None, Some(path)) => std::fs::read_to_string(&path).map_err(|e| {
            CliRunError::InvalidFieldPatch(format!(
                "cannot read content-file: {e}"
            ))
        }),
        (None, None) => Err(CliRunError::BadRequest(
            "one of --content or --content-file is required".into(),
        )),
        (Some(_), Some(_)) => unreachable!("clap enforces conflicts_with"),
    }
}

fn load_parts_report(
    store: &TicketStore,
    id: &Uuid,
) -> Result<PartsLoadReport, CliRunError> {
    let indexed = store.get_indexed(id)?.ok_or_else(|| {
        CliRunError::BadRequest(format!("ticket not found: {id}"))
    })?;
    let manifest = TicketFs::read(&indexed.path)?;
    TicketFs::load_parts(&indexed.path, &manifest).map_err(CliRunError::from)
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

pub(crate) fn cmd_list_parts(
    args: ListPartsArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let report = load_parts_report(store, &id)?;
    let parts: Vec<Value> = report
        .parts
        .iter()
        .map(|part| part_json(part, args.with_content))
        .collect();
    let orphans: Vec<String> = report
        .orphans
        .iter()
        .map(|path| path.display().to_string())
        .collect();
    Ok(json!({
        "command": "list_parts",
        "status": "ok",
        "id": id,
        "count": parts.len(),
        "parts": parts,
        "orphans": orphans,
    }))
}

pub(crate) fn cmd_get_part(
    args: GetPartArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let report = load_parts_report(store, &id)?;
    let part = report.find(args.part_id).ok_or_else(|| {
        CliRunError::BadRequest(format!(
            "part '{}' was not found on ticket '{id}'",
            args.part_id
        ))
    })?;
    Ok(json!({
        "command": "get_part",
        "status": "ok",
        "id": id,
        "part": part_json(part, true),
    }))
}

fn find_part_in_manifest(
    manifest: &TicketManifest,
    part_id: Uuid,
) -> Option<Value> {
    manifest.parts().into_iter().find(|p| p.id == part_id).map(|p| {
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

pub(crate) fn cmd_write_part(
    args: WritePartArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let author = super::resolve_author(args.author.as_deref());
    let content = resolve_content(args.content, args.content_file)?;
    let part_id = args.part_id.unwrap_or_else(Uuid::new_v4);
    let manifest = store.write_part(
        &id,
        part_id,
        &args.kind,
        &content,
        author.as_deref(),
    )?;
    Ok(json!({
        "command": "write_part",
        "status": "ok",
        "id": id,
        "part": find_part_in_manifest(&manifest, part_id),
    }))
}

pub(crate) fn cmd_write_amendment(
    args: WriteAmendmentArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let author = super::resolve_author(args.author.as_deref());
    let content = resolve_content(args.content, args.content_file)?;
    let part_id = args.part_id.unwrap_or_else(Uuid::new_v4);
    let manifest = store.write_amendment_part(
        &id,
        part_id,
        &content,
        args.supersedes,
        author.as_deref(),
    )?;
    Ok(json!({
        "command": "write_amendment",
        "status": "ok",
        "id": id,
        "supersedes": args.supersedes,
        "part": find_part_in_manifest(&manifest, part_id),
    }))
}

pub(crate) fn cmd_undo_part(
    args: UndoPartArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let id = super::resolve_uuid_prefix(&args.id, store)?;
    let author = super::resolve_author(args.author.as_deref());
    let manifest = store.undo_part(&id, args.part_id, author.as_deref())?;
    Ok(json!({
        "command": "undo_part",
        "status": "ok",
        "id": id,
        "part": find_part_in_manifest(&manifest, args.part_id),
    }))
}
