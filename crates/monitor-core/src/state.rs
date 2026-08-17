use std::sync::Arc;

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
}

impl AppState {
    pub fn new(store: Arc<dyn Store>) -> Self {
        let (status_tx, _) = broadcast::channel(256);
        Self { store, status_tx }
    }
}
