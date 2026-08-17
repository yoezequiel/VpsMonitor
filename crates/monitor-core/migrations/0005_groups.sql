-- V2: named collections of servers (Proyecto.md section 26), distinct
-- from tags — a server can be tagged freely but belongs to a group only
-- when explicitly added.
CREATE TABLE server_groups (
    id   TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE
);

CREATE TABLE server_group_members (
    group_id  TEXT NOT NULL REFERENCES server_groups(id) ON DELETE CASCADE,
    server_id TEXT NOT NULL REFERENCES servers(id) ON DELETE CASCADE,
    PRIMARY KEY (group_id, server_id)
);
