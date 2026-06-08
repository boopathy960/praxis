// ─────────────────────────────────────────────────────────────
// HTTPA/1.0 — Internal Message Bus
// ─────────────────────────────────────────────────────────────
// Every subsystem registers as an HTTPA endpoint.
// Intents are routed via tokio mpsc channels.

use super::protocol::{HttpaFrame, IntentPriority};
use log::{debug, info, warn};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Maximum channel buffer size per subscriber.
const CHANNEL_BUFFER: usize = 256;

/// A subscription handle that receives frames.
pub type FrameReceiver = mpsc::Receiver<HttpaFrame>;
pub type FrameSender = mpsc::Sender<HttpaFrame>;

/// Route target — which subsystem handles which intent domains.
#[derive(Debug, Clone)]
pub struct RouteEntry {
    pub name: String,
    pub domains: Vec<String>,
    pub sender: FrameSender,
}

/// The HTTPA message bus — backbone of the engine.
///
/// All components communicate through this bus using typed HTTPA frames.
/// Intent routing dispatches frames to the correct handler based on domain.
pub struct HttpaBus {
    routes: Arc<RwLock<HashMap<String, RouteEntry>>>,
    broadcast_senders: Arc<RwLock<Vec<FrameSender>>>,
    /// Priority channel for critical intents (auth, purchase).
    priority_sender: Option<FrameSender>,
    priority_receiver: Option<FrameReceiver>,
    metrics: Arc<BusMetrics>,
}

/// Bus performance metrics.
pub struct BusMetrics {
    pub frames_routed: std::sync::atomic::AtomicU64,
    pub frames_broadcast: std::sync::atomic::AtomicU64,
    pub frames_dropped: std::sync::atomic::AtomicU64,
}

impl Default for BusMetrics {
    fn default() -> Self {
        Self {
            frames_routed: std::sync::atomic::AtomicU64::new(0),
            frames_broadcast: std::sync::atomic::AtomicU64::new(0),
            frames_dropped: std::sync::atomic::AtomicU64::new(0),
        }
    }
}

impl HttpaBus {
    /// Create a new HTTPA message bus.
    pub fn new() -> Self {
        let (ptx, prx) = mpsc::channel(CHANNEL_BUFFER);
        Self {
            routes: Arc::new(RwLock::new(HashMap::new())),
            broadcast_senders: Arc::new(RwLock::new(Vec::new())),
            priority_sender: Some(ptx),
            priority_receiver: Some(prx),
            metrics: Arc::new(BusMetrics::default()),
        }
    }

    /// Register a subsystem as an HTTPA endpoint.
    ///
    /// Returns a receiver that the subsystem uses to receive dispatched frames.
    pub async fn register(&self, name: impl Into<String>, domains: Vec<String>) -> FrameReceiver {
        let name = name.into();
        let (tx, rx) = mpsc::channel(CHANNEL_BUFFER);

        let entry = RouteEntry {
            name: name.clone(),
            domains: domains.clone(),
            sender: tx,
        };

        let mut routes = self.routes.write().await;
        for domain in &domains {
            routes.insert(domain.clone(), entry.clone());
        }

        info!(
            "📡 HTTPA Bus: registered '{}' for domains {:?}",
            name, domains
        );
        rx
    }

    /// Subscribe to all broadcast frames (e.g., for observability).
    pub async fn subscribe_broadcast(&self) -> FrameReceiver {
        let (tx, rx) = mpsc::channel(CHANNEL_BUFFER);
        let mut senders = self.broadcast_senders.write().await;
        senders.push(tx);
        rx
    }

    /// Take ownership of the priority receiver (for the priority handler).
    pub fn take_priority_receiver(&mut self) -> Option<FrameReceiver> {
        self.priority_receiver.take()
    }

    /// Route a frame to the appropriate handler based on intent/domain.
    pub async fn route(&self, frame: HttpaFrame) {
        use std::sync::atomic::Ordering;

        // Critical priority → priority channel
        if frame.priority == IntentPriority::Critical {
            if let Some(ref ptx) = self.priority_sender {
                if ptx.send(frame.clone()).await.is_err() {
                    warn!("📡 HTTPA Bus: priority channel full, dropping frame");
                    self.metrics.frames_dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
        }

        // Route by intent domain
        let domain = frame.intent.as_deref().unwrap_or("general");
        let routes = self.routes.read().await;

        if let Some(entry) = routes.get(domain) {
            debug!("📡 HTTPA Bus: routing '{}' → '{}'", domain, entry.name);
            if entry.sender.send(frame.clone()).await.is_err() {
                warn!("📡 HTTPA Bus: handler '{}' channel full", entry.name);
                self.metrics.frames_dropped.fetch_add(1, Ordering::Relaxed);
            } else {
                self.metrics.frames_routed.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            debug!(
                "📡 HTTPA Bus: no handler for domain '{}', broadcasting",
                domain
            );
        }

        // Broadcast to all subscribers (observability), cleaning up dead channels
        {
            let mut senders = self.broadcast_senders.write().await;
            let mut i = 0;
            let mut broadcast_count = 0u64;
            while i < senders.len() {
                if senders[i].is_closed() {
                    senders.swap_remove(i);
                } else {
                    let _ = senders[i].try_send(frame.clone());
                    broadcast_count += 1;
                    i += 1;
                }
            }
            self.metrics
                .frames_broadcast
                .fetch_add(broadcast_count, std::sync::atomic::Ordering::Relaxed);
        }
    }

    /// Emit a frame (convenience wrapper around route).
    pub async fn emit(&self, frame: HttpaFrame) {
        self.route(frame).await;
    }

    /// Get bus metrics.
    pub fn get_metrics(&self) -> BusStats {
        use std::sync::atomic::Ordering;
        BusStats {
            frames_routed: self.metrics.frames_routed.load(Ordering::Relaxed),
            frames_broadcast: self.metrics.frames_broadcast.load(Ordering::Relaxed),
            frames_dropped: self.metrics.frames_dropped.load(Ordering::Relaxed),
        }
    }
}

/// Snapshot of bus statistics.
#[derive(Debug, Clone, serde::Serialize)]
pub struct BusStats {
    pub frames_routed: u64,
    pub frames_broadcast: u64,
    pub frames_dropped: u64,
}
