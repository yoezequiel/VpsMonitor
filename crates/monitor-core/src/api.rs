use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shared::proto::{
    DockerReportRequest, HeartbeatRequest, LogsReportRequest, MetricsRequest, RegisterRequest,
    RegisterResponse,
};
use shared::{
    AlertEvent, AlertRule, AlertSeverity, DockerContainer, HealthCheck, HealthCheckResult,
    HealthCheckType, Incident, IncidentEvent, LogEntry, LogLevel, LogSource, Metric,
    NotificationChannel, NotificationChannelKind, Server, ServerStatus,
};
use uuid::Uuid;

use crate::auth::{self, AuthAgent, AuthUser};
use crate::error::ApiError;
use crate::state::{AppState, ServerStatusEvent};
use crate::store::{LogQuery, Store};
use crate::ws::ws_handler;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/agent/register", post(register))
        .route("/api/agent/heartbeat", post(heartbeat))
        .route("/api/agent/metrics", post(ingest_metrics))
        .route("/api/agent/docker", post(ingest_docker))
        .route("/api/agent/logs", post(ingest_logs))
        .route("/api/logs", get(list_logs))
        .route("/api/servers", get(list_servers))
        .route("/api/servers/{id}", get(get_server))
        .route("/api/servers/{id}/metrics", get(list_metrics))
        .route("/api/servers/{id}/docker", get(list_docker_containers))
        .route("/api/servers/{id}/tags", post(set_server_tags))
        .route(
            "/api/healthchecks",
            get(list_health_checks).post(create_health_check),
        )
        .route("/api/healthchecks/{id}", delete(delete_health_check))
        .route(
            "/api/healthchecks/{id}/results",
            get(list_health_check_results),
        )
        .route(
            "/api/alerts/rules",
            get(list_alert_rules).post(create_alert_rule),
        )
        .route("/api/alerts/rules/{id}", delete(delete_alert_rule))
        .route("/api/alerts/events", get(list_alert_events))
        .route(
            "/api/notifications/channels",
            get(list_notification_channels).post(create_notification_channel),
        )
        .route("/api/incidents", get(list_incidents))
        .route("/api/incidents/{id}", get(get_incident))
        .route("/api/incidents/{id}/events", get(list_incident_events))
        .route("/api/groups", get(list_groups).post(create_group))
        .route("/api/groups/{id}", delete(delete_group))
        .route(
            "/api/groups/{id}/members/{server_id}",
            post(add_group_member).delete(remove_group_member),
        )
        .route("/api/groups/{id}/members", get(list_group_members))
        .route("/api/ws", get(ws_handler))
        .with_state(state)
}

#[derive(Debug, Deserialize)]
struct LoginRequest {
    username: String,
    password: String,
}

