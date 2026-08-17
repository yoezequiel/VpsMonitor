mod error;
pub mod sqlite;

pub use error::StoreError;
pub use sqlite::SqliteStore;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use shared::{
    AlertEvent, AlertRule, DockerContainer, HealthCheck, HealthCheckResult, Incident,
    IncidentEvent, Metric, NotificationChannel, Server, User,
};
use uuid::Uuid;

pub type StoreResult<T> = Result<T, StoreError>;

/// Persistence abstraction for the Monitor. The MVP implementation targets
/// SQLite (see `monitor-core::store::sqlite`, added in Fase 1); PostgreSQL
/// support (V4) is expected to implement this same trait so the rest of the
/// codebase stays backend-agnostic.
#[async_trait]
pub trait Store: Send + Sync {
    // -- servers ---------------------------------------------------------
    /// `token_hash` is the hash of the permanent agent token and must be
    /// unique — it lives outside `Server` because the domain model is what
    /// gets serialized back to API clients, and a token hash should never
    /// round-trip through JSON.
    async fn create_server(&self, server: &Server, token_hash: &str) -> StoreResult<()>;
    async fn get_server(&self, id: Uuid) -> StoreResult<Option<Server>>;
    async fn list_servers(&self) -> StoreResult<Vec<Server>>;
    async fn update_heartbeat(&self, id: Uuid, ts: DateTime<Utc>) -> StoreResult<()>;
    async fn delete_server(&self, id: Uuid) -> StoreResult<()>;

    /// Flips every non-offline server whose last heartbeat is older than
    /// `threshold` (or that never sent one) to `offline`, returning the ids
    /// that actually changed so the caller can broadcast status events
    /// without re-diffing the whole table.
    async fn mark_servers_offline(&self, threshold: DateTime<Utc>) -> StoreResult<Vec<Uuid>>;

