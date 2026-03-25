use serde_json::{Value, json};

use ticket_api::storage::TicketStore;

use crate::cli::{CliRunError, TextArgs};

pub(crate) fn cmd_search(args: TextArgs, store: &TicketStore) -> Result<Value, CliRunError> {
    let results = store.search_tickets(&args.expression, args.limit)?;
    let items: Vec<Value> = results
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "title": r.title,
                "state": r.state,
                "type": r.ticket_type,
                "snippet": r.snippet,
                "score": r.score,
            })
        })
        .collect();
    Ok(json!({
        "command": "search",
        "status": "ok",
        "query": args.expression,
        "count": items.len(),
        "results": items,
    }))
}
