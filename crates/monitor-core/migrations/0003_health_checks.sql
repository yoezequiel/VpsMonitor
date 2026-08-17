-- V2: external service health checks (Proyecto.md sections 16-17). Run by
-- the Monitor on a schedule (see monitor_core::healthcheck +
-- scheduler::run_health_checks), independent of any Agent.
CREATE TABLE health_checks (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    kind            TEXT NOT NULL,
    target          TEXT NOT NULL,
    interval_s      INTEGER NOT NULL,
    timeout_s       INTEGER NOT NULL,
    expected_status INTEGER,
    server_id       TEXT REFERENCES servers(id) ON DELETE CASCADE,
    created_at      TEXT NOT NULL
);

CREATE TABLE health_check_results (
    id              TEXT PRIMARY KEY,
    health_check_id TEXT NOT NULL REFERENCES health_checks(id) ON DELETE CASCADE,
    ts              TEXT NOT NULL,
    status_code     INTEGER,
    latency_ms      INTEGER NOT NULL,
    ok              INTEGER NOT NULL
);
CREATE INDEX idx_health_check_results_check_ts ON health_check_results(health_check_id, ts);