    // -- metrics -----------------------------------------------------------
    async fn insert_metric(&self, metric: &Metric) -> StoreResult<()>;
    async fn list_metrics(
        &self,
        server_id: Uuid,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> StoreResult<Vec<Metric>>;

    /// Hourly-averaged metrics for a range, read from `metric_rollups`
    /// rather than raw samples. Returned as `Metric` (averages standing in
    /// for point values) so API/frontend consumers don't need a second
    /// shape — see `Store::rollup_and_prune`.
    async fn list_metric_rollups(
        &self,
        server_id: Uuid,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> StoreResult<Vec<Metric>>;

    /// Maintenance pass: aggregates every raw metric older than
    /// `raw_cutoff` into hourly buckets in `metric_rollups`, deletes those
    /// raw rows, then deletes rollup buckets older than `rollup_cutoff`.
    /// Returns `(buckets_rolled_up, raw_rows_deleted, rollups_pruned)` for
    /// logging. Keeps the raw `metrics` table bounded regardless of how
    /// long a server has been monitored (PLAN.md section 2.3 / risk on
    /// `metrics` volume).
    async fn rollup_and_prune(
        &self,
        raw_cutoff: DateTime<Utc>,
        rollup_cutoff: DateTime<Utc>,
    ) -> StoreResult<(u64, u64, u64)>;

    // -- users ---------------------------------------------------------------
    async fn create_user(&self, user: &User) -> StoreResult<()>;
    async fn get_user_by_username(&self, username: &str) -> StoreResult<Option<User>>;

    // -- agent pairing / auth ---------------------------------------------
    /// Registers a one-time pairing token created by `monitor server add`.
    async fn create_pairing_token(
        &self,
        token_hash: &str,
        server_name: &str,
        expires_at: DateTime<Utc>,
    ) -> StoreResult<()>;

    /// Consumes a pairing token, returning the server name it was created
    /// for. Returns `Ok(None)` if the token is unknown, already used, or
    /// expired.
    async fn consume_pairing_token(&self, token_hash: &str) -> StoreResult<Option<String>>;

    async fn get_server_by_token_hash(&self, token_hash: &str) -> StoreResult<Option<Server>>;

    // -- sessions -----------------------------------------------------------
    async fn create_session(
        &self,
        id: Uuid,
        user_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> StoreResult<()>;

    /// Returns the session's owner, or `None` if the token is unknown or
    /// expired.
    async fn get_user_by_session_token_hash(&self, token_hash: &str) -> StoreResult<Option<User>>;

    async fn delete_session(&self, token_hash: &str) -> StoreResult<()>;

    // -- health checks -------------------------------------------------------
    async fn create_health_check(&self, check: &HealthCheck) -> StoreResult<()>;
    async fn list_health_checks(&self) -> StoreResult<Vec<HealthCheck>>;
    async fn delete_health_check(&self, id: Uuid) -> StoreResult<()>;

    async fn insert_health_check_result(&self, result: &HealthCheckResult) -> StoreResult<()>;
    async fn list_health_check_results(
        &self,
        health_check_id: Uuid,
        since: DateTime<Utc>,
        until: DateTime<Utc>,
    ) -> StoreResult<Vec<HealthCheckResult>>;

    /// Timestamp of the most recent result for this check, or `None` if it
    /// has never run — used by the scheduler to decide whether a check is
    /// due without keeping separate "last run" bookkeeping.
    async fn last_health_check_result_at(
        &self,
        health_check_id: Uuid,
    ) -> StoreResult<Option<DateTime<Utc>>>;

    // -- docker ---------------------------------------------------------------
    /// Replaces the stored snapshot for every container on `server_id`
    /// with `containers` (upsert by container_id, drop rows for
    /// containers no longer reported — they were removed from the host).
    async fn upsert_docker_containers(
        &self,
        server_id: Uuid,
        containers: &[DockerContainer],
    ) -> StoreResult<()>;
    async fn list_docker_containers(&self, server_id: Uuid) -> StoreResult<Vec<DockerContainer>>;

    // -- alert rules -------------------------------------------------------
    async fn create_alert_rule(&self, rule: &AlertRule) -> StoreResult<()>;
    async fn list_alert_rules(&self) -> StoreResult<Vec<AlertRule>>;
    async fn list_enabled_alert_rules(&self) -> StoreResult<Vec<AlertRule>>;
    async fn delete_alert_rule(&self, id: Uuid) -> StoreResult<()>;

    // -- alert events --------------------------------------------------------
    /// The still-open (unresolved) event for this rule/server pair, if
    /// any — a rule evaluates to at most one open event per server at a
    /// time, which is what lets the evaluator tell "still firing" apart
    /// from "newly firing".
    async fn get_open_alert_event(
        &self,
        rule_id: Uuid,
        server_id: Option<Uuid>,
    ) -> StoreResult<Option<AlertEvent>>;
    async fn create_alert_event(&self, event: &AlertEvent) -> StoreResult<()>;
    async fn mark_alert_event_notified(&self, id: Uuid, ts: DateTime<Utc>) -> StoreResult<()>;
    async fn resolve_alert_event(&self, id: Uuid, ts: DateTime<Utc>) -> StoreResult<()>;
    async fn list_recent_alert_events(&self, since: DateTime<Utc>) -> StoreResult<Vec<AlertEvent>>;

    // -- notification channels ------------------------------------------------
    async fn create_notification_channel(&self, channel: &NotificationChannel) -> StoreResult<()>;
    async fn list_enabled_notification_channels(&self) -> StoreResult<Vec<NotificationChannel>>;

    // -- incidents --------------------------------------------------------------
    async fn create_incident(&self, incident: &Incident) -> StoreResult<()>;
    /// The most recent still-open incident for this server that started
    /// within the correlation window, if any — used to fold a newly
    /// firing alert into an existing incident instead of opening a new
    /// one for every related event.
    async fn find_open_incident_for_server(
        &self,
        server_id: Uuid,
        since: DateTime<Utc>,
    ) -> StoreResult<Option<Incident>>;
    async fn resolve_incident(&self, id: Uuid, ts: DateTime<Utc>) -> StoreResult<()>;
    async fn add_incident_event(&self, event: &IncidentEvent) -> StoreResult<()>;
    async fn list_incidents(&self) -> StoreResult<Vec<Incident>>;
    async fn get_incident(&self, id: Uuid) -> StoreResult<Option<Incident>>;
    async fn list_incident_events(&self, incident_id: Uuid) -> StoreResult<Vec<IncidentEvent>>;

    // -- tags / groups ------------------------------------------------------------
    async fn set_server_tags(&self, server_id: Uuid, tags: &[String]) -> StoreResult<()>;
    async fn list_servers_with_tags(&self, tags: &[String]) -> StoreResult<Vec<Server>>;

    async fn create_group(&self, id: Uuid, name: &str) -> StoreResult<()>;
    async fn list_groups(&self) -> StoreResult<Vec<(Uuid, String)>>;
    async fn delete_group(&self, id: Uuid) -> StoreResult<()>;
    async fn add_group_member(&self, group_id: Uuid, server_id: Uuid) -> StoreResult<()>;
    async fn remove_group_member(&self, group_id: Uuid, server_id: Uuid) -> StoreResult<()>;
    async fn list_group_members(&self, group_id: Uuid) -> StoreResult<Vec<Server>>;
}
