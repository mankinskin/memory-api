use transport_harness::{
    HarnessError,
    Output,
    cli::clap::{
        self,
        Parser,
    },
};

#[derive(Parser)]
#[command(name = "ticket-cli")]
struct TicketCommand {
    #[command(subcommand)]
    op: Op,
}

#[derive(clap::Subcommand)]
enum Op {
    /// Get a ticket by ID.
    Get {
        #[arg(long)]
        id: String,
        #[arg(long)]
        store_path: String,
    },
}

fn dispatch(command: TicketCommand) -> Result<Output, HarnessError> {
    match command.op {
        Op::Get { id, store_path } => {
            let store = ticket::storage::TicketStore::open(std::path::Path::new(&store_path))
                .map_err(|e| HarnessError::domain(format!("failed to open store: {e}")))?;
            
            let uuid = id.parse::<uuid::Uuid>()
                .map_err(|e| HarnessError::domain(format!("invalid ticket id: {e}")))?;
            
            let ticket = store
                .get(&uuid)
                .map_err(|e| HarnessError::domain(format!("ticket not found: {e}")))?;
            
            Output::json(&ticket)
        }
    }
}

fn main() -> Result<(), HarnessError> {
    transport_harness::cli::run::<TicketCommand, _>(dispatch)
}
