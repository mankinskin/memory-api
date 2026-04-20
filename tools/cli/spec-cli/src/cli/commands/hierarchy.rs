use serde_json::{Value, json};

use spec_api::SpecStore;

use crate::cli::{CliRunError, TreeArgs};

pub(crate) fn cmd_tree(args: TreeArgs, store: &SpecStore) -> Result<Value, CliRunError> {
    if let Some(root_id) = &args.id {
        let root = store.get(root_id)?;
        let descendants = store.subtree(root_id)?;
        Ok(json!({
            "command": "tree",
            "status": "ok",
            "root": {
                "id": root.id,
                "slug": root.slug(),
                "title": root.title(),
                "state": root.state(),
            },
            "descendants": descendants.iter().map(|c| json!({
                "id": c.id,
                "slug": c.slug(),
                "title": c.title(),
                "state": c.state(),
                "parent": c.parent(),
            })).collect::<Vec<_>>(),
        }))
    } else {
        // Show all root specs (no parent) with their direct child counts
        let all = store.entity_store().list_indexed(false)?;
        let mut roots = Vec::new();
        for indexed in &all {
            if let Ok(spec) = store.get(&indexed.id.to_string()) {
                if spec.parent().is_none() {
                    let children = store.children(&indexed.id.to_string())?;
                    roots.push(json!({
                        "id": spec.id,
                        "slug": spec.slug(),
                        "title": spec.title(),
                        "children_count": children.len(),
                    }));
                }
            }
        }
        Ok(json!({
            "command": "tree",
            "status": "ok",
            "roots": roots,
        }))
    }
}
