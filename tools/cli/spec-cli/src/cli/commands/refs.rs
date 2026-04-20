use serde_json::{Value, json};

use spec_api::SpecStore;
use spec_api::code_ref::validate_refs;

use crate::cli::{CliRunError, RefsArgs, RefsSubcommand};

pub(crate) fn cmd_refs(args: RefsArgs, store: &SpecStore) -> Result<Value, CliRunError> {
    let spec = store.get(&args.id)?;

    match args.subcommand {
        Some(RefsSubcommand::Validate { workspace_root }) => {
            let results = validate_refs(&spec.code_refs, &workspace_root);
            let items: Vec<Value> = results
                .iter()
                .map(|r| {
                    json!({
                        "file": r.code_ref.file,
                        "symbol": r.code_ref.symbol,
                        "kind": format!("{:?}", r.code_ref.kind),
                        "file_exists": r.file_exists,
                        "line_range_valid": r.line_range_valid,
                        "message": r.message,
                    })
                })
                .collect();
            let all_valid = results.iter().all(|r| r.file_exists && r.line_range_valid);
            Ok(json!({
                "command": "refs_validate",
                "status": "ok",
                "id": spec.id,
                "valid": all_valid,
                "count": items.len(),
                "results": items,
            }))
        }
        None => {
            let refs: Vec<Value> = spec
                .code_refs
                .iter()
                .map(|r| {
                    json!({
                        "file": r.file,
                        "symbol": r.symbol,
                        "kind": format!("{:?}", r.kind),
                        "line_start": r.line_start,
                        "line_end": r.line_end,
                        "description": r.description,
                    })
                })
                .collect();
            Ok(json!({
                "command": "refs",
                "status": "ok",
                "id": spec.id,
                "count": refs.len(),
                "refs": refs,
            }))
        }
    }
}
