use serde_json::{Value, json};

use spec_api::SpecStore;

use crate::cli::{CliRunError, SectionArgs, SectionCommand};

pub(crate) fn cmd_section(args: SectionArgs, store: &mut SpecStore) -> Result<Value, CliRunError> {
    match args.command {
        SectionCommand::Add { id, name, file } => {
            let content = std::fs::read_to_string(&file)
                .map_err(|e| CliRunError::BadRequest(format!("cannot read file: {e}")))?;
            store.add_section(&id, &name, &content)?;
            Ok(json!({
                "command": "section_add",
                "status": "ok",
                "spec": id,
                "section": name,
            }))
        }
        SectionCommand::List { id } => {
            let sections = store.list_sections(&id)?;
            Ok(json!({
                "command": "section_list",
                "status": "ok",
                "spec": id,
                "count": sections.len(),
                "sections": sections,
            }))
        }
        SectionCommand::Get { id, name } => {
            let uuid = store.resolve_id(&id)?;
            let indexed = store
                .entity_store()
                .get_indexed(&uuid)?
                .ok_or_else(|| CliRunError::BadRequest("spec not found".to_string()))?;
            let file_name = if name.ends_with(".md") {
                name.clone()
            } else {
                format!("{}.md", name)
            };
            let path = indexed.path.join("sections").join(&file_name);
            let content = std::fs::read_to_string(&path)
                .map_err(|e| CliRunError::BadRequest(format!("section not found: {e}")))?;
            Ok(json!({
                "command": "section_get",
                "status": "ok",
                "spec": id,
                "section": name,
                "content": content,
            }))
        }
        SectionCommand::Delete { id, name } => {
            store.delete_section(&id, &name)?;
            Ok(json!({
                "command": "section_delete",
                "status": "ok",
                "spec": id,
                "section": name,
            }))
        }
    }
}
