//! Boots the Monitor's HTTPS API (+ optional embedded dashboard and
//! self-monitoring) from a set of plain parameters — no dependency on how
//! those parameters got persisted to disk. `monitor-bin` fills this in
//! from its on-disk `config` crate; the desktop app fills it in the same
//! way, so both start the exact same server instead of maintaining two
//! copies of this wiring.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_core::collector::Collector;
use axum_server::tls_rustls::RustlsConfig;
use shared::ServerStatus;
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

use crate::state::ServerStatusEvent;
use crate::store::SqliteStore;
use crate::{api, scheduler, AppState};

/// Network-facing installs (`monitor run`, systemd) always use TLS — see
/// `monitor_core::tls`. The desktop app's embedded copy binds loopback
/// only and uses `Plain`: no self-hosted CA exists to make a self-signed
/// cert acceptable to an embedded webview (WebKitGTK/WebView2/WKWebView
/// all reject it with no click-through), and since the connection never
/// leaves 127.0.0.1, TLS buys nothing there anyway.
pub enum Transport {
    Tls {
        cert_path: PathBuf,
        key_path: PathBuf,
    },
    Plain,
}

pub struct ServerParams {
    pub bind_addr: SocketAddr,
    pub db_path: PathBuf,
    pub transport: Transport,
    pub static_dir: Option<PathBuf>,
    pub heartbeat_timeout: Duration,
    pub raw_metrics_retention: Duration,
    pub rollup_metrics_retention: Duration,
    /// Set when this node is also monitoring itself (Monitor+Agent): the
    /// id of the server row it registered for itself.
    pub self_server_id: Option<Uuid>,
}

/// Runs until the process is killed — starts the offline detector, the
/// metrics-rollup job, the health check poller, optional self-monitoring,
/// and finally serves the API (plus the dashboard, if `static_dir` is set).
pub async fn run(params: ServerParams) -> anyhow::Result<()> {
    let store = SqliteStore::connect(&params.db_path.to_string_lossy()).await?;
    let raw_retention = chrono::Duration::from_std(params.raw_metrics_retention)
        .unwrap_or(chrono::Duration::hours(24));
    let state = AppState::new(Arc::new(store)).with_raw_metrics_retention(raw_retention);

    scheduler::spawn_offline_detector(
        state.clone(),
        params.heartbeat_timeout,
        Duration::from_secs(10),
    );
    scheduler::spawn_health_checks(state.clone(), Duration::from_secs(10));
    scheduler::spawn_metrics_rollup(
        state.clone(),
        params.raw_metrics_retention,
        params.rollup_metrics_retention,
        Duration::from_secs(15 * 60),
    );

    if let Some(server_id) = params.self_server_id {
        tokio::spawn(self_monitor_loop(state.clone(), server_id));
    }

    let mut router = api::router(state);
    if let Some(static_dir) = &params.static_dir {
        tracing::info!(dir = %static_dir.display(), "serving dashboard");
        let serve_dir =
            ServeDir::new(static_dir).fallback(ServeFile::new(static_dir.join("index.html")));
        router = router.fallback_service(serve_dir);
    }

    match params.transport {
        Transport::Tls {
            cert_path,
            key_path,
        } => {
            let tls_config = RustlsConfig::from_pem_file(&cert_path, &key_path).await?;
            tracing::info!(addr = %params.bind_addr, "Monitor listening (HTTPS)");
            axum_server::bind_rustls(params.bind_addr, tls_config)
                .serve(router.into_make_service())
                .await?;
        }
        Transport::Plain => {
            tracing::info!(addr = %params.bind_addr, "Monitor listening (HTTP, loopback-only)");
            let listener = tokio::net::TcpListener::bind(params.bind_addr).await?;
            axum::serve(listener, router.into_make_service()).await?;
        }
    }
    Ok(())
}

/// Drives self-monitoring straight into the store instead of looping an
/// HTTP client back to this same process (see PLAN.md section 5 risks).
async fn self_monitor_loop(state: AppState, server_id: Uuid) {
    let mut collector = Collector::new();
    let mut ticker = tokio::time::interval(Duration::from_secs(15));
    loop {
        ticker.tick().await;
        let metric = collector.snapshot(server_id);

        if let Err(err) = state.store.update_heartbeat(server_id, metric.ts).await {
            tracing::warn!(%err, "self-monitor: heartbeat update failed");
            continue;
        }
        let _ = state.status_tx.send(ServerStatusEvent {
            server_id,
            status: ServerStatus::Online,
        });
        if let Err(err) = state.store.insert_metric(&metric).await {
            tracing::warn!(%err, "self-monitor: metric insert failed");
        }
    }
}
