use monitor_core::store::{LogQuery, SqliteStore};
use monitor_core::Store;
use shared::{LogLevel, LogSource};

use super::common::require_monitor;

pub async fn run(server: Option<String>, level: Option<String>) -> anyhow::Result<()> {
    let monitor_cfg = require_monitor().await?;
    let store = SqliteStore::connect(&monitor_cfg.db_path.to_string_lossy()).await?;

    let servers = store.list_servers().await?;
    let server_id = match &server {
        Some(name) => Some(
            servers
                .iter()
                .find(|s| &s.name == name)
                .ok_or_else(|| anyhow::anyhow!("no server named '{name}'"))?
                .id,
        ),
        None => None,
    };
    let min_level = level.as_deref().map(parse_level).transpose()?;

    let entries = store
        .list_logs(&LogQuery {
            server_id,
            min_level,
            since: Some(chrono::Utc::now() - chrono::Duration::hours(24)),
            limit: 200,
            ..Default::default()
        })
        .await?;

    if entries.is_empty() {
        println!("No log entries in the last 24h.");
        return Ok(());
    }

    for entry in entries.iter().rev() {
        let server_name = servers
            .iter()
            .find(|s| s.id == entry.server_id)
            .map(|s| s.name.as_str())
            .unwrap_or("unknown");
        let container = entry
            .container_name
            .as_deref()
            .map(|c| format!("/{c}"))
            .unwrap_or_default();
        println!(
            "{} {:<5} {:<16} {}{}: {}",
            entry.ts.to_rfc3339(),
            level_str(entry.level),
            server_name,
            source_str(entry.source),
            container,
            entry.message,
        );
    }
    Ok(())
}

fn parse_level(s: &str) -> anyhow::Result<LogLevel> {
    match s.to_lowercase().as_str() {
        "trace" => Ok(LogLevel::Trace),
        "debug" => Ok(LogLevel::Debug),
        "info" => Ok(LogLevel::Info),
        "warn" | "warning" => Ok(LogLevel::Warn),
        "error" => Ok(LogLevel::Error),
        other => Err(anyhow::anyhow!(
            "unknown log level '{other}' (expected trace/debug/info/warn/error)"
        )),
    }
}

fn level_str(l: LogLevel) -> &'static str {
    match l {
        LogLevel::Trace => "TRACE",
        LogLevel::Debug => "DEBUG",
        LogLevel::Info => "INFO",
        LogLevel::Warn => "WARN",
        LogLevel::Error => "ERROR",
    }
}

fn source_str(s: LogSource) -> &'static str {
    match s {
        LogSource::Agent => "agent",
        LogSource::Monitor => "monitor",
        LogSource::Docker => "docker",
    }
}
