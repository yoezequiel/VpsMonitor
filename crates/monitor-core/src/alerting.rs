//! Alert rule evaluation and notification dispatch (Proyecto.md sections
//! 19-20). Rules are evaluated on a schedule (see
//! `scheduler::run_alert_evaluation`); each condition maps to a small,
//! fixed check against current state — no general expression language,
//! by design, so evaluation stays predictable and testable.

use chrono::{Duration as ChronoDuration, Utc};
use lettre::message::Mailbox;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor};
use shared::{
    AlertCondition, AlertEvent, AlertRule, AlertState, ContainerState, EmailConfig,
    NotificationChannel, NotificationChannelKind, Server, ServerStatus, WebhookConfig,
};
use uuid::Uuid;

use crate::incidents;
use crate::state::AppState;

pub async fn evaluate_rules(state: &AppState) {
    let rules = match state.store.list_enabled_alert_rules().await {
        Ok(rules) => rules,
        Err(err) => {
            tracing::error!(%err, "alerting: failed to list rules");
            return;
        }
    };

    for rule in rules {
        let condition = match rule.parsed_condition() {
            Ok(c) => c,
            Err(err) => {
                tracing::error!(%err, rule = %rule.name, "alerting: rule has an unparseable condition, skipping");
                continue;
            }
        };
        evaluate_condition(state, &rule, &condition).await;
    }
}

async fn target_servers(state: &AppState, server_id: Option<Uuid>) -> Vec<Server> {
    match server_id {
        Some(id) => state
            .store
            .get_server(id)
            .await
            .ok()
            .flatten()
            .into_iter()
            .collect(),
        None => state.store.list_servers().await.unwrap_or_default(),
    }
}

async fn latest_cpu_pct(state: &AppState, server_id: Uuid) -> Option<f32> {
    let until = Utc::now();
    let since = until - ChronoDuration::minutes(2);
    state
        .store
        .list_metrics(server_id, since, until)
        .await
        .ok()?
        .last()
        .map(|m| m.cpu_pct)
}

async fn latest_mem_pct(state: &AppState, server_id: Uuid) -> Option<f32> {
    let until = Utc::now();
    let since = until - ChronoDuration::minutes(2);
    state
        .store
        .list_metrics(server_id, since, until)
        .await
        .ok()?
        .last()
        .map(|m| (m.mem_used as f32 / m.mem_total.max(1) as f32) * 100.0)
}

async fn latest_disk_pct(state: &AppState, server_id: Uuid) -> Option<f32> {
    let until = Utc::now();
    let since = until - ChronoDuration::minutes(2);
    state
        .store
        .list_metrics(server_id, since, until)
        .await
        .ok()?
        .last()
        .map(|m| (m.disk_used as f32 / m.disk_total.max(1) as f32) * 100.0)
}

