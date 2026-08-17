use monitor_core::store::SqliteStore;
use monitor_core::{api, Store};

use crate::config::{Config, MonitorConfig};

async fn require_monitor() -> anyhow::Result<MonitorConfig> {
    let config = Config::load()
        .await?
        .ok_or_else(|| anyhow::anyhow!("not installed. Run `monitor install` first."))?;
    config
        .monitor
        .ok_or_else(|| anyhow::anyhow!("this node is not a Monitor (Agent-only install)"))
}

pub async fn list() -> anyhow::Result<()> {
    let monitor_cfg = require_monitor().await?;
    let store = SqliteStore::connect(&monitor_cfg.db_path.to_string_lossy()).await?;
    let servers = store.list_servers().await?;

    if servers.is_empty() {
        println!("No servers registered yet. Add one with `monitor server add --name <name>`.");
        return Ok(());
    }

    for server in servers {
        let heartbeat = server
            .last_heartbeat_at
            .map(|t| t.to_rfc3339())
            .unwrap_or_else(|| "never".to_string());
        println!(
            "{:<20} {:<10?} {:<25} last_heartbeat={}",
            server.name, server.status, server.hostname, heartbeat
        );
    }
    Ok(())
}

pub async fn add(name: String) -> anyhow::Result<()> {
    let monitor_cfg = require_monitor().await?;
    let store = SqliteStore::connect(&monitor_cfg.db_path.to_string_lossy()).await?;
    let token = api::create_pairing_token(&store, &name).await?;

    println!("Pairing token generated for '{name}' (valid 15 minutes):\n");
    println!("  {token}\n");
    println!("On the target machine, run:");
    println!(
        "  monitor install --mode agent --monitor-url https://<this-host>:{} --pairing-token {token}",
        monitor_cfg
            .bind_addr
            .rsplit(':')
            .next()
            .unwrap_or("8443")
    );
    Ok(())
}
