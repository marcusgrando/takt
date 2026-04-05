-- SQLite doesn't support ALTER CHECK constraints, so we recreate the table.
-- This preserves existing data.

CREATE TABLE execution_logs_new (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    started_at TEXT NOT NULL,
    finished_at TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('success', 'failure', 'skipped', 'schedule_error')),
    stdout TEXT,
    stderr TEXT,
    error TEXT
);

INSERT INTO execution_logs_new SELECT * FROM execution_logs;
DROP TABLE execution_logs;
ALTER TABLE execution_logs_new RENAME TO execution_logs;

CREATE INDEX IF NOT EXISTS idx_execution_logs_task_id ON execution_logs(task_id);
CREATE INDEX IF NOT EXISTS idx_execution_logs_started_at ON execution_logs(started_at);
