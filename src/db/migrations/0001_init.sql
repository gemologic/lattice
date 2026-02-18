CREATE TABLE IF NOT EXISTS projects (
    id              TEXT PRIMARY KEY,
    slug            TEXT UNIQUE NOT NULL,
    name            TEXT NOT NULL,
    goal            TEXT NOT NULL DEFAULT '',
    task_counter    INTEGER NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL,
    updated_at      TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS spec_sections (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    section     TEXT NOT NULL,
    content     TEXT NOT NULL DEFAULT '',
    updated_at  TEXT NOT NULL,
    CHECK (section IN ('overview', 'requirements', 'architecture', 'technical_design', 'open_decisions', 'references')),
    UNIQUE(project_id, section)
);

CREATE TABLE IF NOT EXISTS spec_revisions (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    section     TEXT NOT NULL,
    content     TEXT NOT NULL,
    edited_by   TEXT NOT NULL,
    created_at  TEXT NOT NULL,
    CHECK (section IN ('overview', 'requirements', 'architecture', 'technical_design', 'open_decisions', 'references'))
);

CREATE TABLE IF NOT EXISTS tasks (
    id           TEXT PRIMARY KEY,
    project_id   TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_number  INTEGER NOT NULL,
    title        TEXT NOT NULL,
    description  TEXT NOT NULL DEFAULT '',
    status       TEXT NOT NULL DEFAULT 'backlog',
    priority     TEXT NOT NULL DEFAULT 'medium',
    review_state TEXT NOT NULL DEFAULT 'ready',
    sort_order   REAL NOT NULL DEFAULT 0,
    created_by   TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    CHECK (status IN ('backlog', 'ready', 'in_progress', 'review', 'done')),
    CHECK (priority IN ('low', 'medium', 'high', 'critical')),
    CHECK (review_state IN ('ready', 'not_ready')),
    UNIQUE(project_id, task_number)
);

CREATE TABLE IF NOT EXISTS task_labels (
    task_id  TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    label    TEXT NOT NULL,
    PRIMARY KEY (task_id, label)
);

CREATE TABLE IF NOT EXISTS subtasks (
    id         TEXT PRIMARY KEY,
    task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    title      TEXT NOT NULL,
    done       INTEGER NOT NULL DEFAULT 0,
    sort_order REAL NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    CHECK (done IN (0, 1))
);

CREATE TABLE IF NOT EXISTS open_questions (
    id          TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    question    TEXT NOT NULL,
    context     TEXT NOT NULL DEFAULT '',
    answer      TEXT,
    status      TEXT NOT NULL DEFAULT 'open',
    asked_by    TEXT NOT NULL,
    resolved_by TEXT,
    created_at  TEXT NOT NULL,
    resolved_at TEXT,
    CHECK (status IN ('open', 'resolved'))
);

CREATE TABLE IF NOT EXISTS attachments (
    id           TEXT PRIMARY KEY,
    task_id      TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    filename     TEXT NOT NULL,
    content_type TEXT NOT NULL,
    size_bytes   INTEGER NOT NULL,
    storage_path TEXT NOT NULL,
    uploaded_by  TEXT NOT NULL,
    created_at   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS task_history (
    id         TEXT PRIMARY KEY,
    task_id    TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    actor      TEXT NOT NULL,
    action     TEXT NOT NULL,
    detail     TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS webhooks (
    id          TEXT PRIMARY KEY,
    project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    url         TEXT NOT NULL,
    platform    TEXT NOT NULL DEFAULT 'generic',
    events      TEXT NOT NULL DEFAULT '[]',
    secret      TEXT,
    active      INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    CHECK (platform IN ('slack', 'discord', 'generic')),
    CHECK (active IN (0, 1))
);

CREATE INDEX IF NOT EXISTS idx_tasks_project_status_sort ON tasks(project_id, status, sort_order);
CREATE INDEX IF NOT EXISTS idx_questions_task_status ON open_questions(task_id, status);
CREATE INDEX IF NOT EXISTS idx_history_task_created ON task_history(task_id, created_at);
