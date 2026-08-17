//! Periodic jobs. The MVP only needs heartbeat-timeout detection; health
//! check execution and alert rule evaluation join this module in V2.

use std::time::Duration;

use chrono::Utc;
use shared::ServerStatus;

use crate::state::{AppState, ServerStatusEvent};

/// Every `poll_interval`, flips any server whose last heartbeat is older
/// than `heartbeat_timeout` to offline and broadcasts the change over the
/// WebSocket hub. Runs until the process exits.
pub async fn run_offline_detector(
    state: AppState,
    heartbeat_timeout: Duration,
    poll_interval: Duration,
) {
    let mut ticker = tokio::time::interval(poll_interval);
    loop {
        ticker.tick().await;

        let threshold = Utc::now()
            - chrono::Duration::from_std(heartbeat_timeout)
                .unwrap_or(chrono::Duration::seconds(60));
        match state.store.mark_servers_offline(threshold).await {
            Ok(ids) => {
                for server_id in ids {
                    tracing::info!(%server_id, "server marked offline (heartbeat timeout)");
                    let _ = state.status_tx.send(ServerStatusEvent {
                        server_id,
                        status: ServerStatus::Offline,
                    });
                }
            }
            Err(err) => tracing::error!(%err, "offline detector: failed to query store"),
        }
    }
}

pub fn spawn_offline_detector(
    state: AppState,
    heartbeat_timeout: Duration,
    poll_interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_offline_detector(
        state,
        heartbeat_timeout,
        poll_interval,
    ))
}
