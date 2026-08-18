use std::time::Duration;

use agent_core::collector::Collector;
use agent_core::reporter::Reporter;
use monitor_core::server::{ServerParams, Transport};

use config::{AgentConfig, Config, Mode, MonitorConfig};

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
    self_server_id: Option<uuid::Uuid>,
) -> anyhow::Result<()> {
    let bind_addr = monitor_cfg.bind_addr.parse()?;
    let port = bind_addr_port(&monitor_cfg.bind_addr);
    let host = monitor_core::tls::primary_local_ip().unwrap_or_else(|| "localhost".to_string());
    tracing::info!(
        dashboard = format!("https://{host}:{port}"),
        "reachable from your network"
    );

    monitor_core::server::run(ServerParams {
        bind_addr,
        db_path: monitor_cfg.db_path,
        transport: Transport::Tls {
            cert_path: monitor_cfg.cert_path,
            key_path: monitor_cfg.key_path,
        },
        static_dir: monitor_cfg.static_dir,
        heartbeat_timeout: Duration::from_secs(monitor_cfg.heartbeat_timeout_s),
        raw_metrics_retention: Duration::from_secs(monitor_cfg.metrics_raw_retention_hours * 3600),
        rollup_metrics_retention: Duration::from_secs(
            monitor_cfg.metrics_rollup_retention_days * 86400,
        ),
        logs_retention: Duration::from_secs(monitor_cfg.logs_retention_hours * 3600),
        self_server_id,
    })
    .await
}

fn bind_addr_port(bind_addr: &str) -> &str {
    bind_addr.rsplit(':').next().unwrap_or("8443")
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
