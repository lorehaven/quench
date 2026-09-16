use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Tracks liveness/readiness independently of the HTTP framework in front
/// of it - both `actix::routers::health` and `http::routers::health` wrap
/// the same state.
#[derive(Clone, Default)]
pub struct HealthState {
    live: Arc<AtomicBool>,
    ready: Arc<AtomicBool>,
}

impl HealthState {
    pub fn live() -> Self {
        Self {
            live: Arc::new(AtomicBool::new(true)),
            ready: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn is_live(&self) -> bool {
        self.live.load(Ordering::Acquire)
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    pub fn mark_ready(&self) {
        self.ready.store(true, Ordering::Release);
    }

    /// Spawns `init`, then marks this ready once it completes - so `/health`
    /// answers live-but-not-ready while dependency waits and one-time setup
    /// run, instead of blocking startup on them.
    pub fn spawn_ready_when<F>(&self, init: F)
    where
        F: Future<Output = ()> + Send + 'static,
    {
        let state = self.clone();
        tokio::spawn(async move {
            init.await;
            state.mark_ready();
            tracing::info!("Service initialization complete");
        });
    }
}
