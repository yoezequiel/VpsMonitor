use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerStatus {
    Online,
    Offline,
    Warning,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Server {
    pub id: Uuid,
    pub name: String,
    pub hostname: String,
    pub ip: Option<String>,
    pub os: String,
    pub arch: String,
    pub agent_version: String,
    pub tags: Vec<String>,
    pub status: ServerStatus,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metric {
    pub id: Uuid,
    pub server_id: Uuid,
    pub ts: DateTime<Utc>,
    pub cpu_pct: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub disk_used: u64,
    pub disk_total: u64,
    pub load1: f32,
    pub load5: f32,
    pub load15: f32,
    pub net_rx: u64,
    pub net_tx: u64,
    pub uptime_s: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContainerState {
    Running,
    Stopped,
    Restarting,
    Paused,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerContainer {
    pub server_id: Uuid,
    /// Docker's own container id, not a row id — containers are upserted
    /// by (server_id, container_id) rather than accumulating history.
    pub container_id: String,
    pub name: String,
    pub image: String,
    pub state: ContainerState,
    pub cpu_pct: f32,
    pub mem_used: u64,
    pub mem_limit: u64,
    pub net_rx: u64,
    pub net_tx: u64,
    pub restart_count: u32,
    pub started_at: Option<DateTime<Utc>>,
    pub updated_at: DateTime<Utc>,
}

/// Flat permission model by design: any authenticated user has full
/// access. No role tiers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthCheckType {
    Http,
    Https,
    Tcp,
    Ping,
    Dns,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub id: Uuid,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: HealthCheckType,
    pub target: String,
    pub interval_s: u32,
    pub timeout_s: u32,
    pub expected_status: Option<u16>,
    pub server_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheckResult {
    pub id: Uuid,
    pub health_check_id: Uuid,
    pub ts: DateTime<Utc>,
    pub status_code: Option<u16>,
    pub latency_ms: u32,
    pub ok: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Typed shape of `AlertRule::condition`. Stored as JSON (not a typed
/// column) so new condition kinds don't need a migration, but every
/// producer/consumer goes through this enum rather than hand-rolling JSON
/// — see `AlertRule::parsed_condition` / `AlertRule::condition_json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AlertCondition {
    /// Fires once CPU has stayed above `threshold_pct` continuously for
    /// `for_secs`, scoped to `server_id` (or every server, if `None`).
    CpuAbove {
        server_id: Option<Uuid>,
        threshold_pct: f32,
        for_secs: u64,
    },
    MemAbove {
        server_id: Option<Uuid>,
        threshold_pct: f32,
        for_secs: u64,
    },
    DiskAbove {
        server_id: Option<Uuid>,
        threshold_pct: f32,
        for_secs: u64,
    },
    ServerOffline {
        server_id: Option<Uuid>,
        for_secs: u64,
    },
    ContainerStopped {
        server_id: Option<Uuid>,
    },
    HealthCheckFailed {
        health_check_id: Uuid,
        for_secs: u64,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: Uuid,
    pub name: String,
    pub condition: serde_json::Value,
    pub severity: AlertSeverity,
    pub enabled: bool,
}

impl AlertRule {
    pub fn parsed_condition(&self) -> Result<AlertCondition, serde_json::Error> {
        serde_json::from_value(self.condition.clone())
    }

    pub fn from_condition(
        id: Uuid,
        name: String,
        condition: &AlertCondition,
        severity: AlertSeverity,
        enabled: bool,
    ) -> Self {
        Self {
            id,
            name,
            condition: serde_json::to_value(condition).expect("AlertCondition always serializes"),
            severity,
            enabled,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AlertState {
    Firing,
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertEvent {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub server_id: Option<Uuid>,
    pub state: AlertState,
    /// When the underlying condition first became true — not necessarily
    /// when it started "firing"; a `for_secs` duration must still elapse
    /// (see `notified_at`) before a notification actually goes out.
    pub started_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    /// Set the first time a notification was sent for this event, so the
    /// evaluator doesn't re-notify on every poll once a rule's `for_secs`
    /// has elapsed.
    pub notified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IncidentStatus {
    Open,
    Resolved,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Incident {
    pub id: Uuid,
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub status: IncidentStatus,
    /// The server this incident is about, when it's about one specific
    /// server — also the correlation key new alert events are matched
    /// against (see `Store::find_open_incident_for_server`).
    pub server_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentEvent {
    pub id: Uuid,
    pub incident_id: Uuid,
    pub ts: DateTime<Utc>,
    pub description: String,
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationChannelKind {
    Webhook,
    Email,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookConfig {
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmailConfig {
    pub smtp_host: String,
    pub smtp_port: u16,
    pub username: String,
    pub password: String,
    pub from: String,
    pub to: String,
}

/// A destination for alert notifications. `config`'s shape depends on
/// `kind` (`WebhookConfig` / `EmailConfig`) — kept as JSON, like
/// `AlertRule::condition`, so new channel kinds don't need a migration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannel {
    pub id: Uuid,
    pub kind: NotificationChannelKind,
    pub config: serde_json::Value,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub user_id: Uuid,
    pub action: String,
    pub target: String,
    pub ts: DateTime<Utc>,
    pub metadata: serde_json::Value,
}
