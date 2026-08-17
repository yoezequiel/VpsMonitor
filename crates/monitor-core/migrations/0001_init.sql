CREATE TABLE servers (
    id              TEXT PRIMARY KEY,
    name            TEXT NOT NULL,
    hostname        TEXT NOT NULL,
    ip              TEXT,
    os              TEXT NOT NULL,
    arch            TEXT NOT NULL,
    agent_version   TEXT NOT NULL,
    tags            TEXT NOT NULL DEFAULT '[]',
    status          TEXT NOT NULL DEFAULT 'unknown',
    last_heartbeat_at TEXT,
    token_hash      TEXT NOT NULL UNIQUE,
    created_at      TEXT NOT NULL
);

CREATE TABLE metrics (
    id          TEXT PRIMARY KEY,
    server_id   TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    ts          TEXT NOT NULL,
    cpu_pct     REAL NOT NULL,
    mem_used    INTEGER NOT NULL,
    mem_total   INTEGER NOT NULL,
    disk_used   INTEGER NOT NULL,
    disk_total  INTEGER NOT NULL,
    load1       REAL NOT NULL,
    load5       REAL NOT NULL,
    load15      REAL NOT NULL,
    net_rx      INTEGER NOT NULL,
    net_tx      INTEGER NOT NULL,
    uptime_s    INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_metrics_server_ts ON metrics(server_id, ts);

CREATE TABLE users (
    id              TEXT PRIMARY KEY,
    username        TEXT NOT NULL UNIQUE,
    password_hash   TEXT NOT NULL,
    role            TEXT NOT NULL,
    created_at      TEXT NOT NULL
);

CREATE TABLE sessions (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    token_hash  TEXT NOT NULL UNIQUE,
    expires_at  TEXT NOT NULL
);

-- One-time tokens handed out by `monitor server add`; exchanged for a
-- permanent agent token on POST /api/agent/register.
CREATE TABLE pairing_tokens (
    token_hash   TEXT PRIMARY KEY,
    server_name  TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    expires_at   TEXT NOT NULL,
    used_at      TEXT
);
