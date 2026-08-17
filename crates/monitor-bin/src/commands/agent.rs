use monitor_core::store::SqliteStore;
use monitor_core::Store;

use config::Config;

pub async fn status() -> anyhow::Result<()> {
    let Some(config) = Config::load().await? else {
        println!("Not installed. Run `monitor install` to get started.");
        return Ok(());
    };

    if let Some(agent_cfg) = &config.agent {
        println!("Agent registered: server_id={}", agent_cfg.server_id);
        println!("Monitor: {}", agent_cfg.monitor_url);
        println!("Heartbeat interval: {}s", agent_cfg.heartbeat_interval_s);
        return Ok(());
    }

    if let Some(monitor_cfg) = &config.monitor {
        if let Some(self_id) = monitor_cfg.self_server_id {
            let store = SqliteStore::connect(&monitor_cfg.db_path.to_string_lossy()).await?;
            let server = store
                .get_server(self_id)
                .await?
                .ok_or_else(|| anyhow::anyhow!("self server row missing from store"))?;
            println!("Self-monitoring: {:?}", server.status);
            println!("Last heartbeat: {:?}", server.last_heartbeat_at);
            return Ok(());
        }
    }

    println!("This node has no local Agent (Monitor-only install).");
    Ok(())
}
