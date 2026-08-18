//! Native wrapper around the Monitor: no sidecar process, no second
//! implementation of the server — this embeds `monitor-core`/`agent-core`
//! directly (both are plain Rust crates, and Tauri's backend is Rust too,
//! so there's nothing to shell out to) and points the window at the same
//! dashboard the browser would see. See PLAN.md section 2.1/5.
//!
//! Scope note: this wraps an *already-installed* node. Running the
//! interactive `monitor install` wizard from inside a native window is a
//! real GUI-design task on its own and is deliberately out of scope here
//! — if no config exists yet, this shows instructions to run
//! `monitor install` from a terminal instead of pretending to install.

use std::time::Duration;

use config::{Config, Mode};
use tauri::{Manager, WebviewWindow};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let window = app
                .get_webview_window("main")
                .expect("tauri.conf.json declares a window labeled \"main\"");
            tauri::async_runtime::spawn(start(window));
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running the VpsMonitor desktop shell");
}

async fn start(window: WebviewWindow) {
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("no CryptoProvider installed yet");

    let config = match Config::load().await {
        Ok(Some(config)) => config,
        Ok(None) => {
            show_message(
                &window,
                "Not installed yet",
                "Run `monitor install` in a terminal first, then reopen this app.",
            );
            return;
        }
        Err(err) => {
            show_message(&window, "Failed to read configuration", &err.to_string());
            return;
        }
    };

    match config.mode {
        Mode::Monitor | Mode::MonitorAgent => start_monitor(window, config).await,
        Mode::Agent => start_agent_only(window, config).await,
    }
}

async fn start_monitor(window: WebviewWindow, config: Config) {
    let monitor_cfg = config
        .monitor
        .expect("Monitor/Monitor+Agent config always has a [monitor] section");
    let self_server_id = monitor_cfg.self_server_id;

    // Deliberately not `monitor_cfg.bind_addr` (typically 0.0.0.0, for the
    // network-facing `monitor run` case): the embedded copy is a local
    // view of this install, not a second network-exposed instance, so it
    // always binds loopback-only regardless of what the persisted config
    // says — see `monitor_core::server::Transport::Plain`.
    let port = match monitor_cfg
        .bind_addr
        .rsplit(':')
        .next()
        .and_then(|p| p.parse::<u16>().ok())
    {
        Some(port) => port,
        None => {
            show_message(
                &window,
                "Invalid bind address in config",
                &monitor_cfg.bind_addr,
            );
            return;
        }
    };
    let bind_addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));

    let params = monitor_core::server::ServerParams {
        bind_addr,
        db_path: monitor_cfg.db_path,
        transport: monitor_core::server::Transport::Plain,
        static_dir: monitor_cfg.static_dir,
        heartbeat_timeout: Duration::from_secs(monitor_cfg.heartbeat_timeout_s),
        raw_metrics_retention: Duration::from_secs(monitor_cfg.metrics_raw_retention_hours * 3600),
        rollup_metrics_retention: Duration::from_secs(
            monitor_cfg.metrics_rollup_retention_days * 86400,
        ),
        logs_retention: Duration::from_secs(monitor_cfg.logs_retention_hours * 3600),
        self_server_id,
    };

    tauri::async_runtime::spawn(async move {
        if let Err(err) = monitor_core::server::run(params).await {
            tracing::error!(%err, "embedded Monitor server exited");
        }
    });

    if !wait_for_port("127.0.0.1", port, Duration::from_secs(10)).await {
        show_message(
            &window,
            "Monitor did not start",
            "Timed out waiting for the embedded server to come up (is another `monitor run` already using this port?). Check the app logs.",
        );
        return;
    }

    let url = format!("http://127.0.0.1:{port}/");
    if let Err(err) = window.navigate(url.parse().expect("well-formed loopback URL")) {
        show_message(&window, "Failed to load dashboard", &err.to_string());
    }
}

/// Agent-only installs have no local dashboard to show — this just keeps
/// the reporting loop alive in the background and shows a small status
/// page instead of an empty window.
async fn start_agent_only(window: WebviewWindow, config: Config) {
    let agent_cfg = config
        .agent
        .expect("Agent config always has an [agent] section");
    let monitor_url = agent_cfg.monitor_url.clone();

    tauri::async_runtime::spawn(async move {
        let reporter = agent_core::reporter::Reporter::new(&agent_cfg.monitor_url)
            .with_agent_token(&agent_cfg.agent_token);
        let mut collector = agent_core::collector::Collector::new();
        agent_core::reporter::run_loop(
            &reporter,
            &mut collector,
            agent_cfg.server_id,
            Duration::from_secs(agent_cfg.heartbeat_interval_s),
        )
        .await;
    });

    show_message(
        &window,
        "Agent running",
        &format!("Reporting to {monitor_url}. Open the Monitor's dashboard in a browser to view metrics."),
    );
}

async fn wait_for_port(host: &str, port: u16, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    while tokio::time::Instant::now() < deadline {
        if tokio::net::TcpStream::connect((host, port)).await.is_ok() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    false
}

fn show_message(window: &WebviewWindow, title: &str, body: &str) {
    let escaped_title = title.replace('\\', "\\\\").replace('`', "\\`");
    let escaped_body = body.replace('\\', "\\\\").replace('`', "\\`");
    let _ = window.eval(format!(
        "document.body.innerHTML = `<div style=\"font-family:system-ui,sans-serif;padding:40px;color:#fff\"><h2>{escaped_title}</h2><p>{escaped_body}</p></div>`;"
    ));
}
