use monitor_core::store::SqliteStore;
use monitor_core::Store;

use super::common::require_monitor;

pub async fn list() -> anyhow::Result<()> {
    let monitor_cfg = require_monitor().await?;
    let store = SqliteStore::connect(&monitor_cfg.db_path.to_string_lossy()).await?;
    let rules = store.list_alert_rules().await?;

    if rules.is_empty() {
        println!("No alert rules configured yet.");
        return Ok(());
    }

    for rule in &rules {
        let status = if rule.enabled { "enabled" } else { "disabled" };
        println!("{:<24} {:<9} {:?}", rule.name, status, rule.severity);
    }

    let since = chrono::Utc::now() - chrono::Duration::hours(24);
    let recent = store.list_recent_alert_events(since).await?;
    let firing = recent
        .iter()
        .filter(|e| e.state == shared::AlertState::Firing)
        .count();
    println!(
        "\n{firing} alert(s) currently firing (last 24h: {} event(s)).",
        recent.len()
    );
    Ok(())
}
