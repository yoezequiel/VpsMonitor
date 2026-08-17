use std::io::Write;

use agent_core::collector::host_info;
use chrono::Utc;
use cli::InstallMode;
use monitor_core::store::SqliteStore;
use monitor_core::{auth, tls, Store};
use shared::{Server, ServerStatus, User};
use uuid::Uuid;

use config::{self, AgentConfig, Config, Mode, MonitorConfig};

fn prompt(label: &str, default: Option<&str>) -> anyhow::Result<String> {
    let suffix = default.map(|d| format!(" [{d}]")).unwrap_or_default();
    print!("{label}{suffix}: ");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().read_line(&mut line)?;
    let line = line.trim().to_string();
    Ok(if line.is_empty() {
        default.unwrap_or_default().to_string()
    } else {
        line
    })
}

#[allow(clippy::too_many_arguments)]
pub async fn run(
    mode: Option<InstallMode>,
    bind_addr: String,
    admin_username: Option<String>,
    admin_password: Option<String>,
    monitor_url: Option<String>,
    pairing_token: Option<String>,
    name: Option<String>,
) -> anyhow::Result<()> {
    if config::Config::load().await?.is_some() {
        anyhow::bail!(
            "this node is already installed (config at {}). Remove it first if you want to reinstall.",
            config::config_path()?.display()
        );
    }

    let interactive = mode.is_none();
    let mode = match mode {
        Some(m) => m,
        None => {
            println!(
                "Installation mode\n  1) Monitor + Agent (recommended)\n  2) Monitor\n  3) Agent"
            );
            match prompt("Choose", Some("1"))?.as_str() {
                "2" => InstallMode::Monitor,
                "3" => InstallMode::Agent,
                _ => InstallMode::MonitorAgent,
            }
        }
    };

    let mut monitor_cfg = if matches!(mode, InstallMode::Monitor | InstallMode::MonitorAgent) {
        Some(install_monitor(interactive, &bind_addr, admin_username, admin_password).await?)
    } else {
        None
    };

    let agent_cfg = match mode {
        InstallMode::Agent => {
            Some(install_remote_agent(interactive, monitor_url, pairing_token).await?)
        }
        InstallMode::MonitorAgent => {
            let cfg = monitor_cfg.as_mut().expect("monitor installed above");
            cfg.self_server_id = Some(install_self_agent(cfg, name).await?);
            None
        }
        InstallMode::Monitor => None,
    };

    let config = Config {
        mode: match mode {
            InstallMode::Monitor => Mode::Monitor,
            InstallMode::Agent => Mode::Agent,
            InstallMode::MonitorAgent => Mode::MonitorAgent,
        },
        monitor: monitor_cfg,
        agent: agent_cfg,
    };
    config.save().await?;

    println!(
        "\nInstalled. Config written to {}",
        config::config_path()?.display()
    );
    println!("Start it with: monitor run");
    Ok(())
}

async fn install_monitor(
    interactive: bool,
    bind_addr: &str,
    admin_username: Option<String>,
    admin_password: Option<String>,
) -> anyhow::Result<MonitorConfig> {
    let db_path = config::default_db_path()?;
    if let Some(parent) = db_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let store = SqliteStore::connect(&db_path.to_string_lossy()).await?;

    let username = match admin_username {
        Some(u) => u,
        None if interactive => prompt("Admin username", Some("admin"))?,
        None => "admin".to_string(),
    };
    let (password, generated) = match admin_password {
        Some(p) => (p, false),
        None if interactive => (prompt("Admin password", None)?, false),
        None => (auth::generate_token(), true),
    };

    store
        .create_user(&User {
            id: Uuid::new_v4(),
            username: username.clone(),
            password_hash: auth::hash_password(&password)?,
            created_at: Utc::now(),
        })
        .await?;

    if generated {
        println!("Generated admin credentials (shown once):");
        println!("  username: {username}");
        println!("  password: {password}");
    }

    let (cert_path, key_path) = config::default_tls_paths()?;
    let hostname = host_info().hostname;
    tls::load_or_generate(&cert_path, &key_path, &hostname).await?;

    Ok(MonitorConfig {
        bind_addr: bind_addr.to_string(),
        db_path,
        cert_path,
        key_path,
        heartbeat_timeout_s: 60,
        metrics_raw_retention_hours: 24,
        metrics_rollup_retention_days: 30,
        self_server_id: None,
        static_dir: find_dashboard_build(),
    })
}

/// Looks for a built dashboard next to the current checkout/executable.
/// Development convenience only — packaged releases will embed the
/// dashboard into the binary instead (see PLAN.md section 36/40).
fn find_dashboard_build() -> Option<std::path::PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(dir) = std::env::current_dir() {
        candidates.push(dir.join("web/dist"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            candidates.push(parent.join("web/dist"));
        }
    }
    candidates
        .into_iter()
        .find(|p| p.join("index.html").is_file())
}

/// Self-monitoring (Monitor+Agent) skips the pairing-token/HTTPS dance
/// entirely: it's the same process, so the server row is created directly
/// and `monitor run` drives metrics collection straight into the store
/// instead of looping back over the network. See PLAN.md section 5 risks.
async fn install_self_agent(
    monitor_cfg: &MonitorConfig,
    name: Option<String>,
) -> anyhow::Result<Uuid> {
    let store = SqliteStore::connect(&monitor_cfg.db_path.to_string_lossy()).await?;
    let info = host_info();
    let server = Server {
        id: Uuid::new_v4(),
        name: name.unwrap_or_else(|| info.hostname.clone()),
        hostname: info.hostname,
        ip: None,
        os: info.os,
        arch: info.arch,
        agent_version: env!("CARGO_PKG_VERSION").to_string(),
        tags: vec![],
        status: ServerStatus::Unknown,
        last_heartbeat_at: None,
        created_at: Utc::now(),
    };
    // Self-monitoring never needs its token verified over HTTP, but every
    // server row still needs a unique token_hash — derive one so it can
    // never collide with (or be reused as) a real agent token.
    store
        .create_server(&server, &auth::hash_token(&format!("self:{}", server.id)))
        .await?;
    store.update_heartbeat(server.id, Utc::now()).await?;
    Ok(server.id)
}

async fn install_remote_agent(
    interactive: bool,
    monitor_url: Option<String>,
    pairing_token: Option<String>,
) -> anyhow::Result<AgentConfig> {
    let monitor_url = match monitor_url {
        Some(u) => u,
        None if interactive => prompt("Monitor URL (e.g. https://monitor.local:8443)", None)?,
        None => anyhow::bail!("--monitor-url is required for non-interactive Agent installs"),
    };
    let pairing_token = match pairing_token {
        Some(t) => t,
        None if interactive => prompt("Pairing token", None)?,
        None => anyhow::bail!("--pairing-token is required for non-interactive Agent installs"),
    };

    let info = host_info();
    let mut reporter = agent_core::reporter::Reporter::new(&monitor_url);
    let server_id = reporter
        .register(
            &pairing_token,
            &info.hostname,
            &info.os,
            &info.arch,
            env!("CARGO_PKG_VERSION"),
        )
        .await?;
    let agent_token = reporter
        .agent_token()
        .expect("register() sets the agent token on success")
        .to_string();

    println!(
        "Agent installed successfully.\n\nServer: {}\nStatus: ONLINE",
        info.hostname
    );

    Ok(AgentConfig {
        monitor_url,
        agent_token,
        server_id,
        heartbeat_interval_s: 20,
    })
}
