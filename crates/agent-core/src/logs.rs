//! Tails each running container's stdout/stderr since the last poll and
//! turns new lines into `LogEntryReport`s the reporter loop ships
//! alongside metrics (see PLAN.md's log viewer, `LogSource::Docker`).

use std::collections::HashMap;

use bollard::container::LogsOptions;
use bollard::Docker;
use chrono::{DateTime, Utc};
use futures_util::StreamExt;
use shared::proto::{DockerContainerReport, LogEntryReport};
use shared::{ContainerState, LogLevel, LogSource};

pub struct DockerLogTailer {
    last_seen: HashMap<String, DateTime<Utc>>,
}

impl DockerLogTailer {
    pub fn new() -> Self {
        Self {
            last_seen: HashMap::new(),
        }
    }

    /// Fetches new log lines for every *running* container in `containers`
    /// since the last call for that container id. A container's first
    /// poll only looks back a short window, so a freshly started Agent
    /// doesn't replay a container's entire log history.
    pub async fn collect(
        &mut self,
        docker: &Docker,
        containers: &[DockerContainerReport],
    ) -> Vec<LogEntryReport> {
        let mut out = Vec::new();
        for c in containers {
            if c.state != ContainerState::Running {
                continue;
            }
            let since = self.last_seen.get(&c.container_id).copied();
            let since_ts = since.unwrap_or_else(|| Utc::now() - chrono::Duration::seconds(30));

            let options = LogsOptions::<String> {
                stdout: true,
                stderr: true,
                timestamps: true,
                since: since_ts.timestamp(),
                tail: "200".to_string(),
                ..Default::default()
            };

            let mut stream = docker.logs(&c.container_id, Some(options));
            let mut latest = since;
            while let Some(frame) = stream.next().await {
                let Ok(chunk) = frame else {
                    break;
                };
                let bytes = chunk.into_bytes();
                for line in String::from_utf8_lossy(&bytes).lines() {
                    let Some((ts, message)) = parse_docker_log_line(line) else {
                        continue;
                    };
                    if since.is_some_and(|s| ts <= s) {
                        continue;
                    }
                    latest = Some(latest.map_or(ts, |l| l.max(ts)));
                    out.push(LogEntryReport {
                        source: LogSource::Docker,
                        container_name: Some(c.name.clone()),
                        level: LogLevel::Info,
                        message: message.to_string(),
                        ts,
                    });
                }
            }
            if let Some(l) = latest {
                self.last_seen.insert(c.container_id.clone(), l);
            }
        }
        out
    }
}

impl Default for DockerLogTailer {
    fn default() -> Self {
        Self::new()
    }
}

/// Docker's `timestamps: true` option prefixes each line with an RFC3339
/// timestamp and a space (e.g. `2024-01-01T00:00:00.123456789Z message`).
fn parse_docker_log_line(line: &str) -> Option<(DateTime<Utc>, &str)> {
    let (ts_str, rest) = line.split_once(' ')?;
    let ts = DateTime::parse_from_rfc3339(ts_str)
        .ok()?
        .with_timezone(&Utc);
    Some((ts, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_timestamped_docker_log_line() {
        let (ts, msg) = parse_docker_log_line("2024-01-01T00:00:00.123456789Z listening on :80")
            .expect("line parses");
        assert_eq!(ts.timestamp(), 1704067200);
        assert_eq!(msg, "listening on :80");
    }

    #[test]
    fn rejects_a_line_with_no_timestamp_prefix() {
        assert!(parse_docker_log_line("not a timestamp here").is_none());
    }
}
