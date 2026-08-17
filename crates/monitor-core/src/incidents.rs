//! Groups related alert events into incidents with a timeline
//! (Proyecto.md sections 21-22). A new firing alert folds into any
//! still-open incident for the same server started within
//! `CORRELATION_WINDOW`; otherwise it opens a new incident. An incident
//! auto-resolves once none of that server's alerts are still firing.

use chrono::{Duration as ChronoDuration, Utc};
use shared::{AlertEvent, AlertState, Incident, IncidentEvent, IncidentStatus};
use uuid::Uuid;

use crate::store::Store;

pub const CORRELATION_WINDOW_SECS: i64 = 600;

/// Called when an alert event starts actually notifying (see
/// `alerting::apply`) — folds it into an open incident for the same
/// server, opening one if none exists.
pub async fn record_alert_firing(
    store: &dyn Store,
    event: &AlertEvent,
    description: String,
) -> anyhow::Result<()> {
    let Some(server_id) = event.server_id else {
        return Ok(());
    };

    let since = Utc::now() - ChronoDuration::seconds(CORRELATION_WINDOW_SECS);
    let incident = match store
        .find_open_incident_for_server(server_id, since)
        .await?
    {
        Some(incident) => incident,
        None => {
            let server_name = store
                .get_server(server_id)
                .await?
                .map(|s| s.name)
                .unwrap_or_else(|| server_id.to_string());
            let incident = Incident {
                id: Uuid::new_v4(),
                title: format!("{server_name} issues"),
                started_at: Utc::now(),
                resolved_at: None,
                status: IncidentStatus::Open,
                server_id: Some(server_id),
            };
            store.create_incident(&incident).await?;
            incident
        }
    };

    store
        .add_incident_event(&IncidentEvent {
            id: Uuid::new_v4(),
            incident_id: incident.id,
            ts: Utc::now(),
            description,
            source_ref: Some(event.rule_id.to_string()),
        })
        .await?;
    Ok(())
}

/// Called when a previously-notified alert resolves — logs the recovery
/// on the open incident's timeline, then closes the incident if no other
/// alert on that server is still firing.
pub async fn record_alert_resolution(
    store: &dyn Store,
    event: &AlertEvent,
    description: String,
) -> anyhow::Result<()> {
    let Some(server_id) = event.server_id else {
        return Ok(());
    };

    let since = Utc::now() - ChronoDuration::seconds(CORRELATION_WINDOW_SECS);
    let Some(incident) = store
        .find_open_incident_for_server(server_id, since)
        .await?
    else {
        return Ok(());
    };

    store
        .add_incident_event(&IncidentEvent {
            id: Uuid::new_v4(),
            incident_id: incident.id,
            ts: Utc::now(),
            description,
            source_ref: Some(event.rule_id.to_string()),
        })
        .await?;

    let still_firing = store
        .list_recent_alert_events(since)
        .await?
        .iter()
        .any(|e| {
            e.server_id == Some(server_id) && e.state == AlertState::Firing && e.id != event.id
        });

    if !still_firing {
        store.resolve_incident(incident.id, Utc::now()).await?;
        store
            .add_incident_event(&IncidentEvent {
                id: Uuid::new_v4(),
                incident_id: incident.id,
                ts: Utc::now(),
                description: "All alerts recovered — incident resolved".to_string(),
                source_ref: None,
            })
            .await?;
    }
    Ok(())
}
