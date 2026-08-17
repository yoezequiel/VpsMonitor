use std::sync::Arc;

use chrono::Duration;
use serde::Serialize;
use shared::ServerStatus;
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::store::Store;

#[derive(Debug, Clone, Serialize)]
pub struct ServerStatusEvent {
    pub server_id: Uuid,
    pub status: ServerStatus,
}

#[derive(Clone)]
pub struct AppState {
    pub store: Arc<dyn Store>,
    pub status_tx: broadcast::Sender<ServerStatusEvent>,
    /// How far back `/api/servers/:id/metrics` still reads from raw
    /// samples before falling back to hourly rollups — kept in sync with
    /// whatever the scheduler's `rollup_and_prune` job actually uses.
    pub raw_metrics_retention: Duration,
}

impl AppState {
    pub fn new(store: Arc<dyn Store>) -> Self {
        let (status_tx, _) = broadcast::channel(256);
        Self {
            store,
            status_tx,
            raw_metrics_retention: Duration::hours(24),
        }
    }

    pub fn with_raw_metrics_retention(mut self, retention: Duration) -> Self {
        self.raw_metrics_retention = retention;
        self
    }
}
