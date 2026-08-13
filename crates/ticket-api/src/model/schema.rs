pub use memory_kernel::model::schema::*;
// Backward-compatible alias: downstream code uses TicketTypeSchema.
pub use memory_kernel::model::schema::EntityTypeSchema as TicketTypeSchema;

pub trait TicketTypeSchemaExt {
	fn entry_state(&self) -> Option<&str>;
}

impl TicketTypeSchemaExt for TicketTypeSchema {
	fn entry_state(&self) -> Option<&str> {
		self.states
			.iter()
			.find(|state| state.as_str() == "open")
			.map(String::as_str)
			.or_else(|| self.states.first().map(String::as_str))
	}
}
