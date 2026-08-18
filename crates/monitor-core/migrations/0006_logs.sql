-- V2: log lines shipped by the Agent (its own process, plus tailed Docker
-- container output) and produced by the Monitor about itself in
-- Monitor+Agent mode. Pruned on a retention window like `metrics` — see
-- Store::prune_logs / scheduler::run_logs_prune.
CREATE TABLE logs (
    id              TEXT PRIMARY KEY,
    server_id       TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    source          TEXT NOT NULL,
    container_name  TEXT,
    level           TEXT NOT NULL,
    message         TEXT NOT NULL,
    ts              TEXT NOT NULL
);
CREATE INDEX idx_logs_server_ts ON logs(server_id, ts);
CREATE INDEX idx_logs_ts ON logs(ts);
