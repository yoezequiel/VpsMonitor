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
