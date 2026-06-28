//! Criterion benchmark: scan throughput over the generated large fixture variant.
//!
//! Materializes the shared multi-store fixture with a benchmark-scale set of
//! generated tickets and measures full reindex (`scan(true)`) over the root
//! ticket store. Complements `graph_ops` by exercising the on-disk discovery
//! + index path against the canonical fixture.

use criterion::{
    Criterion,
    criterion_group,
    criterion_main,
};
use memory_fixtures::materialize_fixture_with_generated_tickets;
use ticket_api::storage::store::TicketStore;

fn bench_fixture_scan(c: &mut Criterion) {
    let generated = 200usize;

    c.bench_function("fixture_scan_reindex_root_store", |b| {
        b.iter_batched(
            || {
                let fixture = materialize_fixture_with_generated_tickets(generated)
                    .expect("fixture should materialize");
                let store_root = fixture
                    .store_root("ticket-root")
                    .expect("ticket-root path")
                    .to_path_buf();
                let store = TicketStore::open_or_init(&store_root).expect("open_or_init");
                (fixture, store)
            },
            |(fixture, store)| {
                store.scan(true).expect("scan should index generated tickets");
                drop(fixture);
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

criterion_group!(benches, bench_fixture_scan);
criterion_main!(benches);
