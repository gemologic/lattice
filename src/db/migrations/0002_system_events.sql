CREATE TABLE IF NOT EXISTS system_events (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id     TEXT REFERENCES tasks(id) ON DELETE CASCADE,
    task_number INTEGER,
    actor       TEXT NOT NULL,
    action      TEXT NOT NULL,
    detail      TEXT NOT NULL DEFAULT '{}',
    created_at  TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_system_events_project_created
    ON system_events(project_id, created_at);
CREATE INDEX IF NOT EXISTS idx_system_events_created_id
    ON system_events(created_at, id);