#[derive(Debug, Serialize)]
struct LoginResponse {
    token: String,
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>, ApiError> {
    let user = state
        .store
        .get_user_by_username(&req.username)
        .await?
        .ok_or(ApiError::Unauthorized)?;
    if !auth::verify_password(&req.password, &user.password_hash) {
        return Err(ApiError::Unauthorized);
    }

    let token = auth::generate_token();
    state
        .store
        .create_session(
            Uuid::new_v4(),
            user.id,
            &auth::hash_token(&token),
            Utc::now() + chrono::Duration::days(7),
        )
        .await?;

    Ok(Json(LoginResponse { token }))
}

async fn logout(State(state): State<AppState>, headers: HeaderMap) -> Result<StatusCode, ApiError> {
    let token = auth::extract_bearer(&headers)?;
    state.store.delete_session(&auth::hash_token(token)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// One-time pairing-token flow: `monitor server add` mints a token and
/// shows it to the operator, who pastes it into the Agent's config; the
/// Agent exchanges it here for a permanent bearer token. See
/// `Proyecto.md` section 10.
async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<Json<RegisterResponse>, ApiError> {
    let server_name = state
        .store
        .consume_pairing_token(&auth::hash_token(&req.pairing_token))
        .await?
        .ok_or_else(|| ApiError::BadRequest("invalid or expired pairing token".into()))?;

    let server = Server {
        id: Uuid::new_v4(),
        name: server_name,
        hostname: req.hostname,
        ip: None,
        os: req.os,
        arch: req.arch,
        agent_version: req.agent_version,
        tags: vec![],
        status: ServerStatus::Unknown,
        last_heartbeat_at: None,
        created_at: Utc::now(),
    };

    let agent_token = auth::generate_token();
    state
        .store
        .create_server(&server, &auth::hash_token(&agent_token))
        .await?;
    state.store.update_heartbeat(server.id, Utc::now()).await?;
    let _ = state.status_tx.send(ServerStatusEvent {
        server_id: server.id,
        status: ServerStatus::Online,
    });

    Ok(Json(RegisterResponse {
        server_id: server.id,
        agent_token,
    }))
}

async fn heartbeat(
    State(state): State<AppState>,
    AuthAgent(server): AuthAgent,
    Json(req): Json<HeartbeatRequest>,
) -> Result<StatusCode, ApiError> {
    if req.server_id != server.id {
        return Err(ApiError::BadRequest(
            "server_id does not match token".into(),
        ));
    }
    state.store.update_heartbeat(server.id, req.ts).await?;
    let _ = state.status_tx.send(ServerStatusEvent {
        server_id: server.id,
        status: ServerStatus::Online,
    });
    Ok(StatusCode::NO_CONTENT)
}

async fn ingest_metrics(
    State(state): State<AppState>,
    AuthAgent(server): AuthAgent,
    Json(req): Json<MetricsRequest>,
) -> Result<StatusCode, ApiError> {
    if req.server_id != server.id {
        return Err(ApiError::BadRequest(
            "server_id does not match token".into(),
        ));
    }
    let metric = Metric {
        id: Uuid::new_v4(),
        server_id: server.id,
        ts: req.ts,
        cpu_pct: req.cpu_pct,
        mem_used: req.mem_used,
        mem_total: req.mem_total,
        disk_used: req.disk_used,
        disk_total: req.disk_total,
        load1: req.load1,
        load5: req.load5,
        load15: req.load15,
        net_rx: req.net_rx,
        net_tx: req.net_tx,
        uptime_s: req.uptime_s,
    };
    state.store.insert_metric(&metric).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ingest_docker(
    State(state): State<AppState>,
    AuthAgent(server): AuthAgent,
    Json(req): Json<DockerReportRequest>,
) -> Result<StatusCode, ApiError> {
    if req.server_id != server.id {
        return Err(ApiError::BadRequest(
            "server_id does not match token".into(),
        ));
    }
    let now = Utc::now();
    let containers: Vec<DockerContainer> = req
        .containers
        .into_iter()
        .map(|c| DockerContainer {
            server_id: server.id,
            container_id: c.container_id,
            name: c.name,
            image: c.image,
            state: c.state,
            cpu_pct: c.cpu_pct,
            mem_used: c.mem_used,
            mem_limit: c.mem_limit,
            net_rx: c.net_rx,
            net_tx: c.net_tx,
            restart_count: c.restart_count,
            started_at: c.started_at,
            updated_at: now,
        })
        .collect();
    state
        .store
        .upsert_docker_containers(server.id, &containers)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn ingest_logs(
    State(state): State<AppState>,
    AuthAgent(server): AuthAgent,
    Json(req): Json<LogsReportRequest>,
) -> Result<StatusCode, ApiError> {
    if req.server_id != server.id {
        return Err(ApiError::BadRequest(
            "server_id does not match token".into(),
        ));
    }
    let entries: Vec<LogEntry> = req
        .entries
        .into_iter()
        .map(|e| LogEntry {
            id: Uuid::new_v4(),
            server_id: server.id,
            source: e.source,
            container_name: e.container_name,
            level: e.level,
            message: e.message,
            ts: e.ts,
        })
        .collect();
    state.store.insert_logs(&entries).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Deserialize)]
struct ListLogsQuery {
    server_id: Option<Uuid>,
    source: Option<LogSource>,
    /// Minimum severity — "warn" returns warn and error both.
    level: Option<LogLevel>,
    q: Option<String>,
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
    limit: Option<u32>,
}

async fn list_logs(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Query(params): Query<ListLogsQuery>,
) -> Result<Json<Vec<LogEntry>>, ApiError> {
    let query = LogQuery {
        server_id: params.server_id,
        source: params.source,
        min_level: params.level,
        q: params.q.filter(|q| !q.trim().is_empty()),
        since: params.since,
        until: params.until,
        limit: params.limit.unwrap_or(200).min(1000),
    };
    Ok(Json(state.store.list_logs(&query).await?))
}

async fn list_docker_containers(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<DockerContainer>>, ApiError> {
    Ok(Json(state.store.list_docker_containers(id).await?))
}

#[derive(Debug, Deserialize)]
struct ListServersQuery {
    /// Comma-separated tags; a server must have all of them (AND
    /// semantics — "production,docker" per Proyecto.md section 25).
    tags: Option<String>,
}

async fn list_servers(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Query(query): Query<ListServersQuery>,
) -> Result<Json<Vec<Server>>, ApiError> {
    match query.tags {
        Some(tags) => {
            let tags: Vec<String> = tags
                .split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();
            Ok(Json(state.store.list_servers_with_tags(&tags).await?))
        }
        None => Ok(Json(state.store.list_servers().await?)),
    }
}

#[derive(Debug, Deserialize)]
struct SetTagsRequest {
    tags: Vec<String>,
}

async fn set_server_tags(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<Uuid>,
    Json(req): Json<SetTagsRequest>,
) -> Result<StatusCode, ApiError> {
    state.store.set_server_tags(id, &req.tags).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn get_server(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Server>, ApiError> {
    let server = state
        .store
        .get_server(id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(server))
}

#[derive(Debug, Deserialize)]
struct MetricsRangeQuery {
    since: Option<DateTime<Utc>>,
    until: Option<DateTime<Utc>>,
}

/// Serves raw samples for the recent window and hourly rollups for
/// anything older, transparently — see `Store::rollup_and_prune`. The
/// response shape (`Vec<Metric>`) never changes, so the frontend doesn't
/// need to know which source served a given point.
async fn list_metrics(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<Uuid>,
    Query(range): Query<MetricsRangeQuery>,
) -> Result<Json<Vec<Metric>>, ApiError> {
    let until = range.until.unwrap_or_else(Utc::now);
    let since = range.since.unwrap_or(until - chrono::Duration::hours(1));
    let raw_cutoff = Utc::now() - state.raw_metrics_retention;

    let metrics = if since >= raw_cutoff {
        state.store.list_metrics(id, since, until).await?
    } else if until <= raw_cutoff {
        state.store.list_metric_rollups(id, since, until).await?
    } else {
        let mut merged = state
            .store
            .list_metric_rollups(id, since, raw_cutoff)
            .await?;
        merged.extend(state.store.list_metrics(id, raw_cutoff, until).await?);
        merged
    };

    Ok(Json(metrics))
}

#[derive(Debug, Deserialize)]
struct CreateHealthCheckRequest {
    name: String,
    #[serde(rename = "type")]
    kind: HealthCheckType,
    target: String,
    #[serde(default = "default_interval_s")]
    interval_s: u32,
    #[serde(default = "default_timeout_s")]
    timeout_s: u32,
    expected_status: Option<u16>,
    server_id: Option<Uuid>,
}

fn default_interval_s() -> u32 {
    30
}

fn default_timeout_s() -> u32 {
    5
}

async fn create_health_check(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Json(req): Json<CreateHealthCheckRequest>,
) -> Result<Json<HealthCheck>, ApiError> {
    if req.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".into()));
    }
    if req.target.trim().is_empty() {
        return Err(ApiError::BadRequest("target must not be empty".into()));
    }

    let check = HealthCheck {
        id: Uuid::new_v4(),
        name: req.name,
        kind: req.kind,
        target: req.target,
        interval_s: req.interval_s.max(5),
        timeout_s: req.timeout_s.max(1),
        expected_status: req.expected_status,
        server_id: req.server_id,
    };
    state.store.create_health_check(&check).await?;
    Ok(Json(check))
}

async fn list_health_checks(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
) -> Result<Json<Vec<HealthCheck>>, ApiError> {
    Ok(Json(state.store.list_health_checks().await?))
}

async fn delete_health_check(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.store.delete_health_check(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_health_check_results(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<Uuid>,
    Query(range): Query<MetricsRangeQuery>,
) -> Result<Json<Vec<HealthCheckResult>>, ApiError> {
    let until = range.until.unwrap_or_else(Utc::now);
    let since = range.since.unwrap_or(until - chrono::Duration::hours(24));
    Ok(Json(
        state
            .store
            .list_health_check_results(id, since, until)
            .await?,
    ))
}

#[derive(Debug, Deserialize)]
struct CreateAlertRuleRequest {
    name: String,
    condition: serde_json::Value,
    #[serde(default = "default_alert_severity")]
    severity: AlertSeverity,
    #[serde(default = "default_true")]
    enabled: bool,
}

fn default_alert_severity() -> AlertSeverity {
    AlertSeverity::Warning
}

fn default_true() -> bool {
    true
}

async fn create_alert_rule(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Json(req): Json<CreateAlertRuleRequest>,
) -> Result<Json<AlertRule>, ApiError> {
    let rule = AlertRule {
        id: Uuid::new_v4(),
        name: req.name,
        condition: req.condition,
        severity: req.severity,
        enabled: req.enabled,
    };
    // Validate the condition parses before persisting a rule the
    // evaluator would otherwise silently skip every cycle.
    if rule.parsed_condition().is_err() {
        return Err(ApiError::BadRequest(
            "condition does not match a known alert condition shape".into(),
        ));
    }
    state.store.create_alert_rule(&rule).await?;
    Ok(Json(rule))
}

async fn list_alert_rules(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
) -> Result<Json<Vec<AlertRule>>, ApiError> {
    Ok(Json(state.store.list_alert_rules().await?))
}

async fn delete_alert_rule(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.store.delete_alert_rule(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_alert_events(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Query(range): Query<MetricsRangeQuery>,
) -> Result<Json<Vec<AlertEvent>>, ApiError> {
    let since = range
        .since
        .unwrap_or(Utc::now() - chrono::Duration::days(7));
    Ok(Json(state.store.list_recent_alert_events(since).await?))
}

#[derive(Debug, Deserialize)]
struct CreateNotificationChannelRequest {
    kind: NotificationChannelKind,
    config: serde_json::Value,
    #[serde(default = "default_true")]
    enabled: bool,
}

async fn create_notification_channel(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Json(req): Json<CreateNotificationChannelRequest>,
) -> Result<Json<NotificationChannel>, ApiError> {
    let valid = match req.kind {
        NotificationChannelKind::Webhook => {
            serde_json::from_value::<shared::WebhookConfig>(req.config.clone()).is_ok()
        }
        NotificationChannelKind::Email => {
            serde_json::from_value::<shared::EmailConfig>(req.config.clone()).is_ok()
        }
    };
    if !valid {
        return Err(ApiError::BadRequest(
            "config does not match the expected shape for this channel kind".into(),
        ));
    }
    let channel = NotificationChannel {
        id: Uuid::new_v4(),
        kind: req.kind,
        config: req.config,
        enabled: req.enabled,
    };
    state.store.create_notification_channel(&channel).await?;
    Ok(Json(channel))
}

async fn list_notification_channels(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
) -> Result<Json<Vec<NotificationChannel>>, ApiError> {
    Ok(Json(
        state.store.list_enabled_notification_channels().await?,
    ))
}

async fn list_incidents(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
) -> Result<Json<Vec<Incident>>, ApiError> {
    Ok(Json(state.store.list_incidents().await?))
}

async fn get_incident(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Incident>, ApiError> {
    let incident = state
        .store
        .get_incident(id)
        .await?
        .ok_or(ApiError::NotFound)?;
    Ok(Json(incident))
}

async fn list_incident_events(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<IncidentEvent>>, ApiError> {
    Ok(Json(state.store.list_incident_events(id).await?))
}

#[derive(Debug, Serialize)]
struct GroupSummary {
    id: Uuid,
    name: String,
}

#[derive(Debug, Deserialize)]
struct CreateGroupRequest {
    name: String,
}

async fn create_group(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Json(req): Json<CreateGroupRequest>,
) -> Result<Json<GroupSummary>, ApiError> {
    if req.name.trim().is_empty() {
        return Err(ApiError::BadRequest("name must not be empty".into()));
    }
    let id = Uuid::new_v4();
    state.store.create_group(id, &req.name).await?;
    Ok(Json(GroupSummary { id, name: req.name }))
}

async fn list_groups(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
) -> Result<Json<Vec<GroupSummary>>, ApiError> {
    let groups = state.store.list_groups().await?;
    Ok(Json(
        groups
            .into_iter()
            .map(|(id, name)| GroupSummary { id, name })
            .collect(),
    ))
}

async fn delete_group(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.store.delete_group(id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn add_group_member(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path((group_id, server_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state.store.add_group_member(group_id, server_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_group_member(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path((group_id, server_id)): Path<(Uuid, Uuid)>,
) -> Result<StatusCode, ApiError> {
    state.store.remove_group_member(group_id, server_id).await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn list_group_members(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<Uuid>,
) -> Result<Json<Vec<Server>>, ApiError> {
    Ok(Json(state.store.list_group_members(id).await?))
}

/// Mints a one-time pairing token for a new server, valid for 15 minutes.
/// Used by `monitor server add`; kept here (rather than the CLI crate) so
/// a future admin API route can reuse it without duplicating the hashing
/// and expiry logic.
pub async fn create_pairing_token(store: &dyn Store, server_name: &str) -> anyhow::Result<String> {
    let token = auth::generate_token();
    store
        .create_pairing_token(
            &auth::hash_token(&token),
            server_name,
            Utc::now() + chrono::Duration::minutes(15),
        )
        .await?;
    Ok(token)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use serde_json::{json, Value};
    use shared::User;
    use tower::ServiceExt;

    use super::*;
    use crate::store::SqliteStore;

    async fn test_state() -> AppState {
        let store = SqliteStore::in_memory().await.unwrap();
        store
            .create_user(&User {
                id: Uuid::new_v4(),
                username: "admin".into(),
                password_hash: auth::hash_password("s3cret-password").unwrap(),
                created_at: Utc::now(),
            })
            .await
            .unwrap();
        AppState::new(Arc::new(store))
    }

    async fn json_body(response: axum::response::Response) -> Value {
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn json_request(method: &str, uri: &str, bearer: Option<&str>, body: Value) -> Request<Body> {
        let mut builder = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(token) = bearer {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        builder.body(Body::from(body.to_string())).unwrap()
    }

    #[tokio::test]
    async fn full_agent_and_dashboard_flow() {
        let state = test_state().await;
        let pairing_token = create_pairing_token(state.store.as_ref(), "vps-01")
            .await
            .unwrap();
        let app = router(state);

        // Agent exchanges the pairing token for a permanent token.
        let resp = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/agent/register",
                None,
                json!({
                    "pairing_token": pairing_token,
                    "hostname": "vps-01.local",
                    "os": "Ubuntu 24.04",
                    "arch": "x86_64",
                    "agent_version": "0.1.0",
                }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = json_body(resp).await;
        let agent_token = body["agent_token"].as_str().unwrap().to_string();
        let server_id = body["server_id"].as_str().unwrap().to_string();

        // Reusing the same pairing token must fail.
        let resp = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/agent/register",
                None,
                json!({
                    "pairing_token": pairing_token,
                    "hostname": "vps-01.local",
                    "os": "Ubuntu 24.04",
                    "arch": "x86_64",
                    "agent_version": "0.1.0",
                }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // Heartbeat with the agent token.
        let resp = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/agent/heartbeat",
                Some(&agent_token),
                json!({ "server_id": server_id, "ts": Utc::now().to_rfc3339() }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // Metrics with the agent token.
        let resp = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/agent/metrics",
                Some(&agent_token),
                json!({
                    "server_id": server_id,
                    "ts": Utc::now().to_rfc3339(),
                    "cpu_pct": 12.5, "mem_used": 100, "mem_total": 200,
                    "disk_used": 10, "disk_total": 100,
                    "load1": 0.1, "load5": 0.2, "load15": 0.3,
                    "net_rx": 10, "net_tx": 20, "uptime_s": 60,
                }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        // Dashboard routes require a session, not an agent token.
        let resp = app
            .clone()
            .oneshot(json_request("GET", "/api/servers", None, Value::Null))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let resp = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/auth/login",
                None,
                json!({ "username": "admin", "password": "s3cret-password" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let session_token = json_body(resp).await["token"].as_str().unwrap().to_string();

        let resp = app
            .clone()
            .oneshot(json_request(
                "GET",
                "/api/servers",
                Some(&session_token),
                Value::Null,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let servers = json_body(resp).await;
        assert_eq!(servers.as_array().unwrap().len(), 1);
        assert_eq!(servers[0]["name"], "vps-01");
        assert_eq!(servers[0]["status"], "online");

        let resp = app
            .clone()
            .oneshot(json_request(
                "GET",
                &format!("/api/servers/{server_id}/metrics"),
                Some(&session_token),
                Value::Null,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let metrics = json_body(resp).await;
        assert_eq!(metrics.as_array().unwrap().len(), 1);
        assert_eq!(metrics[0]["cpu_pct"], 12.5);
    }

    #[tokio::test]
    async fn health_check_routes_require_auth_and_round_trip() {
        let state = test_state().await;
        let app = router(state);

        let resp = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/healthchecks",
                None,
                json!({ "name": "api", "type": "https", "target": "https://example.com/health" }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

        let resp = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/auth/login",
                None,
                json!({ "username": "admin", "password": "s3cret-password" }),
            ))
            .await
            .unwrap();
        let session_token = json_body(resp).await["token"].as_str().unwrap().to_string();

        let resp = app
            .clone()
            .oneshot(json_request(
                "POST",
                "/api/healthchecks",
                Some(&session_token),
                json!({ "name": "api", "type": "https", "target": "https://example.com/health", "expected_status": 200 }),
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let created = json_body(resp).await;
        let check_id = created["id"].as_str().unwrap().to_string();
        assert_eq!(created["interval_s"], 30); // default applied

        let resp = app
            .clone()
            .oneshot(json_request(
                "GET",
                "/api/healthchecks",
                Some(&session_token),
                Value::Null,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let checks = json_body(resp).await;
        assert_eq!(checks.as_array().unwrap().len(), 1);

        let resp = app
            .clone()
            .oneshot(json_request(
                "GET",
                &format!("/api/healthchecks/{check_id}/results"),
                Some(&session_token),
                Value::Null,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(json_body(resp).await.as_array().unwrap().len(), 0);

        let resp = app
            .clone()
            .oneshot(json_request(
                "DELETE",
                &format!("/api/healthchecks/{check_id}"),
                Some(&session_token),
                Value::Null,
            ))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);

        let resp = app
            .clone()
            .oneshot(json_request(
                "GET",
                "/api/healthchecks",
                Some(&session_token),
                Value::Null,
            ))
            .await
            .unwrap();
        assert_eq!(json_body(resp).await.as_array().unwrap().len(), 0);
    }
}
