-- Hourly aggregates of `metrics`. The scheduler rolls raw rows older than
-- the configured raw-retention window into here (see
-- Store::rollup_and_prune), then deletes them, so `metrics` stays bounded
-- regardless of how long a server has been monitored.
CREATE TABLE metric_rollups (
    id           TEXT PRIMARY KEY,
    server_id    TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    bucket_start TEXT NOT NULL,
    avg_cpu_pct  REAL NOT NULL,
    avg_mem_used INTEGER NOT NULL,
    avg_mem_total INTEGER NOT NULL,
    avg_disk_used INTEGER NOT NULL,
    avg_disk_total INTEGER NOT NULL,
    avg_load1    REAL NOT NULL,
    avg_load5    REAL NOT NULL,
    avg_load15   REAL NOT NULL,
    avg_net_rx   INTEGER NOT NULL,
    avg_net_tx   INTEGER NOT NULL,
    avg_uptime_s INTEGER NOT NULL,
    sample_count INTEGER NOT NULL
);
CREATE UNIQUE INDEX idx_metric_rollups_server_bucket ON metric_rollups(server_id, bucket_start);
