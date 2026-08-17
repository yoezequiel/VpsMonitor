-- V2: Docker container snapshots (latest known state per container, not
-- history — see shared::DockerContainer), alert rules/events, and
-- incidents grouping related alert events with a timeline.

CREATE TABLE docker_containers (
    server_id       TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    container_id    TEXT NOT NULL,
    name            TEXT NOT NULL,
    image           TEXT NOT NULL,
    state           TEXT NOT NULL,
    cpu_pct         REAL NOT NULL,
    mem_used        INTEGER NOT NULL,
    mem_limit       INTEGER NOT NULL,
    net_rx          INTEGER NOT NULL,
    net_tx          INTEGER NOT NULL,
    restart_count   INTEGER NOT NULL,
    started_at      TEXT,
    updated_at      TEXT NOT NULL,
    PRIMARY KEY (server_id, container_id)
);

CREATE TABLE alert_rules (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    condition   TEXT NOT NULL,
    severity    TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL
);

CREATE TABLE alert_events (
    id            TEXT PRIMARY KEY,
    rule_id       TEXT NOT NULL REFERENCES alert_rules(id) ON DELETE CASCADE,
    server_id     TEXT REFERENCES servers(id) ON DELETE CASCADE,
    state         TEXT NOT NULL,
    started_at    TEXT NOT NULL,
    resolved_at   TEXT,
    notified_at   TEXT
);
CREATE INDEX idx_alert_events_rule ON alert_events(rule_id);

CREATE TABLE notification_channels (
    id          TEXT PRIMARY KEY,
    kind        TEXT NOT NULL,
    config      TEXT NOT NULL,
    enabled     INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL
);

CREATE TABLE incidents (
    id            TEXT PRIMARY KEY,
    title         TEXT NOT NULL,
    started_at    TEXT NOT NULL,
    resolved_at   TEXT,
    status        TEXT NOT NULL,
    -- Correlation key: incidents group alert events for the same server
    -- opened within a short window of each other (see
    -- monitor_core::incidents). NULL for alerts with no specific server.
    server_id     TEXT REFERENCES servers(id) ON DELETE CASCADE
);

CREATE TABLE incident_events (
    id            TEXT PRIMARY KEY,
    incident_id   TEXT NOT NULL REFERENCES incidents(id) ON DELETE CASCADE,
    ts            TEXT NOT NULL,
    description   TEXT NOT NULL,
    source_ref    TEXT
);
CREATE INDEX idx_incident_events_incident ON incident_events(incident_id, ts);