async fn evaluate_condition(state: &AppState, rule: &AlertRule, condition: &AlertCondition) {
    match condition {
        AlertCondition::CpuAbove {
            server_id,
            threshold_pct,
            for_secs,
        } => {
            for server in target_servers(state, *server_id).await {
                let is_true = latest_cpu_pct(state, server.id)
                    .await
                    .is_some_and(|v| v > *threshold_pct);
                apply(
                    state,
                    rule,
                    Some(server.id),
                    is_true,
                    *for_secs,
                    format!("CPU on {} exceeded {threshold_pct}%", server.name),
                    format!("CPU on {} recovered below {threshold_pct}%", server.name),
                )
                .await;
            }
        }
        AlertCondition::MemAbove {
            server_id,
            threshold_pct,
            for_secs,
        } => {
            for server in target_servers(state, *server_id).await {
                let is_true = latest_mem_pct(state, server.id)
                    .await
                    .is_some_and(|v| v > *threshold_pct);
                apply(
                    state,
                    rule,
                    Some(server.id),
                    is_true,
                    *for_secs,
                    format!("Memory on {} exceeded {threshold_pct}%", server.name),
                    format!("Memory on {} recovered below {threshold_pct}%", server.name),
                )
                .await;
            }
        }
        AlertCondition::DiskAbove {
            server_id,
            threshold_pct,
            for_secs,
        } => {
            for server in target_servers(state, *server_id).await {
                let is_true = latest_disk_pct(state, server.id)
                    .await
                    .is_some_and(|v| v > *threshold_pct);
                apply(
                    state,
                    rule,
                    Some(server.id),
                    is_true,
                    *for_secs,
                    format!("Disk on {} exceeded {threshold_pct}%", server.name),
                    format!("Disk on {} recovered below {threshold_pct}%", server.name),
                )
                .await;
            }
        }
        AlertCondition::ServerOffline {
            server_id,
            for_secs,
        } => {
            for server in target_servers(state, *server_id).await {
                let is_true = server.status == ServerStatus::Offline;
                apply(
                    state,
                    rule,
                    Some(server.id),
                    is_true,
                    *for_secs,
                    format!("{} went offline", server.name),
                    format!("{} is back online", server.name),
                )
                .await;
            }
        }
        AlertCondition::ContainerStopped { server_id } => {
            for server in target_servers(state, *server_id).await {
                let containers = state
                    .store
                    .list_docker_containers(server.id)
                    .await
                    .unwrap_or_default();
                let is_true = containers
                    .iter()
                    .any(|c| c.state == ContainerState::Stopped);
                apply(
                    state,
                    rule,
                    Some(server.id),
                    is_true,
                    0,
                    format!("A container stopped on {}", server.name),
                    format!("Containers on {} are running again", server.name),
                )
                .await;
            }
        }
        AlertCondition::HealthCheckFailed {
            health_check_id,
            for_secs,
        } => {
            let until = Utc::now();
            let since = until - ChronoDuration::minutes(5);
            let is_true = state
                .store
                .list_health_check_results(*health_check_id, since, until)
                .await
                .ok()
                .and_then(|r| r.last().map(|r| !r.ok))
                .unwrap_or(false);
            apply(
                state,
                rule,
                None,
                is_true,
                *for_secs,
                "Health check failing".to_string(),
                "Health check recovered".to_string(),
            )
            .await;
        }
    }
}

/// The pending → firing → resolved state machine: a condition first
/// becoming true opens an event but doesn't notify yet; once it has held
/// true for `for_secs`, it notifies exactly once (tracked by
/// `notified_at`); the condition clearing resolves the event, notifying
/// a recovery only if the firing notification actually went out.
async fn apply(
    state: &AppState,
    rule: &AlertRule,
    server_id: Option<Uuid>,
    is_true: bool,
    for_secs: u64,
    firing_description: String,
    resolved_description: String,
) {
    let existing = state
        .store
        .get_open_alert_event(rule.id, server_id)
        .await
        .ok()
        .flatten();

    if is_true {
        let event = match existing {
            Some(event) => event,
            None => {
                let event = AlertEvent {
                    id: Uuid::new_v4(),
                    rule_id: rule.id,
                    server_id,
                    state: AlertState::Firing,
                    started_at: Utc::now(),
                    resolved_at: None,
                    notified_at: None,
                };
                if let Err(err) = state.store.create_alert_event(&event).await {
                    tracing::error!(%err, rule = %rule.name, "alerting: failed to create alert event");
                    return;
                }
                event
            }
        };

        if event.notified_at.is_none()
            && Utc::now() - event.started_at >= ChronoDuration::seconds(for_secs as i64)
        {
            if let Err(err) = state
                .store
                .mark_alert_event_notified(event.id, Utc::now())
                .await
            {
                tracing::error!(%err, rule = %rule.name, "alerting: failed to mark event notified");
            }
            tracing::warn!(rule = %rule.name, "{firing_description}");
            notify(state, rule, &firing_description).await;
            if let Err(err) =
                incidents::record_alert_firing(state.store.as_ref(), &event, firing_description)
                    .await
            {
                tracing::error!(%err, "alerting: failed to record incident");
            }
        }
    } else if let Some(event) = existing {
        let was_notified = event.notified_at.is_some();
        if let Err(err) = state.store.resolve_alert_event(event.id, Utc::now()).await {
            tracing::error!(%err, rule = %rule.name, "alerting: failed to resolve alert event");
            return;
        }
        if was_notified {
            tracing::info!(rule = %rule.name, "{resolved_description}");
            notify(state, rule, &resolved_description).await;
            if let Err(err) = incidents::record_alert_resolution(
                state.store.as_ref(),
                &event,
                resolved_description,
            )
            .await
            {
                tracing::error!(%err, "alerting: failed to record incident resolution");
            }
        }
    }
}

