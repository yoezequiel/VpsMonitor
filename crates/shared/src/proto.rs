//! Wire contracts for the Agent <-> Monitor HTTP API.
//! Kept separate from `model` because these are request/response shapes,
//! not persisted entities — they can evolve independently of the DB schema.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest {
    pub pairing_token: String,
    pub hostname: String,
    pub os: String,
    pub arch: String,
    pub agent_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterResponse {
    pub server_id: Uuid,
    pub agent_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartbeatRequest {
    pub server_id: Uuid,
    pub ts: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsRequest {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AgentEvent {
    DockerContainerStateChanged {
        container_id: String,
        container_name: String,
        from_state: String,
        to_state: String,
    },
    DockerContainerRestarted {
        container_id: String,
        container_name: String,
        restart_count: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventsRequest {
    pub server_id: Uuid,
    pub ts: DateTime<Utc>,
    pub events: Vec<AgentEvent>,
}
