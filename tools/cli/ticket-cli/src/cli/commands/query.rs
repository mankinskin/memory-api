use serde_json::{
    Value,
    json,
};

use ticket_api::storage::TicketStore;

use crate::cli::{
    CliRunError,
    TextArgs,
};
use crate::cli::commands::ticket_workspace_metadata_for_id;

pub(crate) fn cmd_search(
    args: TextArgs,
    store: &TicketStore,
) -> Result<Value, CliRunError> {
    let results = store.search_tickets(&args.expression, args.limit)?;
    let mut items: Vec<Value> = Vec::with_capacity(results.len());
    for result in results {
        items.push(json!({
            "id": result.id,
            "title": result.title,
            "state": result.state,
            "type": result.ticket_type,
            "snippet": result.snippet,
            "score": result.score,
            "workspace": ticket_workspace_metadata_for_id(store, result.id),
        }));
    }
    Ok(json!({
        "command": "search",
        "status": "ok",
        "query": args.expression,
        "count": items.len(),
        "results": items,
    }))
}