async fn notify(state: &AppState, rule: &AlertRule, description: &str) {
    let channels = match state.store.list_enabled_notification_channels().await {
        Ok(c) => c,
        Err(err) => {
            tracing::error!(%err, "alerting: failed to list notification channels");
            return;
        }
    };
    for channel in channels {
        if let Err(err) = send_notification(&channel, rule, description).await {
            tracing::error!(%err, channel = ?channel.kind, "alerting: notification failed");
        }
    }
}

async fn send_notification(
    channel: &NotificationChannel,
    rule: &AlertRule,
    description: &str,
) -> anyhow::Result<()> {
    match channel.kind {
        NotificationChannelKind::Webhook => {
            let config: WebhookConfig = serde_json::from_value(channel.config.clone())?;
            reqwest::Client::new()
                .post(&config.url)
                .json(&serde_json::json!({
                    "rule": rule.name,
                    "severity": rule.severity,
                    "description": description,
                    "ts": Utc::now(),
                }))
                .send()
                .await?
                .error_for_status()?;
            Ok(())
        }
        NotificationChannelKind::Email => {
            let config: EmailConfig = serde_json::from_value(channel.config.clone())?;
            send_email(&config, rule, description).await
        }
    }
}

async fn send_email(
    config: &EmailConfig,
    rule: &AlertRule,
    description: &str,
) -> anyhow::Result<()> {
    let email = Message::builder()
        .from(config.from.parse::<Mailbox>()?)
        .to(config.to.parse::<Mailbox>()?)
        .subject(format!("[VpsMonitor] {}", rule.name))
        .body(description.to_string())?;

    let creds = Credentials::new(config.username.clone(), config.password.clone());
    let transport = AsyncSmtpTransport::<Tokio1Executor>::relay(&config.smtp_host)?
        .port(config.smtp_port)
        .credentials(creds)
        .build();

    transport.send(email).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use shared::{
        AlertCondition, AlertSeverity, AlertState, ContainerState, DockerContainer, IncidentStatus,
    };
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;
    use crate::store::{SqliteStore, Store};

    fn sample_server() -> Server {
        Server {
            id: Uuid::new_v4(),
            name: "vps-01".into(),
            hostname: "vps-01.local".into(),
            ip: None,
            os: "Ubuntu 24.04".into(),
            arch: "x86_64".into(),
            agent_version: "0.1.0".into(),
            tags: vec![],
            status: ServerStatus::Online,
            last_heartbeat_at: Some(Utc::now()),
            created_at: Utc::now(),
        }
    }

    /// The V2 acceptance bar (PLAN.md "Criterio de salida"): a container
    /// stopping fires a real webhook and the alert shows up grouped into
    /// an incident with a timeline.
    #[tokio::test]
    async fn container_stopped_fires_webhook_and_opens_incident() {
        let webhook_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/hook"))
            .respond_with(ResponseTemplate::new(200))
            .expect(2) // one for firing, one for the recovery notification
            .mount(&webhook_server)
            .await;

        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store
            .create_server(&server, "hash-alert-e2e")
            .await
            .unwrap();
        store
            .upsert_docker_containers(
                server.id,
                &[DockerContainer {
                    server_id: server.id,
                    container_id: "abc".into(),
                    name: "my-api".into(),
                    image: "my-api:latest".into(),
                    state: ContainerState::Stopped,
                    cpu_pct: 0.0,
                    mem_used: 0,
                    mem_limit: 0,
                    net_rx: 0,
                    net_tx: 0,
                    restart_count: 0,
                    started_at: None,
                    updated_at: Utc::now(),
                }],
            )
            .await
            .unwrap();

        store
            .create_notification_channel(&NotificationChannel {
                id: Uuid::new_v4(),
                kind: NotificationChannelKind::Webhook,
                config: serde_json::json!({ "url": format!("{}/hook", webhook_server.uri()) }),
                enabled: true,
            })
            .await
            .unwrap();

        let rule = AlertRule::from_condition(
            Uuid::new_v4(),
            "container stopped".to_string(),
            &AlertCondition::ContainerStopped { server_id: None },
            AlertSeverity::Critical,
            true,
        );
        store.create_alert_rule(&rule).await.unwrap();

        let state = AppState::new(Arc::new(store));
        evaluate_rules(&state).await;

        let events = state
            .store
            .list_recent_alert_events(Utc::now() - ChronoDuration::minutes(1))
            .await
            .unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].state, AlertState::Firing);
        assert!(
            events[0].notified_at.is_some(),
            "for_secs=0 must notify on first evaluation"
        );

        let incidents = state.store.list_incidents().await.unwrap();
        assert_eq!(incidents.len(), 1);
        assert_eq!(incidents[0].status, IncidentStatus::Open);
        assert_eq!(incidents[0].server_id, Some(server.id));

        let timeline = state
            .store
            .list_incident_events(incidents[0].id)
            .await
            .unwrap();
        assert_eq!(timeline.len(), 1);
        assert!(timeline[0].description.contains("stopped"));

        // The container coming back up resolves both the alert and the
        // incident, with a recovery entry on the timeline.
        store_container_running(&state, server.id, "abc").await;
        evaluate_rules(&state).await;

        let events = state
            .store
            .list_recent_alert_events(Utc::now() - ChronoDuration::minutes(1))
            .await
            .unwrap();
        assert_eq!(events[0].state, AlertState::Resolved);

        let incident = state
            .store
            .get_incident(incidents[0].id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(incident.status, IncidentStatus::Resolved);

        let timeline = state
            .store
            .list_incident_events(incidents[0].id)
            .await
            .unwrap();
        assert_eq!(timeline.len(), 3); // stopped, recovered, incident resolved

        webhook_server.verify().await;
    }

    async fn store_container_running(state: &AppState, server_id: Uuid, container_id: &str) {
        state
            .store
            .upsert_docker_containers(
                server_id,
                &[DockerContainer {
                    server_id,
                    container_id: container_id.to_string(),
                    name: "my-api".into(),
                    image: "my-api:latest".into(),
                    state: ContainerState::Running,
                    cpu_pct: 1.0,
                    mem_used: 10,
                    mem_limit: 100,
                    net_rx: 0,
                    net_tx: 0,
                    restart_count: 1,
                    started_at: Some(Utc::now()),
                    updated_at: Utc::now(),
                }],
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn cpu_above_only_notifies_after_for_secs_elapsed() {
        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store.create_server(&server, "hash-cpu").await.unwrap();
        store
            .insert_metric(&shared::Metric {
                id: Uuid::new_v4(),
                server_id: server.id,
                ts: Utc::now(),
                cpu_pct: 95.0,
                mem_used: 1,
                mem_total: 2,
                disk_used: 1,
                disk_total: 2,
                load1: 1.0,
                load5: 1.0,
                load15: 1.0,
                net_rx: 0,
                net_tx: 0,
                uptime_s: 60,
            })
            .await
            .unwrap();

        let rule = AlertRule::from_condition(
            Uuid::new_v4(),
            "cpu high".to_string(),
            &AlertCondition::CpuAbove {
                server_id: None,
                threshold_pct: 90.0,
                for_secs: 300,
            },
            AlertSeverity::Warning,
            true,
        );
        store.create_alert_rule(&rule).await.unwrap();

        let state = AppState::new(Arc::new(store));
        evaluate_rules(&state).await;

        let event = state
            .store
            .get_open_alert_event(rule.id, Some(server.id))
            .await
            .unwrap()
            .expect("condition is true, an event should open");
        assert!(
            event.notified_at.is_none(),
            "for_secs hasn't elapsed yet — must not notify"
        );

        // No incident either, since nothing has notified.
        assert!(state.store.list_incidents().await.unwrap().is_empty());
    }
}
