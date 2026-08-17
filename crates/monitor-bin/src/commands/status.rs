use monitor_core::store::SqliteStore;
use monitor_core::Store;
use shared::ServerStatus;

use crate::config::Config;

pub async fn run() -> anyhow::Result<()> {
    let Some(config) = Config::load().await? else {
        println!("Not installed. Run `monitor install` to get started.");
        return Ok(());
    };

    println!("Mode: {:?}", config.mode);

    if let Some(monitor_cfg) = &config.monitor {
        let store = SqliteStore::connect(&monitor_cfg.db_path.to_string_lossy()).await?;
        let servers = store.list_servers().await?;
        let online = servers
            .iter()
            .filter(|s| s.status == ServerStatus::Online)
            .count();
        println!(
            "Monitor: {} server(s), {} online — listening on {}",
            servers.len(),
            online,
            monitor_cfg.bind_addr
        );
        println!("  db: {}", monitor_cfg.db_path.display());
    }

    if let Some(agent_cfg) = &config.agent {
        println!(
            "Agent: server_id={} monitor={}",
            agent_cfg.server_id, agent_cfg.monitor_url
        );
    }

    Ok(())
}
