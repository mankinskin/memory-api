use memory_fixtures::materialize_fixture;
use ticket_api::storage::TicketStore;
use uuid::Uuid;

#[test]
fn ticket_store_reads_seeded_root_ticket_from_materialized_fixture() {
    let fixture = materialize_fixture().expect("fixture should materialize");
    let store_root = fixture
        .store_root("ticket-root")
        .expect("ticket-root store path should exist");

    let store = TicketStore::open_or_init(store_root)
        .expect("open_or_init should work");
    store
        .scan(true)
        .expect("scan should index seeded manifests");

    let id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
    let manifest = store
        .get(&id)
        .expect("seeded root ticket should be readable");
    assert_eq!(
        manifest.extra.get("state").and_then(|v| v.as_str()),
        Some("new")
    );
    assert_eq!(
        manifest.extra.get("title").and_then(|v| v.as_str()),
        Some("Root fixture ticket")
    );
}
