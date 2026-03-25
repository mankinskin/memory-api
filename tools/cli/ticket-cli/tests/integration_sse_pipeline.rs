//! Integration tests for the SSE pipeline:
//!   `TicketStore` mutation → `HookEmitter` → `StreamBroker` → `broadcast::Receiver`
//!
//! These tests verify that ticket/edge mutations emit the correct SSE events
//! without starting an HTTP server.  They use the broker directly.

mod common;

use std::{collections::BTreeMap, sync::Arc};

use ticket_api::{
    model::{edge::EdgeRecord, filesystem::ScanRoot},
    storage::store::TicketStore,
};
use ticket_http::serve::stream::{HookEmitter, StreamBroker, event::SseEvent};
use tempfile::TempDir;

fn open_store_with_broker(
    dir: &TempDir,
    workspace: &str,
    broker: Arc<StreamBroker>,
) -> Arc<TicketStore> {
    let store = Arc::new(TicketStore::open(dir.path()).expect("open store"));
    let emitter = HookEmitter::new(workspace, Arc::clone(&broker));
    store.set_hook(emitter);
    store
}

#[test]
fn ticket_create_emits_upsert_event() {
    let dir = TempDir::new().unwrap();
    let broker = Arc::new(StreamBroker::new());
    let ws = "ws1";
    let mut rx = broker.subscribe(ws);

    let store = open_store_with_broker(&dir, ws, Arc::clone(&broker));
    // Register a scan root so create() can find a target directory.
    store
        .add_scan_root(ScanRoot { path: dir.path().join("tickets"), label: "default".into() })
        .unwrap();

    store
        .create(None, "task", Some("My ticket"), Some("open"), BTreeMap::new(), None, None)
        .unwrap();

    let (_, event) = rx.try_recv().expect("should have received an SSE event");
    match event {
        SseEvent::TicketUpsert(p) => {
            assert_eq!(p.workspace, ws);
            assert_eq!(p.ticket.title.as_deref(), Some("My ticket"));
        }
        other => panic!("expected TicketUpsert, got {other:?}"),
    }
}

#[test]
fn ticket_delete_emits_delete_event() {
    let dir = TempDir::new().unwrap();
    let broker = Arc::new(StreamBroker::new());
    let ws = "ws2";
    let mut rx = broker.subscribe(ws);

    let store = open_store_with_broker(&dir, ws, Arc::clone(&broker));
    store
        .add_scan_root(ScanRoot { path: dir.path().join("tickets"), label: "default".into() })
        .unwrap();

    let id = store
        .create(None, "task", Some("Doomed ticket"), Some("open"), BTreeMap::new(), None, None)
        .unwrap();

    // Consume the upsert event from create.
    rx.try_recv().unwrap();

    store.delete(&id).unwrap();

    let (_, event) = rx.try_recv().expect("should have received a delete SSE event");
    match event {
        SseEvent::TicketDelete(p) => {
            assert_eq!(p.id, id);
            assert_eq!(p.workspace, ws);
        }
        other => panic!("expected TicketDelete, got {other:?}"),
    }
}

#[test]
fn add_edge_emits_edge_upsert_event() {
    let dir = TempDir::new().unwrap();
    let broker = Arc::new(StreamBroker::new());
    let ws = "ws3";
    let mut rx = broker.subscribe(ws);

    let store = open_store_with_broker(&dir, ws, Arc::clone(&broker));
    store
        .add_scan_root(ScanRoot { path: dir.path().join("tickets"), label: "default".into() })
        .unwrap();

    let from = store
        .create(None, "task", Some("A"), Some("open"), BTreeMap::new(), None, None)
        .unwrap();
    let to = store
        .create(None, "task", Some("B"), Some("open"), BTreeMap::new(), None, None)
        .unwrap();

    // Drain create events.
    rx.try_recv().unwrap();
    rx.try_recv().unwrap();

    store
        .add_edge(EdgeRecord { from, to, kind: "depends_on".into(), created_at: chrono::Utc::now() })
        .unwrap();

    let (_, event) = rx.try_recv().expect("should have received an edge SSE event");
    match event {
        SseEvent::EdgeUpsert(p) => {
            assert_eq!(p.edge.from, from);
            assert_eq!(p.edge.to, to);
            assert_eq!(p.edge.kind, "depends_on");
        }
        other => panic!("expected EdgeUpsert, got {other:?}"),
    }
}
