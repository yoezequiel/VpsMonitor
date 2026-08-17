use shared::Metric;
use sysinfo::{Disks, Networks, System};
use uuid::Uuid;

pub struct Collector {
    sys: System,
    disks: Disks,
    networks: Networks,
}

impl Collector {
    pub fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self {
            sys,
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
        }
    }

    /// Takes a fresh snapshot of system metrics. Cheap enough to call on
    /// every heartbeat interval (default 15-30s).
    pub fn snapshot(&mut self, server_id: Uuid) -> Metric {
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        self.disks.refresh(true);
        self.networks.refresh(true);

        let (disk_total, disk_used) =
            self.disks.iter().fold((0u64, 0u64), |(total, used), disk| {
                let disk_total = disk.total_space();
                let disk_free = disk.available_space();
                (
                    total + disk_total,
                    used + disk_total.saturating_sub(disk_free),
                )
            });

        let (net_rx, net_tx) = self
            .networks
            .iter()
            .fold((0u64, 0u64), |(rx, tx), (_, data)| {
                (rx + data.total_received(), tx + data.total_transmitted())
            });

        let load = System::load_average();

        Metric {
            id: Uuid::new_v4(),
            server_id,
            ts: chrono::Utc::now(),
            cpu_pct: self.sys.global_cpu_usage(),
            mem_used: self.sys.used_memory(),
            mem_total: self.sys.total_memory(),
            disk_used,
            disk_total,
            load1: load.one as f32,
            load5: load.five as f32,
            load15: load.fifteen as f32,
            net_rx,
            net_tx,
            uptime_s: System::uptime(),
        }
    }
}

impl Default for Collector {
    fn default() -> Self {
        Self::new()
    }
}

/// Static host identity used at registration time. Cheap, so it's read
/// straight from `sysinfo`'s free functions rather than cached on the
/// collector.
pub struct HostInfo {
    pub hostname: String,
    pub os: String,
    pub arch: String,
}

pub fn host_info() -> HostInfo {
    HostInfo {
        hostname: System::host_name().unwrap_or_else(|| "unknown".to_string()),
        os: System::long_os_version().unwrap_or_else(|| "unknown".to_string()),
        arch: System::cpu_arch(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Smoke tests only: real values depend on the machine running the
    // test, so we assert plausibility (non-negative, disk used <= total)
    // rather than exact numbers.
    #[test]
    fn snapshot_returns_plausible_values() {
        let mut collector = Collector::new();
        let server_id = Uuid::new_v4();
        let metric = collector.snapshot(server_id);

        assert_eq!(metric.server_id, server_id);
        assert!(metric.cpu_pct >= 0.0);
        assert!(metric.mem_total > 0, "a test machine always has some RAM");
        assert!(
            metric.mem_used <= metric.mem_total * 2,
            "sanity bound, not exact: swap accounting varies by OS"
        );
        assert!(metric.disk_used <= metric.disk_total);
        assert!(metric.load1 >= 0.0);
    }

    #[test]
    fn snapshot_ids_are_unique_across_calls() {
        let mut collector = Collector::new();
        let server_id = Uuid::new_v4();
        let a = collector.snapshot(server_id);
        let b = collector.snapshot(server_id);
        assert_ne!(a.id, b.id);
    }

    #[test]
    fn host_info_never_panics_and_reports_non_empty_fields() {
        let info = host_info();
        assert!(!info.hostname.is_empty());
        assert!(!info.arch.is_empty());
    }
}
