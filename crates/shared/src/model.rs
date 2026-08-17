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
pub enum Role {
    Administrator,
    Operator,
    Viewer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub role: Role,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertRule {
    pub id: Uuid,
    pub name: String,
    pub condition: serde_json::Value,
    pub severity: AlertSeverity,
    pub enabled: bool,
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
    pub started_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncidentEvent {
    pub id: Uuid,
    pub incident_id: Uuid,
    pub ts: DateTime<Utc>,
    pub description: String,
    pub source_ref: Option<String>,
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
