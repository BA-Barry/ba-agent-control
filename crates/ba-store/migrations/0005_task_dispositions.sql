CREATE TABLE task_dispositions (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE RESTRICT,
    action TEXT NOT NULL CHECK(action IN ('archived', 'superseded', 'restored')),
    replacement_task_id TEXT REFERENCES tasks(id) ON DELETE RESTRICT,
    previous_disposition_id TEXT REFERENCES task_dispositions(id) ON DELETE RESTRICT,
    created_at TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX task_dispositions_task_idx ON task_dispositions(task_id, created_at DESC, id DESC);
CREATE INDEX task_dispositions_replacement_idx ON task_dispositions(replacement_task_id);
CREATE UNIQUE INDEX task_dispositions_previous_idx ON task_dispositions(previous_disposition_id) WHERE previous_disposition_id IS NOT NULL;
