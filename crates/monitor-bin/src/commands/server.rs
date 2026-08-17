use monitor_core::store::SqliteStore;
use monitor_core::{api, tls, Store};

use super::common::require_monitor;

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

    let port = monitor_cfg.bind_addr.rsplit(':').next().unwrap_or("8443");
    let host = tls::primary_local_ip().unwrap_or_else(|| "<this-host-ip>".to_string());

    println!("Pairing token generated for '{name}' (valid 15 minutes):\n");
    println!("  {token}\n");
    println!("On the target machine, run:");
    println!("  monitor install --mode agent --monitor-url https://{host}:{port} --pairing-token {token}");
    Ok(())
}
