//! Per-workspace broadcast channel fan-out broker.

use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
};

use tokio::sync::broadcast;

use super::event::SseEvent;

/// Default broadcast channel capacity per workspace.
/// On overflow, lagging receivers are dropped (no backpressure on writers).
const CHANNEL_CAPACITY: usize = 256;

/// A monotonically increasing event counter shared across all workspaces.
static GLOBAL_EVENT_ID: AtomicU64 = AtomicU64::new(1);

/// Allocate the next event ID.
pub fn next_event_id() -> u64 {
    GLOBAL_EVENT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Fan-out broker: holds one `broadcast::Sender<SseEvent>` per workspace.
///
/// Multiple SSE clients for the same workspace share one sender; each gets a
/// `Receiver` copy.  The sender is created lazily on first subscribe or emit.
pub struct StreamBroker {
    channels: Mutex<HashMap<String, broadcast::Sender<(u64, SseEvent)>>>,
}

impl StreamBroker {
    pub fn new() -> Self {
        Self {
            channels: Mutex::new(HashMap::new()),
        }
    }

    /// Subscribe a new receiver for `workspace`.  Creates the channel if absent.
    pub fn subscribe(
        &self,
        workspace: &str,
    ) -> broadcast::Receiver<(u64, SseEvent)> {
        let mut map = self.channels.lock().unwrap();
        map.entry(workspace.to_string())
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0)
            .subscribe()
    }

    /// Broadcast an event to all active receivers for `workspace`.
    ///
    /// Non-blocking: if no receivers exist or the channel is full, the event is
    /// silently dropped (metrics are the caller's responsibility).
    pub fn emit(&self, workspace: &str, event: SseEvent) {
        let map = self.channels.lock().unwrap();
        if let Some(tx) = map.get(workspace) {
            let id = next_event_id();
            let _ = tx.send((id, event));
        }
        // If no channel exists yet nobody is listening — skip.
    }

    /// Ensure a channel exists for `workspace` (call when a workspace is opened).
    pub fn ensure_channel(&self, workspace: &str) {
        let mut map = self.channels.lock().unwrap();
        map.entry(workspace.to_string())
            .or_insert_with(|| broadcast::channel(CHANNEL_CAPACITY).0);
    }

    /// Number of active subscribers for a specific workspace.
    pub fn workspace_subscriber_count(&self, workspace: &str) -> usize {
        self.channels
            .lock()
            .unwrap()
            .get(workspace)
            .map(|tx| tx.receiver_count())
            .unwrap_or(0)
    }

    /// Number of active subscribers across all workspaces.
    pub fn total_subscriber_count(&self) -> usize {
        self.channels
            .lock()
            .unwrap()
            .values()
            .map(|tx| tx.receiver_count())
            .sum()
    }
}

impl Default for StreamBroker {
    fn default() -> Self {
        Self::new()
    }
}
