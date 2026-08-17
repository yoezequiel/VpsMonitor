use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shared::proto::{HeartbeatRequest, MetricsRequest, RegisterRequest, RegisterResponse};
use shared::{Metric, Server, ServerStatus};
use uuid::Uuid;

use crate::auth::{self, AuthAgent, AuthUser};
use crate::error::ApiError;
use crate::state::{AppState, ServerStatusEvent};
use crate::store::Store;
use crate::ws::ws_handler;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/auth/login", post(login))
        .route("/api/auth/logout", post(logout))
        .route("/api/agent/register", post(register))
        .route("/api/agent/heartbeat", post(heartbeat))
        .route("/api/agent/metrics", post(ingest_metrics))
        .route("/api/servers", get(list_servers))
        .route("/api/servers/{id}", get(get_server))
        .route("/api/servers/{id}/metrics", get(list_metrics))
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

async fn list_servers(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
) -> Result<Json<Vec<Server>>, ApiError> {
    Ok(Json(state.store.list_servers().await?))
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

async fn list_metrics(
    State(state): State<AppState>,
    AuthUser(_): AuthUser,
    Path(id): Path<Uuid>,
    Query(range): Query<MetricsRangeQuery>,
) -> Result<Json<Vec<Metric>>, ApiError> {
    let until = range.until.unwrap_or_else(Utc::now);
    let since = range.since.unwrap_or(until - chrono::Duration::hours(1));
    Ok(Json(state.store.list_metrics(id, since, until).await?))
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
    use shared::{Role, User};
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
                role: Role::Administrator,
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
}
