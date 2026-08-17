use std::path::PathBuf;

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Monitor,
    Agent,
    MonitorAgent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorConfig {
    pub bind_addr: String,
    pub db_path: PathBuf,
    pub cert_path: PathBuf,
    pub key_path: PathBuf,
    pub heartbeat_timeout_s: u64,
    /// Set only in Monitor+Agent mode: the id of the server row this same
    /// node registered for itself. `monitor run` uses it to drive
    /// self-monitoring straight into the store (see PLAN.md section 5).
    pub self_server_id: Option<Uuid>,
    /// Directory holding the built dashboard (`web/dist`), served at `/`
    /// alongside the API. `None` when no build was found at install time —
    /// the Monitor still runs API-only.
    pub static_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentConfig {
    pub monitor_url: String,
    pub agent_token: String,
    pub server_id: Uuid,
    pub heartbeat_interval_s: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub mode: Mode,
    pub monitor: Option<MonitorConfig>,
    pub agent: Option<AgentConfig>,
}

fn project_dirs() -> anyhow::Result<ProjectDirs> {
    ProjectDirs::from("dev", "vpsmonitor", "vpsmonitor")
        .ok_or_else(|| anyhow::anyhow!("could not determine a home directory for this user"))
}

pub fn config_path() -> anyhow::Result<PathBuf> {
    Ok(project_dirs()?.config_dir().join("config.toml"))
}

pub fn default_db_path() -> anyhow::Result<PathBuf> {
    Ok(project_dirs()?.data_dir().join("monitor.db"))
}

pub fn default_tls_paths() -> anyhow::Result<(PathBuf, PathBuf)> {
    let dir = project_dirs()?.data_dir().join("tls");
    Ok((dir.join("cert.pem"), dir.join("key.pem")))
}

impl Config {
    pub async fn load() -> anyhow::Result<Option<Self>> {
        let path = config_path()?;
        if !path.exists() {
            return Ok(None);
        }
        let raw = tokio::fs::read_to_string(&path).await?;
        Ok(Some(toml::from_str(&raw)?))
    }

    pub async fn save(&self) -> anyhow::Result<()> {
        let path = config_path()?;
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(&path, toml::to_string_pretty(self)?).await?;
        Ok(())
    }
}
