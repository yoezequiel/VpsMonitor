use std::time::Duration;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use shared::{
    AlertEvent, AlertRule, AlertSeverity, AlertState, ContainerState, DockerContainer, HealthCheck,
    HealthCheckResult, HealthCheckType, Incident, IncidentEvent, IncidentStatus, LogEntry,
    LogLevel, LogSource, NotificationChannel, NotificationChannelKind, Server, ServerStatus, User,
};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{QueryBuilder, Row, Sqlite, SqlitePool};
use uuid::Uuid;

use super::{LogQuery, Store, StoreError, StoreResult};

#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// Connects to (and creates if missing) a SQLite database file, then
    /// applies pending migrations.
    ///
    /// WAL mode + a busy timeout matter here specifically because a single
    /// Monitor+Agent process already has several tasks writing to the same
    /// file concurrently (heartbeat/metrics ingestion, the offline
    /// detector, the metrics-rollup job, self-monitoring) — SQLite's
    /// default rollback-journal mode serializes writers hard enough that
    /// "database is locked" shows up under completely ordinary load, not
    /// just under contention from a second process.
    pub async fn connect(path: &str) -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(Duration::from_secs(10));
        let pool = SqlitePoolOptions::new().connect_with(options).await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    #[cfg(test)]
    pub(crate) async fn in_memory() -> anyhow::Result<Self> {
        let pool = SqlitePoolOptions::new().connect(":memory:").await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }
}

fn parse_status(s: &str) -> ServerStatus {
    match s {
        "online" => ServerStatus::Online,
        "offline" => ServerStatus::Offline,
        "warning" => ServerStatus::Warning,
        _ => ServerStatus::Unknown,
    }
}

fn status_str(s: ServerStatus) -> &'static str {
    match s {
        ServerStatus::Online => "online",
        ServerStatus::Offline => "offline",
        ServerStatus::Warning => "warning",
        ServerStatus::Unknown => "unknown",
    }
}

fn parse_health_check_type(s: &str) -> HealthCheckType {
    match s {
        "https" => HealthCheckType::Https,
        "tcp" => HealthCheckType::Tcp,
        "ping" => HealthCheckType::Ping,
        "dns" => HealthCheckType::Dns,
        _ => HealthCheckType::Http,
    }
}

fn health_check_type_str(t: HealthCheckType) -> &'static str {
    match t {
        HealthCheckType::Http => "http",
        HealthCheckType::Https => "https",
        HealthCheckType::Tcp => "tcp",
        HealthCheckType::Ping => "ping",
        HealthCheckType::Dns => "dns",
    }
}

fn row_to_health_check(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<HealthCheck> {
    let server_id: Option<String> = row.try_get("server_id")?;
    let expected_status: Option<i64> = row.try_get("expected_status")?;
    Ok(HealthCheck {
        id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
        name: row.try_get("name")?,
        kind: parse_health_check_type(row.try_get::<String, _>("kind")?.as_str()),
        target: row.try_get("target")?,
        interval_s: row.try_get::<i64, _>("interval_s")? as u32,
        timeout_s: row.try_get::<i64, _>("timeout_s")? as u32,
        expected_status: expected_status.map(|s| s as u16),
        server_id: server_id.map(|s| Uuid::parse_str(&s)).transpose()?,
    })
}

fn row_to_health_check_result(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<HealthCheckResult> {
    let status_code: Option<i64> = row.try_get("status_code")?;
    Ok(HealthCheckResult {
        id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
        health_check_id: Uuid::parse_str(row.try_get::<String, _>("health_check_id")?.as_str())?,
        ts: DateTime::parse_from_rfc3339(row.try_get::<String, _>("ts")?.as_str())?
            .with_timezone(&Utc),
        status_code: status_code.map(|s| s as u16),
        latency_ms: row.try_get::<i64, _>("latency_ms")? as u32,
        ok: row.try_get("ok")?,
    })
}

fn parse_container_state(s: &str) -> ContainerState {
    match s {
        "running" => ContainerState::Running,
        "stopped" => ContainerState::Stopped,
        "restarting" => ContainerState::Restarting,
        "paused" => ContainerState::Paused,
        _ => ContainerState::Unknown,
    }
}

fn container_state_str(s: ContainerState) -> &'static str {
    match s {
        ContainerState::Running => "running",
        ContainerState::Stopped => "stopped",
        ContainerState::Restarting => "restarting",
        ContainerState::Paused => "paused",
        ContainerState::Unknown => "unknown",
    }
}

fn row_to_docker_container(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<DockerContainer> {
    let started_at: Option<String> = row.try_get("started_at")?;
    Ok(DockerContainer {
        server_id: Uuid::parse_str(row.try_get::<String, _>("server_id")?.as_str())?,
        container_id: row.try_get("container_id")?,
        name: row.try_get("name")?,
        image: row.try_get("image")?,
        state: parse_container_state(row.try_get::<String, _>("state")?.as_str()),
        cpu_pct: row.try_get("cpu_pct")?,
        mem_used: row.try_get::<i64, _>("mem_used")? as u64,
        mem_limit: row.try_get::<i64, _>("mem_limit")? as u64,
        net_rx: row.try_get::<i64, _>("net_rx")? as u64,
        net_tx: row.try_get::<i64, _>("net_tx")? as u64,
        restart_count: row.try_get::<i64, _>("restart_count")? as u32,
        started_at: started_at
            .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()?,
        updated_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("updated_at")?.as_str())?
            .with_timezone(&Utc),
    })
}

fn parse_log_level(s: &str) -> LogLevel {
    match s {
        "trace" => LogLevel::Trace,
        "debug" => LogLevel::Debug,
        "warn" => LogLevel::Warn,
        "error" => LogLevel::Error,
        _ => LogLevel::Info,
    }
}

fn log_level_str(l: LogLevel) -> &'static str {
    match l {
        LogLevel::Trace => "trace",
        LogLevel::Debug => "debug",
        LogLevel::Info => "info",
        LogLevel::Warn => "warn",
        LogLevel::Error => "error",
    }
}

fn parse_log_source(s: &str) -> LogSource {
    match s {
        "monitor" => LogSource::Monitor,
        "docker" => LogSource::Docker,
        _ => LogSource::Agent,
    }
}

fn log_source_str(s: LogSource) -> &'static str {
    match s {
        LogSource::Agent => "agent",
        LogSource::Monitor => "monitor",
        LogSource::Docker => "docker",
    }
}

fn row_to_log_entry(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<LogEntry> {
    Ok(LogEntry {
        id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
        server_id: Uuid::parse_str(row.try_get::<String, _>("server_id")?.as_str())?,
        source: parse_log_source(row.try_get::<String, _>("source")?.as_str()),
        container_name: row.try_get("container_name")?,
        level: parse_log_level(row.try_get::<String, _>("level")?.as_str()),
        message: row.try_get("message")?,
        ts: DateTime::parse_from_rfc3339(row.try_get::<String, _>("ts")?.as_str())?
            .with_timezone(&Utc),
    })
}

fn parse_alert_severity(s: &str) -> AlertSeverity {
    match s {
        "warning" => AlertSeverity::Warning,
        "critical" => AlertSeverity::Critical,
        _ => AlertSeverity::Info,
    }
}

fn alert_severity_str(s: AlertSeverity) -> &'static str {
    match s {
        AlertSeverity::Info => "info",
        AlertSeverity::Warning => "warning",
        AlertSeverity::Critical => "critical",
    }
}

fn row_to_alert_rule(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<AlertRule> {
    let condition_json: String = row.try_get("condition")?;
    Ok(AlertRule {
        id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
        name: row.try_get("name")?,
        condition: serde_json::from_str(&condition_json)?,
        severity: parse_alert_severity(row.try_get::<String, _>("severity")?.as_str()),
        enabled: row.try_get("enabled")?,
    })
}

fn row_to_alert_event(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<AlertEvent> {
    let server_id: Option<String> = row.try_get("server_id")?;
    let resolved_at: Option<String> = row.try_get("resolved_at")?;
    let notified_at: Option<String> = row.try_get("notified_at")?;
    Ok(AlertEvent {
        id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
        rule_id: Uuid::parse_str(row.try_get::<String, _>("rule_id")?.as_str())?,
        server_id: server_id.map(|s| Uuid::parse_str(&s)).transpose()?,
        state: if row.try_get::<String, _>("state")?.as_str() == "resolved" {
            AlertState::Resolved
        } else {
            AlertState::Firing
        },
        started_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("started_at")?.as_str())?
            .with_timezone(&Utc),
        resolved_at: resolved_at
            .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()?,
        notified_at: notified_at
            .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()?,
    })
}

fn notification_channel_kind_str(k: NotificationChannelKind) -> &'static str {
    match k {
        NotificationChannelKind::Webhook => "webhook",
        NotificationChannelKind::Email => "email",
    }
}

