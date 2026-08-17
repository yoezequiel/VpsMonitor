use config::{Config, MonitorConfig};

pub async fn require_monitor() -> anyhow::Result<MonitorConfig> {
    let config = Config::load()
        .await?
        .ok_or_else(|| anyhow::anyhow!("not installed. Run `monitor install` first."))?;
    config
        .monitor
        .ok_or_else(|| anyhow::anyhow!("this node is not a Monitor (Agent-only install)"))
}
