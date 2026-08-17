//! Reports the state of every container on this host each heartbeat
//! cycle (Proyecto.md section 15). Gracefully does nothing if Docker
//! isn't installed/running here — most self-hosted machines don't run
//! Docker, and that's not an error condition for the Agent.

use bollard::Docker;
use chrono::{DateTime, Utc};
use shared::proto::DockerContainerReport;
use shared::ContainerState;

pub struct DockerCollector {
    docker: Option<Docker>,
}

impl DockerCollector {
    pub fn new() -> Self {
        Self {
            docker: Docker::connect_with_local_defaults().ok(),
        }
    }

    /// `true` once, on the Agent's first successful connect — lets the
    /// caller log "Docker monitoring enabled" a single time instead of
    /// probing every cycle.
    pub fn available(&self) -> bool {
        self.docker.is_some()
    }

    pub async fn collect(&self) -> Vec<DockerContainerReport> {
        let Some(docker) = &self.docker else {
            return Vec::new();
        };

        let containers = match docker
            .list_containers(Some(bollard::container::ListContainersOptions::<String> {
                all: true,
                ..Default::default()
            }))
            .await
        {
            Ok(containers) => containers,
            Err(err) => {
                tracing::debug!(%err, "docker: failed to list containers");
                return Vec::new();
            }
        };

        let mut reports = Vec::with_capacity(containers.len());
        for container in containers {
            let Some(id) = container.id.clone() else {
                continue;
            };
            let name = container
                .names
                .and_then(|names| names.into_iter().next())
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_string();
            let image = container.image.clone().unwrap_or_default();
            let state = parse_state(container.state.as_deref().unwrap_or(""));

            let (restart_count, started_at) = inspect_extra(docker, &id).await;
            let (cpu_pct, mem_used, mem_limit, net_rx, net_tx) = if state == ContainerState::Running
            {
                stats_for(docker, &id).await.unwrap_or_default()
            } else {
                Default::default()
            };

            reports.push(DockerContainerReport {
                container_id: id,
                name,
                image,
                state,
                cpu_pct,
                mem_used,
                mem_limit,
                net_rx,
                net_tx,
                restart_count,
                started_at,
            });
        }
        reports
    }
}

impl Default for DockerCollector {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_state(s: &str) -> ContainerState {
    match s {
        "running" => ContainerState::Running,
        "exited" | "dead" | "created" => ContainerState::Stopped,
        "restarting" => ContainerState::Restarting,
        "paused" => ContainerState::Paused,
        _ => ContainerState::Unknown,
    }
}

async fn inspect_extra(docker: &Docker, id: &str) -> (u32, Option<DateTime<Utc>>) {
    match docker
        .inspect_container(id, None::<bollard::container::InspectContainerOptions>)
        .await
    {
        Ok(inspect) => {
            let restart_count = inspect
                .restart_count
                .and_then(|n| u32::try_from(n).ok())
                .unwrap_or(0);
            let started_at = inspect
                .state
                .and_then(|s| s.started_at)
                .and_then(|s| DateTime::parse_from_rfc3339(&s).ok())
                .map(|d| d.with_timezone(&Utc));
            (restart_count, started_at)
        }
        Err(err) => {
            tracing::debug!(%err, container = %id, "docker: inspect failed");
            (0, None)
        }
    }
}

/// One-shot (non-streaming) container stats, reduced to the handful of
/// numbers the dashboard needs. Docker's own `cpu_pct` formula: the
/// fraction of CPU-delta the container used out of system-delta, scaled
/// by the number of CPUs.
async fn stats_for(docker: &Docker, id: &str) -> Option<(f32, u64, u64, u64, u64)> {
    use futures_util::StreamExt;

    let mut stream = docker.stats(
        id,
        Some(bollard::container::StatsOptions {
            stream: false,
            one_shot: true,
        }),
    );
    let stats = stream.next().await?.ok()?;

    let cpu_delta = stats
        .cpu_stats
        .cpu_usage
        .total_usage
        .saturating_sub(stats.precpu_stats.cpu_usage.total_usage) as f64;
    let system_delta = stats
        .cpu_stats
        .system_cpu_usage
        .unwrap_or(0)
        .saturating_sub(stats.precpu_stats.system_cpu_usage.unwrap_or(0))
        as f64;
    let num_cpus = stats
        .cpu_stats
        .online_cpus
        .or_else(|| {
            stats
                .cpu_stats
                .cpu_usage
                .percpu_usage
                .as_ref()
                .map(|v| v.len() as u64)
        })
        .unwrap_or(1) as f64;
    let cpu_pct = if system_delta > 0.0 {
        ((cpu_delta / system_delta) * num_cpus * 100.0) as f32
    } else {
        0.0
    };

    let mem_used = stats.memory_stats.usage.unwrap_or(0);
    let mem_limit = stats.memory_stats.limit.unwrap_or(0);

    let (net_rx, net_tx) = stats
        .networks
        .unwrap_or_default()
        .values()
        .fold((0u64, 0u64), |(rx, tx), net| {
            (rx + net.rx_bytes, tx + net.tx_bytes)
        });

    Some((cpu_pct, mem_used, mem_limit, net_rx, net_tx))
}