fn row_to_notification_channel(
    row: &sqlx::sqlite::SqliteRow,
) -> anyhow::Result<NotificationChannel> {
    let config_json: String = row.try_get("config")?;
    Ok(NotificationChannel {
        id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
        kind: match row.try_get::<String, _>("kind")?.as_str() {
            "email" => NotificationChannelKind::Email,
            _ => NotificationChannelKind::Webhook,
        },
        config: serde_json::from_str(&config_json)?,
        enabled: row.try_get("enabled")?,
    })
}

fn row_to_incident(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<Incident> {
    let resolved_at: Option<String> = row.try_get("resolved_at")?;
    let server_id: Option<String> = row.try_get("server_id")?;
    Ok(Incident {
        id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
        title: row.try_get("title")?,
        started_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("started_at")?.as_str())?
            .with_timezone(&Utc),
        resolved_at: resolved_at
            .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()?,
        status: if row.try_get::<String, _>("status")?.as_str() == "resolved" {
            IncidentStatus::Resolved
        } else {
            IncidentStatus::Open
        },
        server_id: server_id.map(|s| Uuid::parse_str(&s)).transpose()?,
    })
}

fn row_to_incident_event(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<IncidentEvent> {
    Ok(IncidentEvent {
        id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
        incident_id: Uuid::parse_str(row.try_get::<String, _>("incident_id")?.as_str())?,
        ts: DateTime::parse_from_rfc3339(row.try_get::<String, _>("ts")?.as_str())?
            .with_timezone(&Utc),
        description: row.try_get("description")?,
        source_ref: row.try_get("source_ref")?,
    })
}

fn row_to_server(row: &sqlx::sqlite::SqliteRow) -> anyhow::Result<Server> {
    let tags_json: String = row.try_get("tags")?;
    let last_heartbeat_at: Option<String> = row.try_get("last_heartbeat_at")?;
    Ok(Server {
        id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
        name: row.try_get("name")?,
        hostname: row.try_get("hostname")?,
        ip: row.try_get("ip")?,
        os: row.try_get("os")?,
        arch: row.try_get("arch")?,
        agent_version: row.try_get("agent_version")?,
        tags: serde_json::from_str(&tags_json)?,
        status: parse_status(row.try_get::<String, _>("status")?.as_str()),
        last_heartbeat_at: last_heartbeat_at
            .map(|s| DateTime::parse_from_rfc3339(&s).map(|d| d.with_timezone(&Utc)))
            .transpose()?,
        created_at: DateTime::parse_from_rfc3339(row.try_get::<String, _>("created_at")?.as_str())?
            .with_timezone(&Utc),
    })
}

#[async_trait]
impl Store for SqliteStore {
    async fn create_server(&self, server: &Server, token_hash: &str) -> StoreResult<()> {
        let tags_json =
            serde_json::to_string(&server.tags).map_err(|e| StoreError::Backend(e.into()))?;
        sqlx::query(
            "INSERT INTO servers (id, name, hostname, ip, os, arch, agent_version, tags, status, last_heartbeat_at, token_hash, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(server.id.to_string())
        .bind(&server.name)
        .bind(&server.hostname)
        .bind(&server.ip)
        .bind(&server.os)
        .bind(&server.arch)
        .bind(&server.agent_version)
        .bind(tags_json)
        .bind(status_str(server.status))
        .bind(server.last_heartbeat_at.map(|d| d.to_rfc3339()))
        .bind(token_hash)
        .bind(server.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn get_server(&self, id: Uuid) -> StoreResult<Option<Server>> {
        let row = sqlx::query("SELECT * FROM servers WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        row.map(|r| row_to_server(&r).map_err(StoreError::Backend))
            .transpose()
    }

    async fn list_servers(&self) -> StoreResult<Vec<Server>> {
        let rows = sqlx::query("SELECT * FROM servers ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        rows.iter()
            .map(|r| row_to_server(r).map_err(StoreError::Backend))
            .collect()
    }

    async fn update_heartbeat(&self, id: Uuid, ts: DateTime<Utc>) -> StoreResult<()> {
        let result =
            sqlx::query("UPDATE servers SET last_heartbeat_at = ?, status = 'online' WHERE id = ?")
                .bind(ts.to_rfc3339())
                .bind(id.to_string())
                .execute(&self.pool)
                .await
                .map_err(|e| StoreError::Backend(e.into()))?;
        if result.rows_affected() == 0 {
            return Err(StoreError::NotFound);
        }
        Ok(())
    }

    async fn delete_server(&self, id: Uuid) -> StoreResult<()> {
        sqlx::query("DELETE FROM servers WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn mark_servers_offline(&self, threshold: DateTime<Utc>) -> StoreResult<Vec<Uuid>> {
        let rows = sqlx::query(
            "SELECT id FROM servers
             WHERE status != 'offline'
               AND (last_heartbeat_at IS NULL OR last_heartbeat_at < ?)",
        )
        .bind(threshold.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;

        let ids = rows
            .iter()
            .map(|r| {
                Uuid::parse_str(r.try_get::<String, _>("id")?.as_str()).map_err(anyhow::Error::from)
            })
            .collect::<anyhow::Result<Vec<_>>>()
            .map_err(StoreError::Backend)?;

        if !ids.is_empty() {
            sqlx::query(
                "UPDATE servers SET status = 'offline'
                 WHERE status != 'offline'
                   AND (last_heartbeat_at IS NULL OR last_heartbeat_at < ?)",
            )
            .bind(threshold.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        }

        Ok(ids)
    }

    async fn insert_metric(&self, metric: &shared::Metric) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO metrics (id, server_id, ts, cpu_pct, mem_used, mem_total, disk_used, disk_total, load1, load5, load15, net_rx, net_tx, uptime_s)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(metric.id.to_string())
        .bind(metric.server_id.to_string())
        .bind(metric.ts.to_rfc3339())
        .bind(metric.cpu_pct)
        .bind(metric.mem_used as i64)
        .bind(metric.mem_total as i64)
        .bind(metric.disk_used as i64)
        .bind(metric.disk_total as i64)
        .bind(metric.load1)
        .bind(metric.load5)
        .bind(metric.load15)
        .bind(metric.net_rx as i64)
        .bind(metric.net_tx as i64)
        .bind(metric.uptime_s as i64)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn list_metrics(
        &self,
        server_id: Uuid,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> StoreResult<Vec<shared::Metric>> {
        let rows = sqlx::query(
            "SELECT * FROM metrics WHERE server_id = ? AND ts BETWEEN ? AND ? ORDER BY ts",
        )
        .bind(server_id.to_string())
        .bind(since.to_rfc3339())
        .bind(until.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;

        rows.iter()
            .map(|row| {
                Ok(shared::Metric {
                    id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
                    server_id: Uuid::parse_str(row.try_get::<String, _>("server_id")?.as_str())?,
                    ts: DateTime::parse_from_rfc3339(row.try_get::<String, _>("ts")?.as_str())?
                        .with_timezone(&Utc),
                    cpu_pct: row.try_get("cpu_pct")?,
                    mem_used: row.try_get::<i64, _>("mem_used")? as u64,
                    mem_total: row.try_get::<i64, _>("mem_total")? as u64,
                    disk_used: row.try_get::<i64, _>("disk_used")? as u64,
                    disk_total: row.try_get::<i64, _>("disk_total")? as u64,
                    load1: row.try_get("load1")?,
                    load5: row.try_get("load5")?,
                    load15: row.try_get("load15")?,
                    net_rx: row.try_get::<i64, _>("net_rx")? as u64,
                    net_tx: row.try_get::<i64, _>("net_tx")? as u64,
                    uptime_s: row.try_get::<i64, _>("uptime_s")? as u64,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()
            .map_err(StoreError::Backend)
    }

    async fn list_metric_rollups(
        &self,
        server_id: Uuid,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> StoreResult<Vec<shared::Metric>> {
        let rows = sqlx::query(
            "SELECT * FROM metric_rollups WHERE server_id = ? AND bucket_start BETWEEN ? AND ? ORDER BY bucket_start",
        )
        .bind(server_id.to_string())
        .bind(since.to_rfc3339())
        .bind(until.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;

        rows.iter()
            .map(|row| {
                Ok(shared::Metric {
                    id: Uuid::parse_str(row.try_get::<String, _>("id")?.as_str())?,
                    server_id: Uuid::parse_str(row.try_get::<String, _>("server_id")?.as_str())?,
                    ts: DateTime::parse_from_rfc3339(
                        row.try_get::<String, _>("bucket_start")?.as_str(),
                    )?
                    .with_timezone(&Utc),
                    cpu_pct: row.try_get("avg_cpu_pct")?,
                    mem_used: row.try_get::<i64, _>("avg_mem_used")? as u64,
                    mem_total: row.try_get::<i64, _>("avg_mem_total")? as u64,
                    disk_used: row.try_get::<i64, _>("avg_disk_used")? as u64,
                    disk_total: row.try_get::<i64, _>("avg_disk_total")? as u64,
                    load1: row.try_get("avg_load1")?,
                    load5: row.try_get("avg_load5")?,
                    load15: row.try_get("avg_load15")?,
                    net_rx: row.try_get::<i64, _>("avg_net_rx")? as u64,
                    net_tx: row.try_get::<i64, _>("avg_net_tx")? as u64,
                    uptime_s: row.try_get::<i64, _>("avg_uptime_s")? as u64,
                })
            })
            .collect::<anyhow::Result<Vec<_>>>()
            .map_err(StoreError::Backend)
    }

    async fn rollup_and_prune(
        &self,
        raw_cutoff: DateTime<Utc>,
        rollup_cutoff: DateTime<Utc>,
    ) -> StoreResult<(u64, u64, u64)> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;

        let buckets = sqlx::query(
            "SELECT server_id, strftime('%Y-%m-%dT%H:00:00Z', ts) AS bucket,
                    AVG(cpu_pct) AS avg_cpu_pct,
                    AVG(mem_used) AS avg_mem_used, AVG(mem_total) AS avg_mem_total,
                    AVG(disk_used) AS avg_disk_used, AVG(disk_total) AS avg_disk_total,
                    AVG(load1) AS avg_load1, AVG(load5) AS avg_load5, AVG(load15) AS avg_load15,
                    AVG(net_rx) AS avg_net_rx, AVG(net_tx) AS avg_net_tx,
                    AVG(uptime_s) AS avg_uptime_s, COUNT(*) AS sample_count
             FROM metrics
             WHERE ts < ?
             GROUP BY server_id, bucket",
        )
        .bind(raw_cutoff.to_rfc3339())
        .fetch_all(&mut *tx)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;

        let rolled_up = buckets.len() as u64;
        for row in &buckets {
            sqlx::query(
                "INSERT INTO metric_rollups
                    (id, server_id, bucket_start, avg_cpu_pct, avg_mem_used, avg_mem_total,
                     avg_disk_used, avg_disk_total, avg_load1, avg_load5, avg_load15,
                     avg_net_rx, avg_net_tx, avg_uptime_s, sample_count)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(server_id, bucket_start) DO UPDATE SET
                    avg_cpu_pct = excluded.avg_cpu_pct,
                    avg_mem_used = excluded.avg_mem_used,
                    avg_mem_total = excluded.avg_mem_total,
                    avg_disk_used = excluded.avg_disk_used,
                    avg_disk_total = excluded.avg_disk_total,
                    avg_load1 = excluded.avg_load1,
                    avg_load5 = excluded.avg_load5,
                    avg_load15 = excluded.avg_load15,
                    avg_net_rx = excluded.avg_net_rx,
                    avg_net_tx = excluded.avg_net_tx,
                    avg_uptime_s = excluded.avg_uptime_s,
                    sample_count = excluded.sample_count",
            )
            .bind(Uuid::new_v4().to_string())
            .bind(
                row.try_get::<String, _>("server_id")
                    .map_err(|e| StoreError::Backend(e.into()))?,
            )
            .bind(
                row.try_get::<String, _>("bucket")
                    .map_err(|e| StoreError::Backend(e.into()))?,
            )
            .bind(
                row.try_get::<f64, _>("avg_cpu_pct")
                    .map_err(|e| StoreError::Backend(e.into()))?,
            )
            .bind(
                row.try_get::<f64, _>("avg_mem_used")
                    .map_err(|e| StoreError::Backend(e.into()))? as i64,
            )
            .bind(
                row.try_get::<f64, _>("avg_mem_total")
                    .map_err(|e| StoreError::Backend(e.into()))? as i64,
            )
            .bind(
                row.try_get::<f64, _>("avg_disk_used")
                    .map_err(|e| StoreError::Backend(e.into()))? as i64,
            )
            .bind(
                row.try_get::<f64, _>("avg_disk_total")
                    .map_err(|e| StoreError::Backend(e.into()))? as i64,
            )
            .bind(
                row.try_get::<f64, _>("avg_load1")
                    .map_err(|e| StoreError::Backend(e.into()))?,
            )
            .bind(
                row.try_get::<f64, _>("avg_load5")
                    .map_err(|e| StoreError::Backend(e.into()))?,
            )
            .bind(
                row.try_get::<f64, _>("avg_load15")
                    .map_err(|e| StoreError::Backend(e.into()))?,
            )
            .bind(
                row.try_get::<f64, _>("avg_net_rx")
                    .map_err(|e| StoreError::Backend(e.into()))? as i64,
            )
            .bind(
                row.try_get::<f64, _>("avg_net_tx")
                    .map_err(|e| StoreError::Backend(e.into()))? as i64,
            )
            .bind(
                row.try_get::<f64, _>("avg_uptime_s")
                    .map_err(|e| StoreError::Backend(e.into()))? as i64,
            )
            .bind(
                row.try_get::<i64, _>("sample_count")
                    .map_err(|e| StoreError::Backend(e.into()))?,
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        }

        let deleted = sqlx::query("DELETE FROM metrics WHERE ts < ?")
            .bind(raw_cutoff.to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?
            .rows_affected();

        let pruned = sqlx::query("DELETE FROM metric_rollups WHERE bucket_start < ?")
            .bind(rollup_cutoff.to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?
            .rows_affected();

        tx.commit()
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;

        Ok((rolled_up, deleted, pruned))
    }

    async fn create_user(&self, user: &User) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, created_at) VALUES (?, ?, ?, ?)",
        )
        .bind(user.id.to_string())
        .bind(&user.username)
        .bind(&user.password_hash)
        .bind(user.created_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn get_user_by_username(&self, username: &str) -> StoreResult<Option<User>> {
        let row = sqlx::query("SELECT * FROM users WHERE username = ?")
            .bind(username)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        let Some(row) = row else { return Ok(None) };
        Ok(Some(User {
            id: Uuid::parse_str(
                row.try_get::<String, _>("id")
                    .map_err(|e| StoreError::Backend(e.into()))?
                    .as_str(),
            )
            .map_err(|e| StoreError::Backend(e.into()))?,
            username: row
                .try_get("username")
                .map_err(|e| StoreError::Backend(e.into()))?,
            password_hash: row
                .try_get("password_hash")
                .map_err(|e| StoreError::Backend(e.into()))?,
            created_at: DateTime::parse_from_rfc3339(
                row.try_get::<String, _>("created_at")
                    .map_err(|e| StoreError::Backend(e.into()))?
                    .as_str(),
            )
            .map_err(|e| StoreError::Backend(e.into()))?
            .with_timezone(&Utc),
        }))
    }

    async fn create_pairing_token(
        &self,
        token_hash: &str,
        server_name: &str,
        expires_at: DateTime<Utc>,
    ) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO pairing_tokens (token_hash, server_name, created_at, expires_at) VALUES (?, ?, ?, ?)",
        )
        .bind(token_hash)
        .bind(server_name)
        .bind(Utc::now().to_rfc3339())
        .bind(expires_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn consume_pairing_token(&self, token_hash: &str) -> StoreResult<Option<String>> {
        let row = sqlx::query(
            "SELECT server_name, expires_at, used_at FROM pairing_tokens WHERE token_hash = ?",
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;

        let Some(row) = row else { return Ok(None) };
        let used_at: Option<String> = row
            .try_get("used_at")
            .map_err(|e| StoreError::Backend(e.into()))?;
        if used_at.is_some() {
            return Ok(None);
        }
        let expires_at: String = row
            .try_get("expires_at")
            .map_err(|e| StoreError::Backend(e.into()))?;
        let expires_at = DateTime::parse_from_rfc3339(&expires_at)
            .map_err(|e| StoreError::Backend(e.into()))?
            .with_timezone(&Utc);
        if expires_at < Utc::now() {
            return Ok(None);
        }

        sqlx::query("UPDATE pairing_tokens SET used_at = ? WHERE token_hash = ?")
            .bind(Utc::now().to_rfc3339())
            .bind(token_hash)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;

        row.try_get("server_name")
            .map(Some)
            .map_err(|e| StoreError::Backend(e.into()))
    }

    async fn get_server_by_token_hash(&self, token_hash: &str) -> StoreResult<Option<Server>> {
        let row = sqlx::query("SELECT * FROM servers WHERE token_hash = ?")
            .bind(token_hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        row.map(|r| row_to_server(&r).map_err(StoreError::Backend))
            .transpose()
    }

    async fn create_session(
        &self,
        id: Uuid,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO sessions (id, user_id, token_hash, expires_at) VALUES (?, ?, ?, ?)",
        )
        .bind(id.to_string())
        .bind(user_id.to_string())
        .bind(token_hash)
        .bind(expires_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn get_user_by_session_token_hash(&self, token_hash: &str) -> StoreResult<Option<User>> {
        let row = sqlx::query(
            "SELECT users.* FROM sessions
             JOIN users ON users.id = sessions.user_id
             WHERE sessions.token_hash = ? AND sessions.expires_at > ?",
        )
        .bind(token_hash)
        .bind(Utc::now().to_rfc3339())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;

        let Some(row) = row else { return Ok(None) };
        Ok(Some(User {
            id: Uuid::parse_str(
                row.try_get::<String, _>("id")
                    .map_err(|e| StoreError::Backend(e.into()))?
                    .as_str(),
            )
            .map_err(|e| StoreError::Backend(e.into()))?,
            username: row
                .try_get("username")
                .map_err(|e| StoreError::Backend(e.into()))?,
            password_hash: row
                .try_get("password_hash")
                .map_err(|e| StoreError::Backend(e.into()))?,
            created_at: DateTime::parse_from_rfc3339(
                row.try_get::<String, _>("created_at")
                    .map_err(|e| StoreError::Backend(e.into()))?
                    .as_str(),
            )
            .map_err(|e| StoreError::Backend(e.into()))?
            .with_timezone(&Utc),
        }))
    }

    async fn delete_session(&self, token_hash: &str) -> StoreResult<()> {
        sqlx::query("DELETE FROM sessions WHERE token_hash = ?")
            .bind(token_hash)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn create_health_check(&self, check: &shared::HealthCheck) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO health_checks (id, name, kind, target, interval_s, timeout_s, expected_status, server_id, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(check.id.to_string())
        .bind(&check.name)
        .bind(health_check_type_str(check.kind))
        .bind(&check.target)
        .bind(check.interval_s as i64)
        .bind(check.timeout_s as i64)
        .bind(check.expected_status.map(|s| s as i64))
        .bind(check.server_id.map(|id| id.to_string()))
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn list_health_checks(&self) -> StoreResult<Vec<shared::HealthCheck>> {
        let rows = sqlx::query("SELECT * FROM health_checks ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        rows.iter()
            .map(|row| row_to_health_check(row).map_err(StoreError::Backend))
            .collect()
    }

    async fn delete_health_check(&self, id: Uuid) -> StoreResult<()> {
        sqlx::query("DELETE FROM health_checks WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn insert_health_check_result(
        &self,
        result: &shared::HealthCheckResult,
    ) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO health_check_results (id, health_check_id, ts, status_code, latency_ms, ok)
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(result.id.to_string())
        .bind(result.health_check_id.to_string())
        .bind(result.ts.to_rfc3339())
        .bind(result.status_code.map(|s| s as i64))
        .bind(result.latency_ms as i64)
        .bind(result.ok)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn list_health_check_results(
        &self,
        health_check_id: Uuid,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> StoreResult<Vec<shared::HealthCheckResult>> {
        let rows = sqlx::query(
            "SELECT * FROM health_check_results WHERE health_check_id = ? AND ts BETWEEN ? AND ? ORDER BY ts",
        )
        .bind(health_check_id.to_string())
        .bind(since.to_rfc3339())
        .bind(until.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;

        rows.iter()
            .map(|row| row_to_health_check_result(row).map_err(StoreError::Backend))
            .collect()
    }

    async fn last_health_check_result_at(
        &self,
        health_check_id: Uuid,
    ) -> StoreResult<Option<DateTime<Utc>>> {
        let row = sqlx::query(
            "SELECT ts FROM health_check_results WHERE health_check_id = ? ORDER BY ts DESC LIMIT 1",
        )
        .bind(health_check_id.to_string())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;

        row.map(|r| {
            DateTime::parse_from_rfc3339(r.try_get::<String, _>("ts")?.as_str())
                .map(|d| d.with_timezone(&Utc))
                .map_err(anyhow::Error::from)
        })
        .transpose()
        .map_err(StoreError::Backend)
    }

    async fn upsert_docker_containers(
        &self,
        server_id: Uuid,
        containers: &[DockerContainer],
    ) -> StoreResult<()> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;

        sqlx::query("DELETE FROM docker_containers WHERE server_id = ?")
            .bind(server_id.to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;

        for c in containers {
            sqlx::query(
                "INSERT INTO docker_containers
                    (server_id, container_id, name, image, state, cpu_pct, mem_used, mem_limit, net_rx, net_tx, restart_count, started_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(server_id.to_string())
            .bind(&c.container_id)
            .bind(&c.name)
            .bind(&c.image)
            .bind(container_state_str(c.state))
            .bind(c.cpu_pct)
            .bind(c.mem_used as i64)
            .bind(c.mem_limit as i64)
            .bind(c.net_rx as i64)
            .bind(c.net_tx as i64)
            .bind(c.restart_count as i64)
            .bind(c.started_at.map(|d| d.to_rfc3339()))
            .bind(c.updated_at.to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        }

        tx.commit()
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn list_docker_containers(&self, server_id: Uuid) -> StoreResult<Vec<DockerContainer>> {
        let rows = sqlx::query("SELECT * FROM docker_containers WHERE server_id = ? ORDER BY name")
            .bind(server_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        rows.iter()
            .map(|r| row_to_docker_container(r).map_err(StoreError::Backend))
            .collect()
    }

    async fn create_alert_rule(&self, rule: &AlertRule) -> StoreResult<()> {
        let condition_json =
            serde_json::to_string(&rule.condition).map_err(|e| StoreError::Backend(e.into()))?;
        sqlx::query(
            "INSERT INTO alert_rules (id, name, condition, severity, enabled, created_at) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(rule.id.to_string())
        .bind(&rule.name)
        .bind(condition_json)
        .bind(alert_severity_str(rule.severity))
        .bind(rule.enabled)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn list_alert_rules(&self) -> StoreResult<Vec<AlertRule>> {
        let rows = sqlx::query("SELECT * FROM alert_rules ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        rows.iter()
            .map(|r| row_to_alert_rule(r).map_err(StoreError::Backend))
            .collect()
    }

    async fn list_enabled_alert_rules(&self) -> StoreResult<Vec<AlertRule>> {
        let rows = sqlx::query("SELECT * FROM alert_rules WHERE enabled = 1 ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        rows.iter()
            .map(|r| row_to_alert_rule(r).map_err(StoreError::Backend))
            .collect()
    }

    async fn delete_alert_rule(&self, id: Uuid) -> StoreResult<()> {
        sqlx::query("DELETE FROM alert_rules WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn get_open_alert_event(
        &self,
        rule_id: Uuid,
        server_id: Option<Uuid>,
    ) -> StoreResult<Option<AlertEvent>> {
        let row = sqlx::query(
            "SELECT * FROM alert_events
             WHERE rule_id = ? AND state = 'firing'
               AND ((server_id IS NULL AND ? IS NULL) OR server_id = ?)
             ORDER BY started_at DESC LIMIT 1",
        )
        .bind(rule_id.to_string())
        .bind(server_id.map(|s| s.to_string()))
        .bind(server_id.map(|s| s.to_string()))
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        row.map(|r| row_to_alert_event(&r).map_err(StoreError::Backend))
            .transpose()
    }

    async fn create_alert_event(&self, event: &AlertEvent) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO alert_events (id, rule_id, server_id, state, started_at, resolved_at, notified_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(event.id.to_string())
        .bind(event.rule_id.to_string())
        .bind(event.server_id.map(|s| s.to_string()))
        .bind(if event.state == AlertState::Firing { "firing" } else { "resolved" })
        .bind(event.started_at.to_rfc3339())
        .bind(event.resolved_at.map(|d| d.to_rfc3339()))
        .bind(event.notified_at.map(|d| d.to_rfc3339()))
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn mark_alert_event_notified(&self, id: Uuid, ts: DateTime<Utc>) -> StoreResult<()> {
        sqlx::query("UPDATE alert_events SET notified_at = ? WHERE id = ?")
            .bind(ts.to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn resolve_alert_event(&self, id: Uuid, ts: DateTime<Utc>) -> StoreResult<()> {
        sqlx::query("UPDATE alert_events SET state = 'resolved', resolved_at = ? WHERE id = ?")
            .bind(ts.to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn list_recent_alert_events(&self, since: DateTime<Utc>) -> StoreResult<Vec<AlertEvent>> {
        let rows = sqlx::query(
            "SELECT * FROM alert_events WHERE started_at >= ? ORDER BY started_at DESC",
        )
        .bind(since.to_rfc3339())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        rows.iter()
            .map(|r| row_to_alert_event(r).map_err(StoreError::Backend))
            .collect()
    }

    async fn create_notification_channel(&self, channel: &NotificationChannel) -> StoreResult<()> {
        let config_json =
            serde_json::to_string(&channel.config).map_err(|e| StoreError::Backend(e.into()))?;
        sqlx::query(
            "INSERT INTO notification_channels (id, kind, config, enabled, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(channel.id.to_string())
        .bind(notification_channel_kind_str(channel.kind))
        .bind(config_json)
        .bind(channel.enabled)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn list_enabled_notification_channels(&self) -> StoreResult<Vec<NotificationChannel>> {
        let rows = sqlx::query("SELECT * FROM notification_channels WHERE enabled = 1")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        rows.iter()
            .map(|r| row_to_notification_channel(r).map_err(StoreError::Backend))
            .collect()
    }

    async fn create_incident(&self, incident: &Incident) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO incidents (id, title, started_at, resolved_at, status, server_id) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(incident.id.to_string())
        .bind(&incident.title)
        .bind(incident.started_at.to_rfc3339())
        .bind(incident.resolved_at.map(|d| d.to_rfc3339()))
        .bind(if incident.status == IncidentStatus::Open { "open" } else { "resolved" })
        .bind(incident.server_id.map(|s| s.to_string()))
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn find_open_incident_for_server(
        &self,
        server_id: Uuid,
        since: DateTime<Utc>,
    ) -> StoreResult<Option<Incident>> {
        let row = sqlx::query(
            "SELECT * FROM incidents WHERE server_id = ? AND status = 'open' AND started_at >= ?
             ORDER BY started_at DESC LIMIT 1",
        )
        .bind(server_id.to_string())
        .bind(since.to_rfc3339())
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        row.map(|r| row_to_incident(&r).map_err(StoreError::Backend))
            .transpose()
    }

    async fn resolve_incident(&self, id: Uuid, ts: DateTime<Utc>) -> StoreResult<()> {
        sqlx::query("UPDATE incidents SET status = 'resolved', resolved_at = ? WHERE id = ?")
            .bind(ts.to_rfc3339())
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn add_incident_event(&self, event: &IncidentEvent) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO incident_events (id, incident_id, ts, description, source_ref) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(event.id.to_string())
        .bind(event.incident_id.to_string())
        .bind(event.ts.to_rfc3339())
        .bind(&event.description)
        .bind(&event.source_ref)
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn list_incidents(&self) -> StoreResult<Vec<Incident>> {
        let rows = sqlx::query("SELECT * FROM incidents ORDER BY started_at DESC")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        rows.iter()
            .map(|r| row_to_incident(r).map_err(StoreError::Backend))
            .collect()
    }

    async fn get_incident(&self, id: Uuid) -> StoreResult<Option<Incident>> {
        let row = sqlx::query("SELECT * FROM incidents WHERE id = ?")
            .bind(id.to_string())
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        row.map(|r| row_to_incident(&r).map_err(StoreError::Backend))
            .transpose()
    }

    async fn list_incident_events(&self, incident_id: Uuid) -> StoreResult<Vec<IncidentEvent>> {
        let rows = sqlx::query("SELECT * FROM incident_events WHERE incident_id = ? ORDER BY ts")
            .bind(incident_id.to_string())
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        rows.iter()
            .map(|r| row_to_incident_event(r).map_err(StoreError::Backend))
            .collect()
    }

    async fn set_server_tags(&self, server_id: Uuid, tags: &[String]) -> StoreResult<()> {
        let tags_json = serde_json::to_string(tags).map_err(|e| StoreError::Backend(e.into()))?;
        sqlx::query("UPDATE servers SET tags = ? WHERE id = ?")
            .bind(tags_json)
            .bind(server_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn list_servers_with_tags(&self, tags: &[String]) -> StoreResult<Vec<Server>> {
        // Tags are stored as a JSON array string; SQLite has no native
        // array containment operator, so filter by substring match on the
        // quoted tag — safe because tags never contain a literal `"`.
        let rows = sqlx::query("SELECT * FROM servers ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        rows.iter()
            .map(|r| row_to_server(r).map_err(StoreError::Backend))
            .collect::<StoreResult<Vec<_>>>()
            .map(|servers| {
                servers
                    .into_iter()
                    .filter(|s| tags.iter().all(|t| s.tags.contains(t)))
                    .collect()
            })
    }

    async fn create_group(&self, id: Uuid, name: &str) -> StoreResult<()> {
        sqlx::query("INSERT INTO server_groups (id, name) VALUES (?, ?)")
            .bind(id.to_string())
            .bind(name)
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn list_groups(&self) -> StoreResult<Vec<(Uuid, String)>> {
        let rows = sqlx::query("SELECT id, name FROM server_groups ORDER BY name")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        rows.iter()
            .map(|r| {
                let id = Uuid::parse_str(r.try_get::<String, _>("id")?.as_str())?;
                let name: String = r.try_get("name")?;
                Ok((id, name))
            })
            .collect::<anyhow::Result<Vec<_>>>()
            .map_err(StoreError::Backend)
    }

    async fn delete_group(&self, id: Uuid) -> StoreResult<()> {
        sqlx::query("DELETE FROM server_groups WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn add_group_member(&self, group_id: Uuid, server_id: Uuid) -> StoreResult<()> {
        sqlx::query(
            "INSERT OR IGNORE INTO server_group_members (group_id, server_id) VALUES (?, ?)",
        )
        .bind(group_id.to_string())
        .bind(server_id.to_string())
        .execute(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn remove_group_member(&self, group_id: Uuid, server_id: Uuid) -> StoreResult<()> {
        sqlx::query("DELETE FROM server_group_members WHERE group_id = ? AND server_id = ?")
            .bind(group_id.to_string())
            .bind(server_id.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn list_group_members(&self, group_id: Uuid) -> StoreResult<Vec<Server>> {
        let rows = sqlx::query(
            "SELECT servers.* FROM servers
             JOIN server_group_members ON server_group_members.server_id = servers.id
             WHERE server_group_members.group_id = ?
             ORDER BY servers.name",
        )
        .bind(group_id.to_string())
        .fetch_all(&self.pool)
        .await
        .map_err(|e| StoreError::Backend(e.into()))?;
        rows.iter()
            .map(|r| row_to_server(r).map_err(StoreError::Backend))
            .collect()
    }

    async fn insert_logs(&self, entries: &[LogEntry]) -> StoreResult<()> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        for entry in entries {
            sqlx::query(
                "INSERT INTO logs (id, server_id, source, container_name, level, message, ts)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(entry.id.to_string())
            .bind(entry.server_id.to_string())
            .bind(log_source_str(entry.source))
            .bind(&entry.container_name)
            .bind(log_level_str(entry.level))
            .bind(&entry.message)
            .bind(entry.ts.to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        }
        tx.commit()
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        Ok(())
    }

    async fn list_logs(&self, query: &LogQuery) -> StoreResult<Vec<LogEntry>> {
        let mut builder: QueryBuilder<Sqlite> = QueryBuilder::new("SELECT * FROM logs WHERE 1 = 1");

        if let Some(server_id) = query.server_id {
            builder
                .push(" AND server_id = ")
                .push_bind(server_id.to_string());
        }
        if let Some(source) = query.source {
            builder
                .push(" AND source = ")
                .push_bind(log_source_str(source));
        }
        if let Some(min_level) = query.min_level {
            // Levels sort by severity in declaration order, so "at least
            // this severe" is every level string at or after it.
            let levels: Vec<&'static str> = [
                LogLevel::Trace,
                LogLevel::Debug,
                LogLevel::Info,
                LogLevel::Warn,
                LogLevel::Error,
            ]
            .into_iter()
            .filter(|l| *l >= min_level)
            .map(log_level_str)
            .collect();
            builder.push(" AND level IN (");
            let mut separated = builder.separated(", ");
            for level in levels {
                separated.push_bind(level);
            }
            separated.push_unseparated(")");
        }
        if let Some(q) = &query.q {
            builder
                .push(" AND message LIKE ")
                .push_bind(format!("%{q}%"));
        }
        if let Some(since) = query.since {
            builder.push(" AND ts >= ").push_bind(since.to_rfc3339());
        }
        if let Some(until) = query.until {
            builder.push(" AND ts <= ").push_bind(until.to_rfc3339());
        }
        builder
            .push(" ORDER BY ts DESC LIMIT ")
            .push_bind(query.limit as i64);

        let rows = builder
            .build()
            .fetch_all(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?;
        rows.iter()
            .map(|r| row_to_log_entry(r).map_err(StoreError::Backend))
            .collect()
    }

    async fn prune_logs(&self, before: DateTime<Utc>) -> StoreResult<u64> {
        let deleted = sqlx::query("DELETE FROM logs WHERE ts < ?")
            .bind(before.to_rfc3339())
            .execute(&self.pool)
            .await
            .map_err(|e| StoreError::Backend(e.into()))?
            .rows_affected();
        Ok(deleted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared::ServerStatus;

    fn sample_server() -> Server {
        Server {
            id: Uuid::new_v4(),
            name: "vps-01".into(),
            hostname: "vps-01.local".into(),
            ip: Some("10.0.0.1".into()),
            os: "Ubuntu 24.04".into(),
            arch: "x86_64".into(),
            agent_version: "0.1.0".into(),
            tags: vec!["production".into()],
            status: ServerStatus::Unknown,
            last_heartbeat_at: None,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn create_and_get_server_roundtrips() {
        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store
            .create_server(&server, &format!("hash-{}", server.id))
            .await
            .unwrap();

        let fetched = store.get_server(server.id).await.unwrap().unwrap();
        assert_eq!(fetched.name, "vps-01");
        assert_eq!(fetched.tags, vec!["production".to_string()]);
        assert_eq!(fetched.status, ServerStatus::Unknown);
    }

    #[tokio::test]
    async fn heartbeat_marks_server_online() {
        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store
            .create_server(&server, &format!("hash-{}", server.id))
            .await
            .unwrap();

        let ts = Utc::now();
        store.update_heartbeat(server.id, ts).await.unwrap();

        let fetched = store.get_server(server.id).await.unwrap().unwrap();
        assert_eq!(fetched.status, ServerStatus::Online);
        assert!(fetched.last_heartbeat_at.is_some());
    }

    #[tokio::test]
    async fn heartbeat_for_unknown_server_errors() {
        let store = SqliteStore::in_memory().await.unwrap();
        let err = store.update_heartbeat(Uuid::new_v4(), Utc::now()).await;
        assert!(matches!(err, Err(StoreError::NotFound)));
    }

    #[tokio::test]
    async fn list_servers_orders_by_name() {
        let store = SqliteStore::in_memory().await.unwrap();
        let mut a = sample_server();
        a.name = "b-server".into();
        let mut b = sample_server();
        b.id = Uuid::new_v4();
        b.name = "a-server".into();
        store.create_server(&a, "hash-a").await.unwrap();
        store.create_server(&b, "hash-b").await.unwrap();

        let servers = store.list_servers().await.unwrap();
        assert_eq!(servers[0].name, "a-server");
        assert_eq!(servers[1].name, "b-server");
    }

    #[tokio::test]
    async fn mark_servers_offline_only_flips_stale_ones() {
        let store = SqliteStore::in_memory().await.unwrap();

        let mut stale = sample_server();
        stale.id = Uuid::new_v4();
        store.create_server(&stale, "hash-stale").await.unwrap();
        store
            .update_heartbeat(stale.id, Utc::now() - chrono::Duration::minutes(5))
            .await
            .unwrap();

        let mut fresh = sample_server();
        fresh.id = Uuid::new_v4();
        store.create_server(&fresh, "hash-fresh").await.unwrap();
        store.update_heartbeat(fresh.id, Utc::now()).await.unwrap();

        let never_heard_from = sample_server();
        store
            .create_server(&never_heard_from, "hash-unknown")
            .await
            .unwrap();

        let threshold = Utc::now() - chrono::Duration::minutes(1);
        let changed = store.mark_servers_offline(threshold).await.unwrap();
        assert_eq!(changed.len(), 2);
        assert!(changed.contains(&stale.id));
        assert!(changed.contains(&never_heard_from.id));

        assert_eq!(
            store.get_server(stale.id).await.unwrap().unwrap().status,
            ServerStatus::Offline
        );
        assert_eq!(
            store.get_server(fresh.id).await.unwrap().unwrap().status,
            ServerStatus::Online
        );

        // A second pass over already-offline servers reports no changes.
        let changed_again = store.mark_servers_offline(threshold).await.unwrap();
        assert!(changed_again.is_empty());
    }

    #[tokio::test]
    async fn metrics_roundtrip_and_filter_by_time_range() {
        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store
            .create_server(&server, &format!("hash-{}", server.id))
            .await
            .unwrap();

        let now = Utc::now();
        let metric = shared::Metric {
            id: Uuid::new_v4(),
            server_id: server.id,
            ts: now,
            cpu_pct: 42.5,
            mem_used: 1024,
            mem_total: 2048,
            disk_used: 10,
            disk_total: 100,
            load1: 0.5,
            load5: 0.4,
            load15: 0.3,
            net_rx: 100,
            net_tx: 200,
            uptime_s: 3600,
        };
        store.insert_metric(&metric).await.unwrap();

        let in_range = store
            .list_metrics(
                server.id,
                now - chrono::Duration::minutes(1),
                now + chrono::Duration::minutes(1),
            )
            .await
            .unwrap();
        assert_eq!(in_range.len(), 1);
        assert_eq!(in_range[0].cpu_pct, 42.5);

        let out_of_range = store
            .list_metrics(
                server.id,
                now + chrono::Duration::minutes(10),
                now + chrono::Duration::minutes(20),
            )
            .await
            .unwrap();
        assert!(out_of_range.is_empty());
    }

    fn metric_at(server_id: Uuid, ts: DateTime<Utc>, cpu_pct: f32) -> shared::Metric {
        shared::Metric {
            id: Uuid::new_v4(),
            server_id,
            ts,
            cpu_pct,
            mem_used: 100,
            mem_total: 200,
            disk_used: 10,
            disk_total: 100,
            load1: 0.1,
            load5: 0.2,
            load15: 0.3,
            net_rx: 10,
            net_tx: 20,
            uptime_s: 60,
        }
    }

    #[tokio::test]
    async fn rollup_and_prune_averages_old_metrics_and_deletes_raw() {
        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store.create_server(&server, "hash-rollup").await.unwrap();

        // Two samples two days ago, in the same hour bucket.
        let bucket_hour = (Utc::now() - chrono::Duration::days(2))
            .date_naive()
            .and_hms_opt(10, 0, 0)
            .unwrap()
            .and_utc();
        store
            .insert_metric(&metric_at(
                server.id,
                bucket_hour + chrono::Duration::minutes(5),
                20.0,
            ))
            .await
            .unwrap();
        store
            .insert_metric(&metric_at(
                server.id,
                bucket_hour + chrono::Duration::minutes(35),
                40.0,
            ))
            .await
            .unwrap();
        // One recent sample that must survive the rollup untouched.
        store
            .insert_metric(&metric_at(server.id, Utc::now(), 99.0))
            .await
            .unwrap();

        let raw_cutoff = Utc::now() - chrono::Duration::hours(24);
        let (rolled_up, raw_deleted, pruned) = store
            .rollup_and_prune(raw_cutoff, Utc::now() - chrono::Duration::days(365))
            .await
            .unwrap();
        assert_eq!(rolled_up, 1);
        assert_eq!(raw_deleted, 2);
        assert_eq!(pruned, 0);

        let old_raw = store
            .list_metrics(
                server.id,
                bucket_hour - chrono::Duration::hours(1),
                bucket_hour + chrono::Duration::hours(1),
            )
            .await
            .unwrap();
        assert!(
            old_raw.is_empty(),
            "raw rows older than the cutoff must be deleted"
        );

        let rollups = store
            .list_metric_rollups(
                server.id,
                bucket_hour - chrono::Duration::hours(1),
                bucket_hour + chrono::Duration::hours(1),
            )
            .await
            .unwrap();
        assert_eq!(rollups.len(), 1);
        assert_eq!(rollups[0].cpu_pct, 30.0); // average of 20.0 and 40.0

        let recent = store
            .list_metrics(
                server.id,
                Utc::now() - chrono::Duration::minutes(1),
                Utc::now() + chrono::Duration::minutes(1),
            )
            .await
            .unwrap();
        assert_eq!(recent.len(), 1, "recent sample must not be rolled up yet");
    }

    #[tokio::test]
    async fn rollup_and_prune_removes_stale_rollups() {
        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store.create_server(&server, "hash-prune").await.unwrap();

        let ancient = Utc::now() - chrono::Duration::days(40);
        store
            .insert_metric(&metric_at(server.id, ancient, 50.0))
            .await
            .unwrap();

        // First pass rolls the ancient sample up.
        store
            .rollup_and_prune(
                Utc::now() - chrono::Duration::hours(24),
                Utc::now() - chrono::Duration::days(365),
            )
            .await
            .unwrap();
        let before_prune = store
            .list_metric_rollups(
                server.id,
                ancient - chrono::Duration::hours(1),
                ancient + chrono::Duration::hours(1),
            )
            .await
            .unwrap();
        assert_eq!(before_prune.len(), 1);

        // Second pass prunes rollups older than 30 days.
        let (rolled_up, _, pruned) = store
            .rollup_and_prune(
                Utc::now() - chrono::Duration::hours(24),
                Utc::now() - chrono::Duration::days(30),
            )
            .await
            .unwrap();
        assert_eq!(rolled_up, 0);
        assert_eq!(pruned, 1);

        let after_prune = store
            .list_metric_rollups(
                server.id,
                ancient - chrono::Duration::hours(1),
                ancient + chrono::Duration::hours(1),
            )
            .await
            .unwrap();
        assert!(after_prune.is_empty());
    }

    #[tokio::test]
    async fn pairing_token_is_single_use() {
        let store = SqliteStore::in_memory().await.unwrap();
        store
            .create_pairing_token(
                "hash1",
                "vps-02",
                Utc::now() + chrono::Duration::minutes(10),
            )
            .await
            .unwrap();

        let name = store.consume_pairing_token("hash1").await.unwrap();
        assert_eq!(name.as_deref(), Some("vps-02"));

        let second_use = store.consume_pairing_token("hash1").await.unwrap();
        assert!(second_use.is_none());
    }

    #[tokio::test]
    async fn expired_pairing_token_is_rejected() {
        let store = SqliteStore::in_memory().await.unwrap();
        store
            .create_pairing_token(
                "hash-expired",
                "vps-03",
                Utc::now() - chrono::Duration::minutes(1),
            )
            .await
            .unwrap();

        let result = store.consume_pairing_token("hash-expired").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn user_roundtrip() {
        let store = SqliteStore::in_memory().await.unwrap();
        let user = User {
            id: Uuid::new_v4(),
            username: "admin".into(),
            password_hash: "hashed".into(),
            created_at: Utc::now(),
        };
        store.create_user(&user).await.unwrap();

        let fetched = store.get_user_by_username("admin").await.unwrap().unwrap();
        assert_eq!(fetched.username, "admin");
        assert_eq!(fetched.password_hash, "hashed");
    }

    #[tokio::test]
    async fn health_check_crud_and_results() {
        let store = SqliteStore::in_memory().await.unwrap();
        let check = HealthCheck {
            id: Uuid::new_v4(),
            name: "api health".into(),
            kind: HealthCheckType::Https,
            target: "https://api.example.com/health".into(),
            interval_s: 30,
            timeout_s: 5,
            expected_status: Some(200),
            server_id: None,
        };
        store.create_health_check(&check).await.unwrap();

        assert!(store
            .last_health_check_result_at(check.id)
            .await
            .unwrap()
            .is_none());

        let checks = store.list_health_checks().await.unwrap();
        assert_eq!(checks.len(), 1);
        assert_eq!(checks[0].name, "api health");
        assert_eq!(checks[0].kind, HealthCheckType::Https);
        assert_eq!(checks[0].expected_status, Some(200));

        let now = Utc::now();
        store
            .insert_health_check_result(&HealthCheckResult {
                id: Uuid::new_v4(),
                health_check_id: check.id,
                ts: now,
                status_code: Some(200),
                latency_ms: 42,
                ok: true,
            })
            .await
            .unwrap();

        let last = store
            .last_health_check_result_at(check.id)
            .await
            .unwrap()
            .unwrap();
        assert!((last - now).num_seconds().abs() <= 1);

        let results = store
            .list_health_check_results(
                check.id,
                now - chrono::Duration::minutes(1),
                now + chrono::Duration::minutes(1),
            )
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert!(results[0].ok);
        assert_eq!(results[0].status_code, Some(200));

        store.delete_health_check(check.id).await.unwrap();
        assert!(store.list_health_checks().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn docker_upsert_replaces_previous_snapshot() {
        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store.create_server(&server, "hash-docker").await.unwrap();

        let nginx = DockerContainer {
            server_id: server.id,
            container_id: "abc123".into(),
            name: "nginx".into(),
            image: "nginx:latest".into(),
            state: ContainerState::Running,
            cpu_pct: 1.5,
            mem_used: 1024,
            mem_limit: 2048,
            net_rx: 10,
            net_tx: 20,
            restart_count: 0,
            started_at: Some(Utc::now()),
            updated_at: Utc::now(),
        };
        store
            .upsert_docker_containers(server.id, std::slice::from_ref(&nginx))
            .await
            .unwrap();

        let containers = store.list_docker_containers(server.id).await.unwrap();
        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].state, ContainerState::Running);

        // A container that stopped existing on the host (e.g. removed) is
        // simply absent from the next report, and the snapshot reflects
        // that — no stale rows linger.
        let mut stopped = nginx.clone();
        stopped.state = ContainerState::Stopped;
        store
            .upsert_docker_containers(server.id, &[stopped])
            .await
            .unwrap();
        let containers = store.list_docker_containers(server.id).await.unwrap();
        assert_eq!(containers.len(), 1);
        assert_eq!(containers[0].state, ContainerState::Stopped);

        store
            .upsert_docker_containers(server.id, &[])
            .await
            .unwrap();
        assert!(store
            .list_docker_containers(server.id)
            .await
            .unwrap()
            .is_empty());
    }

    #[tokio::test]
    async fn alert_rule_and_event_lifecycle() {
        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store.create_server(&server, "hash-alert").await.unwrap();

        let rule = AlertRule {
            id: Uuid::new_v4(),
            name: "cpu high".into(),
            condition: serde_json::json!({"kind": "cpu_above", "threshold_pct": 90.0, "for_secs": 300}),
            severity: AlertSeverity::Warning,
            enabled: true,
        };
        store.create_alert_rule(&rule).await.unwrap();
        assert_eq!(store.list_enabled_alert_rules().await.unwrap().len(), 1);

        assert!(store
            .get_open_alert_event(rule.id, Some(server.id))
            .await
            .unwrap()
            .is_none());

        let event = AlertEvent {
            id: Uuid::new_v4(),
            rule_id: rule.id,
            server_id: Some(server.id),
            state: AlertState::Firing,
            started_at: Utc::now(),
            resolved_at: None,
            notified_at: None,
        };
        store.create_alert_event(&event).await.unwrap();

        let open = store
            .get_open_alert_event(rule.id, Some(server.id))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(open.id, event.id);
        assert!(open.notified_at.is_none());

        store
            .mark_alert_event_notified(event.id, Utc::now())
            .await
            .unwrap();
        let open = store
            .get_open_alert_event(rule.id, Some(server.id))
            .await
            .unwrap()
            .unwrap();
        assert!(open.notified_at.is_some());

        store
            .resolve_alert_event(event.id, Utc::now())
            .await
            .unwrap();
        assert!(
            store
                .get_open_alert_event(rule.id, Some(server.id))
                .await
                .unwrap()
                .is_none(),
            "a resolved event is no longer open"
        );

        store.delete_alert_rule(rule.id).await.unwrap();
        assert!(store.list_alert_rules().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn notification_channels_roundtrip() {
        let store = SqliteStore::in_memory().await.unwrap();
        let channel = NotificationChannel {
            id: Uuid::new_v4(),
            kind: NotificationChannelKind::Webhook,
            config: serde_json::json!({"url": "https://example.com/hook"}),
            enabled: true,
        };
        store.create_notification_channel(&channel).await.unwrap();

        let channels = store.list_enabled_notification_channels().await.unwrap();
        assert_eq!(channels.len(), 1);
        assert_eq!(channels[0].kind, NotificationChannelKind::Webhook);
        assert_eq!(channels[0].config["url"], "https://example.com/hook");
    }

    #[tokio::test]
    async fn incident_correlation_and_timeline() {
        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store.create_server(&server, "hash-incident").await.unwrap();

        assert!(store
            .find_open_incident_for_server(server.id, Utc::now() - chrono::Duration::minutes(10))
            .await
            .unwrap()
            .is_none());

        let incident = Incident {
            id: Uuid::new_v4(),
            title: "VPS-01 issues".into(),
            started_at: Utc::now(),
            resolved_at: None,
            status: IncidentStatus::Open,
            server_id: Some(server.id),
        };
        store.create_incident(&incident).await.unwrap();

        let found = store
            .find_open_incident_for_server(server.id, Utc::now() - chrono::Duration::minutes(10))
            .await
            .unwrap()
            .expect("the incident just created for this server should be found as open");
        assert_eq!(found.id, incident.id);

        store
            .add_incident_event(&IncidentEvent {
                id: Uuid::new_v4(),
                incident_id: incident.id,
                ts: Utc::now(),
                description: "CPU exceeded 90%".into(),
                source_ref: None,
            })
            .await
            .unwrap();

        let events = store.list_incident_events(incident.id).await.unwrap();
        assert_eq!(events.len(), 1);

        store
            .resolve_incident(incident.id, Utc::now())
            .await
            .unwrap();
        let fetched = store.get_incident(incident.id).await.unwrap().unwrap();
        assert_eq!(fetched.status, IncidentStatus::Resolved);
        assert!(fetched.resolved_at.is_some());

        assert_eq!(store.list_incidents().await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn tag_filtering_requires_all_tags() {
        let store = SqliteStore::in_memory().await.unwrap();
        let mut prod_docker = sample_server();
        prod_docker.name = "prod-docker".into();
        store.create_server(&prod_docker, "hash-t1").await.unwrap();
        store
            .set_server_tags(prod_docker.id, &["production".into(), "docker".into()])
            .await
            .unwrap();

        let mut prod_only = sample_server();
        prod_only.id = Uuid::new_v4();
        prod_only.name = "prod-only".into();
        store.create_server(&prod_only, "hash-t2").await.unwrap();
        store
            .set_server_tags(prod_only.id, &["production".into()])
            .await
            .unwrap();

        let both = store
            .list_servers_with_tags(&["production".into(), "docker".into()])
            .await
            .unwrap();
        assert_eq!(both.len(), 1);
        assert_eq!(both[0].name, "prod-docker");

        let just_prod = store
            .list_servers_with_tags(&["production".into()])
            .await
            .unwrap();
        assert_eq!(just_prod.len(), 2);
    }

    #[tokio::test]
    async fn groups_crud_and_membership() {
        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store.create_server(&server, "hash-group").await.unwrap();

        let group_id = Uuid::new_v4();
        store.create_group(group_id, "production").await.unwrap();
        assert_eq!(
            store.list_groups().await.unwrap(),
            vec![(group_id, "production".to_string())]
        );

        store.add_group_member(group_id, server.id).await.unwrap();
        let members = store.list_group_members(group_id).await.unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].id, server.id);

        store
            .remove_group_member(group_id, server.id)
            .await
            .unwrap();
        assert!(store.list_group_members(group_id).await.unwrap().is_empty());

        store.delete_group(group_id).await.unwrap();
        assert!(store.list_groups().await.unwrap().is_empty());
    }

    fn sample_log(server_id: Uuid, source: LogSource, level: LogLevel, message: &str) -> LogEntry {
        LogEntry {
            id: Uuid::new_v4(),
            server_id,
            source,
            container_name: None,
            level,
            message: message.into(),
            ts: Utc::now(),
        }
    }

    #[tokio::test]
    async fn insert_and_list_logs_roundtrips() {
        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store.create_server(&server, "hash-logs").await.unwrap();

        let entries = vec![
            sample_log(
                server.id,
                LogSource::Agent,
                LogLevel::Info,
                "heartbeat sent",
            ),
            sample_log(
                server.id,
                LogSource::Docker,
                LogLevel::Error,
                "container nginx exited",
            ),
        ];
        store.insert_logs(&entries).await.unwrap();

        let all = store
            .list_logs(&LogQuery {
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(all.len(), 2);
    }

    #[tokio::test]
    async fn list_logs_filters_by_min_level_source_and_text() {
        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store.create_server(&server, "hash-logs-2").await.unwrap();

        store
            .insert_logs(&[
                sample_log(server.id, LogSource::Agent, LogLevel::Debug, "polling"),
                sample_log(
                    server.id,
                    LogSource::Agent,
                    LogLevel::Warn,
                    "retrying request",
                ),
                sample_log(
                    server.id,
                    LogSource::Docker,
                    LogLevel::Error,
                    "container nginx exited",
                ),
            ])
            .await
            .unwrap();

        let warn_and_above = store
            .list_logs(&LogQuery {
                min_level: Some(LogLevel::Warn),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(warn_and_above.len(), 2);
        assert!(warn_and_above.iter().all(|e| e.level >= LogLevel::Warn));

        let docker_only = store
            .list_logs(&LogQuery {
                source: Some(LogSource::Docker),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(docker_only.len(), 1);
        assert_eq!(docker_only[0].message, "container nginx exited");

        let text_match = store
            .list_logs(&LogQuery {
                q: Some("nginx".into()),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(text_match.len(), 1);
    }

    #[tokio::test]
    async fn list_logs_respects_server_scope_and_limit() {
        let store = SqliteStore::in_memory().await.unwrap();
        let a = sample_server();
        store.create_server(&a, "hash-a").await.unwrap();
        let mut b = sample_server();
        b.id = Uuid::new_v4();
        b.name = "vps-02".into();
        store.create_server(&b, "hash-b").await.unwrap();

        store
            .insert_logs(&[
                sample_log(a.id, LogSource::Agent, LogLevel::Info, "a-1"),
                sample_log(a.id, LogSource::Agent, LogLevel::Info, "a-2"),
                sample_log(b.id, LogSource::Agent, LogLevel::Info, "b-1"),
            ])
            .await
            .unwrap();

        let for_a = store
            .list_logs(&LogQuery {
                server_id: Some(a.id),
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(for_a.len(), 2);
        assert!(for_a.iter().all(|e| e.server_id == a.id));

        let limited = store
            .list_logs(&LogQuery {
                limit: 1,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(limited.len(), 1);
    }

    #[tokio::test]
    async fn prune_logs_deletes_only_older_entries() {
        let store = SqliteStore::in_memory().await.unwrap();
        let server = sample_server();
        store.create_server(&server, "hash-prune").await.unwrap();

        let mut old = sample_log(server.id, LogSource::Agent, LogLevel::Info, "old");
        old.ts = Utc::now() - chrono::Duration::days(2);
        let recent = sample_log(server.id, LogSource::Agent, LogLevel::Info, "recent");
        store.insert_logs(&[old, recent]).await.unwrap();

        let deleted = store
            .prune_logs(Utc::now() - chrono::Duration::days(1))
            .await
            .unwrap();
        assert_eq!(deleted, 1);

        let remaining = store
            .list_logs(&LogQuery {
                limit: 10,
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].message, "recent");
    }
}
