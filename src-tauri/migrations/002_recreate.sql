-- 002_recreate.sql
-- Pre-release: drop all data and recreate with clean schema.
-- The table structure is identical; only the JSON shape inside
-- action_json changes (new Action enum variants).

DROP TABLE IF EXISTS execution_logs;
DROP TABLE IF EXISTS tasks;

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    schedule_json TEXT NOT NULL,
    action_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    last_run_at TEXT,
    next_run_at TEXT
);

CREATE TABLE IF NOT EXISTS execution_logs (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    finished_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('success', 'failure', 'skipped')),
    stdout TEXT,
    stderr TEXT,
    error TEXT
);

CREATE INDEX IF NOT EXISTS idx_execution_logs_task_id ON execution_logs(task_id);
CREATE INDEX IF NOT EXISTS idx_execution_logs_started_at ON execution_logs(started_at);
