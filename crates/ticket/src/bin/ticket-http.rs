use std::net::SocketAddr;
use transport_harness::http::{
    HttpError,
    Router,
    StatusCode,
    axum::{
        Json,
        extract::{Path, Query},
        response::{IntoResponse, Response},
        routing::get,
    },
};

#[derive(serde::Deserialize)]
struct GetQuery {
    store_path: String,
}

async fn get_ticket(
    Path(id): Path<String>,
    Query(query): Query<GetQuery>,
) -> Response {
    let store = match ticket::storage::TicketStore::open(std::path::Path::new(&query.store_path)) {
        Ok(store) => store,
        Err(e) => {
            return HttpError::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                "store_error",
                format!("failed to open store: {e}"),
            )
            .into_response()
        }
    };
    
    let uuid = match id.parse::<uuid::Uuid>() {
        Ok(uuid) => uuid,
        Err(e) => {
            return HttpError::new(
                StatusCode::BAD_REQUEST,
                "invalid_id",
                format!("invalid ticket id: {e}"),
            )
            .into_response()
        }
    };
    
    match store.get(&uuid) {
        Ok(ticket) => Json(ticket).into_response(),
        Err(e) => HttpError::new(
            StatusCode::NOT_FOUND,
            "not_found",
            format!("ticket not found: {e}"),
        )
        .into_response(),
    }
}

fn router() -> Router {
    Router::new().route("/ticket/{id}", get(get_ticket))
}

fn main() {
    tokio::runtime::Runtime::new()
        .expect("failed to create runtime")
        .block_on(async {
            let addr = SocketAddr::from(([127, 0, 0, 1], 3010));
            println!("ticket-http listening on {addr}");
            
            if let Err(err) = transport_harness::http::run(addr, router()) {
                eprintln!("Fatal error: {err}");
                std::process::exit(1);
            }
        });
}
