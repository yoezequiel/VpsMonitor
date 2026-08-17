use chrono::Utc;
use shared::proto::{HeartbeatRequest, MetricsRequest, RegisterRequest, RegisterResponse};
use uuid::Uuid;

/// Talks to a single Monitor over HTTPS. Holds the permanent agent token
/// once registered; the pairing token is only used once, at `register`.
pub struct Reporter {
    client: reqwest::Client,
    monitor_url: String,
    agent_token: Option<String>,
}

impl Reporter {
    /// A self-hosted Monitor has no public CA to issue a cert from, so its
    /// TLS certificate is self-signed (see `monitor_core::tls`). Until
    /// certificate pinning lands (V2+), the Agent trusts whatever
    /// certificate the Monitor presents — the same trust-on-first-contact
    /// model the pairing token itself relies on.
    pub fn new(monitor_url: impl Into<String>) -> Self {
        let client = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .expect("reqwest client with static TLS config always builds");
        Self {
            client,
            monitor_url: monitor_url.into(),
            agent_token: None,
        }
    }

    pub fn with_agent_token(mut self, token: impl Into<String>) -> Self {
        self.agent_token = Some(token.into());
        self
    }

    pub fn agent_token(&self) -> Option<&str> {
        self.agent_token.as_deref()
    }

    /// Exchanges a one-time pairing token (from `monitor server add`) for a
    /// permanent agent token and this node's assigned `server_id`.
    pub async fn register(
        &mut self,
        pairing_token: &str,
        hostname: &str,
        os: &str,
        arch: &str,
        agent_version: &str,
    ) -> anyhow::Result<Uuid> {
        let req = RegisterRequest {
            pairing_token: pairing_token.to_string(),
            hostname: hostname.to_string(),
            os: os.to_string(),
            arch: arch.to_string(),
            agent_version: agent_version.to_string(),
        };
        let resp: RegisterResponse = self
            .client
            .post(format!("{}/api/agent/register", self.monitor_url))
            .json(&req)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?;

        self.agent_token = Some(resp.agent_token);
        Ok(resp.server_id)
    }

    pub async fn heartbeat(&self, server_id: Uuid) -> anyhow::Result<()> {
        let req = HeartbeatRequest {
            server_id,
            ts: Utc::now(),
        };
        self.authed_post("/api/agent/heartbeat", &req).await
    }

    pub async fn send_metrics(&self, metric: &shared::Metric) -> anyhow::Result<()> {
        let req = MetricsRequest {
            server_id: metric.server_id,
            ts: metric.ts,
            cpu_pct: metric.cpu_pct,
            mem_used: metric.mem_used,
            mem_total: metric.mem_total,
            disk_used: metric.disk_used,
            disk_total: metric.disk_total,
            load1: metric.load1,
            load5: metric.load5,
            load15: metric.load15,
            net_rx: metric.net_rx,
            net_tx: metric.net_tx,
            uptime_s: metric.uptime_s,
        };
        self.authed_post("/api/agent/metrics", &req).await
    }

    async fn authed_post<T: serde::Serialize>(&self, path: &str, body: &T) -> anyhow::Result<()> {
        let token = self
            .agent_token
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("agent is not registered yet"))?;
        self.client
            .post(format!("{}{path}", self.monitor_url))
            .bearer_auth(token)
            .json(body)
            .send()
            .await?
            .error_for_status()?;
        Ok(())
    }
}

/// Runs the heartbeat + metrics reporting loop forever, at `interval`.
/// Errors are logged and swallowed — a single failed request must not kill
/// the Agent process, since the Monitor may just be temporarily
/// unreachable.
pub async fn run_loop(
    reporter: &Reporter,
    collector: &mut super::collector::Collector,
    server_id: Uuid,
    interval: std::time::Duration,
) {
    let mut ticker = tokio::time::interval(interval);
    loop {
        ticker.tick().await;

        if let Err(err) = reporter.heartbeat(server_id).await {
            tracing::warn!(%err, "heartbeat failed");
            continue;
        }

        let metric = collector.snapshot(server_id);
        if let Err(err) = reporter.send_metrics(&metric).await {
            tracing::warn!(%err, "metrics report failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use wiremock::matchers::{bearer_token, body_json, header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    use super::*;

    fn sample_metric(server_id: Uuid) -> shared::Metric {
        shared::Metric {
            id: Uuid::new_v4(),
            server_id,
            ts: Utc::now(),
            cpu_pct: 12.5,
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
    async fn register_success_sets_agent_token_and_returns_server_id() {
        let server = MockServer::start().await;
        let server_id = Uuid::new_v4();
        Mock::given(method("POST"))
            .and(path("/api/agent/register"))
            .and(body_json(json!({
                "pairing_token": "the-token",
                "hostname": "vps-01",
                "os": "Linux",
                "arch": "x86_64",
                "agent_version": "0.1.0",
            })))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "server_id": server_id,
                "agent_token": "permanent-token",
            })))
            .mount(&server)
            .await;

        let mut reporter = Reporter::new(server.uri());
        let returned_id = reporter
            .register("the-token", "vps-01", "Linux", "x86_64", "0.1.0")
            .await
            .unwrap();

        assert_eq!(returned_id, server_id);
        assert_eq!(reporter.agent_token(), Some("permanent-token"));
    }

    #[tokio::test]
    async fn register_failure_propagates_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/agent/register"))
            .respond_with(ResponseTemplate::new(400).set_body_json(json!({
                "error": "invalid or expired pairing token",
            })))
            .mount(&server)
            .await;

        let mut reporter = Reporter::new(server.uri());
        let result = reporter
            .register("bad-token", "vps-01", "Linux", "x86_64", "0.1.0")
            .await;

        assert!(result.is_err());
        assert_eq!(reporter.agent_token(), None);
    }

    #[tokio::test]
    async fn heartbeat_without_registration_errors_without_a_network_call() {
        let reporter = Reporter::new("https://127.0.0.1:0"); // unreachable on purpose
        let result = reporter.heartbeat(Uuid::new_v4()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn heartbeat_sends_bearer_token() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/agent/heartbeat"))
            .and(header("authorization", "Bearer permanent-token"))
            .and(bearer_token("permanent-token"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let reporter = Reporter::new(server.uri()).with_agent_token("permanent-token");
        reporter.heartbeat(Uuid::new_v4()).await.unwrap();
    }

    #[tokio::test]
    async fn send_metrics_sends_bearer_token() {
        let server = MockServer::start().await;
        let server_id = Uuid::new_v4();
        Mock::given(method("POST"))
            .and(path("/api/agent/metrics"))
            .and(bearer_token("permanent-token"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&server)
            .await;

        let reporter = Reporter::new(server.uri()).with_agent_token("permanent-token");
        reporter
            .send_metrics(&sample_metric(server_id))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn heartbeat_propagates_server_error() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/agent/heartbeat"))
            .respond_with(ResponseTemplate::new(500))
            .mount(&server)
            .await;

        let reporter = Reporter::new(server.uri()).with_agent_token("permanent-token");
        let result = reporter.heartbeat(Uuid::new_v4()).await;
        assert!(result.is_err());
    }
}
