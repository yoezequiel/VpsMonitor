use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use agent_core::collector::Collector;
use agent_core::reporter::Reporter;
use axum_server::tls_rustls::RustlsConfig;
use monitor_core::state::ServerStatusEvent;
use monitor_core::store::SqliteStore;
use monitor_core::{api, scheduler, AppState};
use shared::ServerStatus;
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

use crate::config::{AgentConfig, Config, Mode, MonitorConfig};

pub async fn run(config: Config) -> anyhow::Result<()> {
    match config.mode {
        Mode::Monitor => {
            run_monitor(
                config.monitor.expect("Monitor mode requires [monitor]"),
                None,
            )
            .await
        }
        Mode::MonitorAgent => {
            let monitor_cfg = config
                .monitor
                .expect("Monitor+Agent mode requires [monitor]");
            let self_id = monitor_cfg
                .self_server_id
                .expect("Monitor+Agent mode requires self_server_id");
            run_monitor(monitor_cfg, Some(self_id)).await
        }
        Mode::Agent => run_agent(config.agent.expect("Agent mode requires [agent]")).await,
    }
}

async fn run_monitor(
    monitor_cfg: MonitorConfig,
    self_server_id: Option<Uuid>,
) -> anyhow::Result<()> {
    let store = SqliteStore::connect(&monitor_cfg.db_path.to_string_lossy()).await?;
    let state = AppState::new(Arc::new(store));

    scheduler::spawn_offline_detector(
        state.clone(),
        Duration::from_secs(monitor_cfg.heartbeat_timeout_s),
        Duration::from_secs(10),
    );

    if let Some(server_id) = self_server_id {
        tokio::spawn(self_monitor_loop(state.clone(), server_id));
    }

    let tls_config =
        RustlsConfig::from_pem_file(&monitor_cfg.cert_path, &monitor_cfg.key_path).await?;
    let addr: SocketAddr = monitor_cfg.bind_addr.parse()?;
    tracing::info!(%addr, "Monitor listening (HTTPS)");

    let mut router = api::router(state);
    if let Some(static_dir) = &monitor_cfg.static_dir {
        tracing::info!(dir = %static_dir.display(), "serving dashboard");
        let serve_dir =
            ServeDir::new(static_dir).fallback(ServeFile::new(static_dir.join("index.html")));
        router = router.fallback_service(serve_dir);
    }
    axum_server::bind_rustls(addr, tls_config)
        .serve(router.into_make_service())
        .await?;
    Ok(())
}

/// Drives self-monitoring straight into the store instead of looping the
/// Agent's HTTP client back to this same process (see install.rs).
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

async fn run_agent(agent_cfg: AgentConfig) -> anyhow::Result<()> {
    let reporter = Reporter::new(&agent_cfg.monitor_url).with_agent_token(&agent_cfg.agent_token);
    let mut collector = Collector::new();
    tracing::info!(monitor_url = %agent_cfg.monitor_url, "Agent reporting loop starting");
    agent_core::reporter::run_loop(
        &reporter,
        &mut collector,
        agent_cfg.server_id,
        Duration::from_secs(agent_cfg.heartbeat_interval_s),
    )
    .await;
    Ok(())
}
