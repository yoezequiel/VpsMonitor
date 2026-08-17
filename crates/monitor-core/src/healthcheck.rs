//! Executes HTTP/HTTPS/TCP/DNS health checks against arbitrary targets
//! (Proyecto.md sections 16-17). Ping (ICMP) needs raw sockets/elevated
//! privileges on most platforms and is deliberately left out of this
//! first pass — `HealthCheckType::Ping` reports a failed result with a
//! clear reason instead of silently doing nothing.

use std::time::Instant;

use chrono::Utc;
use shared::{HealthCheck, HealthCheckResult, HealthCheckType};
use uuid::Uuid;

/// Runs one check and returns its result. Never returns `Err` — a failed
/// check (timeout, connection refused, DNS failure, non-matching status)
/// is a normal outcome recorded as `ok: false`, not an error to propagate.
pub async fn run_check(check: &HealthCheck) -> HealthCheckResult {
    let start = Instant::now();
    let timeout = std::time::Duration::from_secs(check.timeout_s.max(1) as u64);

    let outcome = tokio::time::timeout(timeout, execute(check)).await;
    let latency_ms = start.elapsed().as_millis().min(u32::MAX as u128) as u32;

    let (status_code, ok) = match outcome {
        Ok(Ok(status_code)) => (status_code, is_ok(check, status_code)),
        Ok(Err(err)) => {
            tracing::debug!(check = %check.name, %err, "health check failed");
            (None, false)
        }
        Err(_) => (None, false), // timed out
    };

    HealthCheckResult {
        id: Uuid::new_v4(),
        health_check_id: check.id,
        ts: Utc::now(),
        status_code,
        latency_ms,
        ok,
    }
}

fn is_ok(check: &HealthCheck, status_code: Option<u16>) -> bool {
    match check.kind {
        HealthCheckType::Http | HealthCheckType::Https => {
            match (check.expected_status, status_code) {
                (Some(expected), Some(actual)) => expected == actual,
                (None, Some(actual)) => (200..300).contains(&actual),
                _ => false,
            }
        }
        // TCP/DNS/Ping have no status code — reaching `execute`'s Ok(None)
        // at all means the connection/resolution succeeded.
        HealthCheckType::Tcp | HealthCheckType::Dns | HealthCheckType::Ping => true,
    }
}

async fn execute(check: &HealthCheck) -> anyhow::Result<Option<u16>> {
    match check.kind {
        HealthCheckType::Http | HealthCheckType::Https => {
            // Unlike the Agent's loopback reporter, this hits arbitrary
            // external URLs the operator configured — a bad cert should
            // fail the check, not be silently trusted.
            let resp = reqwest::get(&check.target).await?;
            Ok(Some(resp.status().as_u16()))
        }
        HealthCheckType::Tcp => {
            tokio::net::TcpStream::connect(&check.target).await?;
            Ok(None)
        }
        HealthCheckType::Dns => {
            let target = if check.target.contains(':') {
                check.target.clone()
            } else {
                format!("{}:0", check.target)
            };
            let mut addrs = tokio::net::lookup_host(target).await?;
            if addrs.next().is_none() {
                anyhow::bail!("DNS lookup for {} returned no addresses", check.target);
            }
            Ok(None)
        }
        HealthCheckType::Ping => {
            anyhow::bail!("ping checks require raw sockets and aren't implemented yet")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_check(kind: HealthCheckType, target: &str) -> HealthCheck {
        HealthCheck {
            id: Uuid::new_v4(),
            name: "test".into(),
            kind,
            target: target.to_string(),
            interval_s: 30,
            timeout_s: 5,
            expected_status: None,
            server_id: None,
        }
    }

    #[tokio::test]
    async fn tcp_check_against_a_closed_port_fails() {
        // Port 1 is reserved and essentially never has anything listening.
        let check = base_check(HealthCheckType::Tcp, "127.0.0.1:1");
        let result = run_check(&check).await;
        assert!(!result.ok);
        assert_eq!(result.status_code, None);
    }

    #[tokio::test]
    async fn dns_check_resolves_localhost() {
        let check = base_check(HealthCheckType::Dns, "localhost");
        let result = run_check(&check).await;
        assert!(result.ok);
    }

    #[tokio::test]
    async fn dns_check_fails_for_a_bogus_tld() {
        let check = base_check(HealthCheckType::Dns, "this-host-should-not-exist.invalid");
        let result = run_check(&check).await;
        assert!(!result.ok);
    }

    #[tokio::test]
    async fn ping_checks_report_a_clean_failure_not_a_panic() {
        let check = base_check(HealthCheckType::Ping, "127.0.0.1");
        let result = run_check(&check).await;
        assert!(!result.ok);
    }

    #[test]
    fn http_ok_defaults_to_2xx_when_no_expected_status_set() {
        let check = base_check(HealthCheckType::Http, "http://example.invalid");
        assert!(is_ok(&check, Some(200)));
        assert!(!is_ok(&check, Some(404)));
        assert!(!is_ok(&check, Some(500)));
    }

    #[test]
    fn http_ok_matches_exact_expected_status_when_set() {
        let mut check = base_check(HealthCheckType::Http, "http://example.invalid");
        check.expected_status = Some(404);
        assert!(is_ok(&check, Some(404)));
        assert!(!is_ok(&check, Some(200)));
    }
}
