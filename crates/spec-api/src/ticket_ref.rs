use serde::{
    Deserialize,
    Serialize,
};
use uuid::Uuid;

/// A structured reference from a spec to a ticket, carried on
/// [`crate::SpecManifest::related_tickets`].
///
/// Always carries an explicit workspace/store identifier so link validation
/// never has to guess which store a reference resolves against. This is the
/// direct fix for the nested-store bug, where a path relative to the
/// referencing spec file silently resolved against the wrong `.ticket`
/// store.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TicketRef {
    /// Ticket UUID.
    pub ticket_id: Uuid,
    /// Named workspace the ticket store belongs to (matches ticket-api
    /// workspace resolution, e.g. "default", "memory-api").
    pub workspace: String,
    /// Store root the ticket resolves against, repo-root-relative
    /// (e.g. ".ticket", "memory-api/.ticket"). Never a path relative to
    /// the referencing spec file.
    pub store_root: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticket_ref_round_trips_through_toml() {
        let ticket_ref = TicketRef {
            ticket_id: Uuid::nil(),
            workspace: "default".to_string(),
            store_root: ".ticket".to_string(),
        };

        let toml_str = toml::to_string(&ticket_ref).unwrap();
        let parsed: TicketRef = toml::from_str(&toml_str).unwrap();

        assert_eq!(parsed, ticket_ref);
    }

    #[test]
    fn ticket_ref_round_trips_as_array_via_json() {
        // Mirrors how `SpecManifest::related_tickets` stores entries inside
        // the `extra` map (as a `serde_json::Value::Array`) before the
        // manifest is rendered to TOML text.
        let refs = vec![
            TicketRef {
                ticket_id: Uuid::nil(),
                workspace: "default".to_string(),
                store_root: ".ticket".to_string(),
            },
            TicketRef {
                ticket_id: Uuid::new_v4(),
                workspace: "memory-api".to_string(),
                store_root: "memory-api/.ticket".to_string(),
            },
        ];

        let value = serde_json::to_value(&refs).unwrap();
        let parsed: Vec<TicketRef> = serde_json::from_value(value).unwrap();

        assert_eq!(parsed, refs);
    }
}
