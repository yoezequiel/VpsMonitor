//! Periodic jobs: heartbeat-timeout detection, metrics downsampling, and
//! health check execution. Alert rule evaluation joins this module next.

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

/// Every `poll_interval`, rolls raw metrics older than `raw_retention`
/// into hourly buckets and prunes rollups older than `rollup_retention` —
/// see `Store::rollup_and_prune`. Keeps `metrics` bounded no matter how
/// long a server has been monitored.
pub async fn run_metrics_rollup(
    state: AppState,
    raw_retention: Duration,
    rollup_retention: Duration,
    poll_interval: Duration,
) {
    let mut ticker = tokio::time::interval(poll_interval);
    loop {
        ticker.tick().await;

        let raw_cutoff = Utc::now()
            - chrono::Duration::from_std(raw_retention).unwrap_or(chrono::Duration::hours(24));
        let rollup_cutoff = Utc::now()
            - chrono::Duration::from_std(rollup_retention).unwrap_or(chrono::Duration::days(30));

        match state
            .store
            .rollup_and_prune(raw_cutoff, rollup_cutoff)
            .await
        {
            Ok((rolled_up, raw_deleted, rollups_pruned)) if rolled_up > 0 || rollups_pruned > 0 => {
                tracing::info!(
                    rolled_up,
                    raw_deleted,
                    rollups_pruned,
                    "metrics downsampling pass complete"
                );
            }
            Ok(_) => {}
            Err(err) => tracing::error!(%err, "metrics rollup: failed to query store"),
        }
    }
}

pub fn spawn_metrics_rollup(
    state: AppState,
    raw_retention: Duration,
    rollup_retention: Duration,
    poll_interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_metrics_rollup(
        state,
        raw_retention,
        rollup_retention,
        poll_interval,
    ))
}

/// Every `poll_interval`, runs any health check whose own `interval_s` has
/// elapsed since its last result (or that has never run). One flat poll
/// loop rather than a timer per check — simpler, and fine at the scale a
/// self-hosted install runs at.
pub async fn run_health_checks(state: AppState, poll_interval: Duration) {
    let mut ticker = tokio::time::interval(poll_interval);
    loop {
        ticker.tick().await;

        let checks = match state.store.list_health_checks().await {
            Ok(checks) => checks,
            Err(err) => {
                tracing::error!(%err, "health checks: failed to list checks");
                continue;
            }
        };

        for check in checks {
            let last_run = match state.store.last_health_check_result_at(check.id).await {
                Ok(last_run) => last_run,
                Err(err) => {
                    tracing::error!(%err, check = %check.name, "health checks: failed to read last result");
                    continue;
                }
            };
            let due = match last_run {
                None => true,
                Some(last) => {
                    Utc::now() - last >= chrono::Duration::seconds(check.interval_s as i64)
                }
            };
            if !due {
                continue;
            }

            let result = crate::healthcheck::run_check(&check).await;
            if !result.ok {
                tracing::warn!(check = %check.name, target = %check.target, "health check failing");
            }
            if let Err(err) = state.store.insert_health_check_result(&result).await {
                tracing::error!(%err, check = %check.name, "health checks: failed to store result");
            }
        }
    }
}

pub fn spawn_health_checks(
    state: AppState,
    poll_interval: Duration,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_health_checks(state, poll_interval))
}
