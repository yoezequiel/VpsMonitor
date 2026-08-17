use async_trait::async_trait;
use chrono::{DateTime, Utc};
use shared::{Role, Server, ServerStatus, User};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use super::{Store, StoreError, StoreResult};

#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    /// Connects to (and creates if missing) a SQLite database file, then
    /// applies pending migrations.
    pub async fn connect(path: &str) -> anyhow::Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true);
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

fn parse_role(s: &str) -> Role {
    match s {
        "administrator" => Role::Administrator,
        "operator" => Role::Operator,
        _ => Role::Viewer,
    }
}

fn role_str(r: Role) -> &'static str {
    match r {
        Role::Administrator => "administrator",
        Role::Operator => "operator",
        Role::Viewer => "viewer",
    }
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

    async fn create_user(&self, user: &User) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO users (id, username, password_hash, role, created_at) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(user.id.to_string())
        .bind(&user.username)
        .bind(&user.password_hash)
        .bind(role_str(user.role))
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
            role: parse_role(
                row.try_get::<String, _>("role")
                    .map_err(|e| StoreError::Backend(e.into()))?
                    .as_str(),
            ),
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
            role: parse_role(
                row.try_get::<String, _>("role")
                    .map_err(|e| StoreError::Backend(e.into()))?
                    .as_str(),
            ),
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
            role: Role::Administrator,
            created_at: Utc::now(),
        };
        store.create_user(&user).await.unwrap();

        let fetched = store.get_user_by_username("admin").await.unwrap().unwrap();
        assert_eq!(fetched.role, Role::Administrator);
        assert_eq!(fetched.password_hash, "hashed");
    }
}
