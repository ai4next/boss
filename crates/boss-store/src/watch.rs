use std::pin::Pin;
use std::sync::Arc;

use boss_api::ResourceVersion;
use parking_lot::Mutex;
use tokio::sync::broadcast;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::{Stream, StreamExt};

use crate::storage::{WatchEvent, extract_rv};

/// Internal bus event: the watch event plus its store key (for prefix filtering).
#[derive(Clone, Debug)]
pub struct BusEvent {
    pub key: String,
    pub event: WatchEvent,
}

const HISTORY_CAPACITY: usize = 4096;

/// A broadcast-based watch bus with a bounded history ring buffer for replay.
///
/// Every create/update/delete pushes an event to live subscribers and into the
/// history. Subscribers first receive matching history events (rv > start_rv),
/// then live events — so a watch from `resourceVersion=0` replays recent state.
pub struct WatchBus {
    tx: broadcast::Sender<BusEvent>,
    history: Arc<Mutex<std::collections::VecDeque<BusEvent>>>,
}

impl WatchBus {
    pub fn new(capacity: usize) -> Self {
        let (tx, _rx) = broadcast::channel(capacity);
        Self {
            tx,
            history: Arc::new(Mutex::new(std::collections::VecDeque::with_capacity(
                HISTORY_CAPACITY,
            ))),
        }
    }

    pub fn publish(&self, key: String, event: WatchEvent) {
        let ev = BusEvent { key, event };
        let _ = self.tx.send(ev.clone());
        let mut hist = self.history.lock();
        if hist.len() >= HISTORY_CAPACITY {
            hist.pop_front();
        }
        hist.push_back(ev);
    }

    /// Subscribe to events whose key starts with `prefix` and whose
    /// `resourceVersion > start_rv`. History is replayed first (snapshot taken
    /// after subscribing, with dedup by rv), then live events follow.
    pub fn subscribe(
        &self,
        prefix: String,
        start_rv: ResourceVersion,
    ) -> Pin<Box<dyn Stream<Item = WatchEvent> + Send>> {
        // 1. Subscribe to live first so the race window is covered by history.
        let rx = self.tx.subscribe();
        // 2. Snapshot history after subscribing.
        let snapshot: Vec<BusEvent> = {
            let hist = self.history.lock();
            hist.iter()
                .filter(|b| {
                    b.key.starts_with(&prefix)
                        && extract_rv(b.event.object()).unwrap_or(ResourceVersion(0)) > start_rv
                })
                .cloned()
                .collect()
        };
        let max_hist_rv = snapshot
            .iter()
            .map(|b| extract_rv(b.event.object()).unwrap_or(ResourceVersion(0)))
            .max()
            .unwrap_or(start_rv);

        let prefix_live = prefix.clone();
        let live = BroadcastStream::new(rx).filter_map(move |res| match res {
            Ok(BusEvent { key, event }) => {
                if !key.starts_with(&prefix_live) {
                    return None;
                }
                let rv = extract_rv(event.object()).unwrap_or(ResourceVersion(0));
                // Dedup: skip live events already covered by history snapshot.
                if rv > max_hist_rv && rv > start_rv {
                    Some(event)
                } else {
                    None
                }
            }
            // lagged: drop silently; client should re-list on disconnect.
            Err(_lagged) => None,
        });

        let history_stream = futures::stream::iter(snapshot.into_iter().map(|b| b.event));

        Box::pin(history_stream.chain(live))
    }
}
