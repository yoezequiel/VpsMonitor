mod error;
pub mod sqlite;

pub use error::StoreError;
pub use sqlite::SqliteStore;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use shared::{Metric, Server, User};
use uuid::Uuid;

pub type StoreResult<T> = Result<T, StoreError>;

/// Persistence abstraction for the Monitor. The MVP implementation targets
/// SQLite (see `monitor-core::store::sqlite`, added in Fase 1); PostgreSQL
/// support (V4) is expected to implement this same trait so the rest of the
/// codebase stays backend-agnostic.
///
/// Scope is intentionally limited to what the MVP needs (servers, metrics,
/// users). Health checks, alerts and incidents get their own trait methods
/// once those features land in V2 — adding them here now would be
/// speculative.
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
}
