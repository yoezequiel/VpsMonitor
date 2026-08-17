use monitor_core::store::SqliteStore;
use monitor_core::Store;
use shared::IncidentStatus;

use super::common::require_monitor;

pub async fn list() -> anyhow::Result<()> {
    let monitor_cfg = require_monitor().await?;
    let store = SqliteStore::connect(&monitor_cfg.db_path.to_string_lossy()).await?;
    let incidents = store.list_incidents().await?;

    if incidents.is_empty() {
        println!("No incidents recorded.");
        return Ok(());
    }

    for incident in incidents {
        let status = if incident.status == IncidentStatus::Open {
            "OPEN"
        } else {
            "resolved"
        };
        let duration = match incident.resolved_at {
            Some(resolved) => format!("{}m", (resolved - incident.started_at).num_minutes()),
            None => "ongoing".to_string(),
        };
        println!(
            "#{}  {:<8} {:<30} started={} duration={}",
            &incident.id.to_string()[..8],
            status,
            incident.title,
            incident.started_at.to_rfc3339(),
            duration,
        );
    }
    Ok(())
}
