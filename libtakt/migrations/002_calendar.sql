-- Phase 1 of calendar-trigger feature: dedup + reconstitute table.
-- The table is populated only after Phase 3 lands the CalendarPoller.
-- In Phase 1 it stays empty; creating it now avoids a later schema churn.

CREATE TABLE calendar_dispatches (
    task_id TEXT NOT NULL,
    event_id TEXT NOT NULL,
    event_start TEXT NOT NULL,
    status TEXT NOT NULL CHECK(status IN ('scheduled', 'dispatched')),
    trigger_at TEXT NOT NULL,
    reserved_at TEXT NOT NULL,
    dispatched_at TEXT,
    PRIMARY KEY (task_id, event_id, event_start)
);

CREATE INDEX idx_calendar_dispatches_status ON calendar_dispatches(status);
CREATE INDEX idx_calendar_dispatches_start ON calendar_dispatches(event_start);
