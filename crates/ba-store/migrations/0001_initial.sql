CREATE TABLE projects (
    id TEXT PRIMARY KEY,
    root_path TEXT NOT NULL UNIQUE,
    updated_at TEXT NOT NULL,
    data TEXT NOT NULL
);

CREATE TABLE workstreams (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX workstreams_project_idx ON workstreams(project_id, updated_at DESC);

CREATE TABLE milestones (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    workstream_id TEXT REFERENCES workstreams(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX milestones_project_idx ON milestones(project_id, updated_at DESC);

CREATE TABLE tasks (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    workstream_id TEXT REFERENCES workstreams(id) ON DELETE SET NULL,
    milestone_id TEXT REFERENCES milestones(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX tasks_project_status_idx ON tasks(project_id, status, updated_at DESC);

CREATE TABLE agent_profiles (
    id TEXT PRIMARY KEY,
    role TEXT NOT NULL,
    runtime TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    data TEXT NOT NULL
);

CREATE TABLE runs (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    agent_profile_id TEXT REFERENCES agent_profiles(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX runs_task_status_idx ON runs(task_id, status, updated_at DESC);

CREATE TABLE dispatches (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    agent_profile_id TEXT REFERENCES agent_profiles(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX dispatches_run_idx ON dispatches(run_id, updated_at DESC);

CREATE TABLE approvals (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    status TEXT NOT NULL,
    requested_at TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX approvals_run_status_idx ON approvals(run_id, status, requested_at DESC);

CREATE TABLE gates (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    created_at TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX gates_task_idx ON gates(task_id, created_at DESC);

CREATE TABLE receipts (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE RESTRICT,
    task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    chain_sequence INTEGER NOT NULL CHECK (chain_sequence > 0),
    hash TEXT NOT NULL UNIQUE,
    previous_hash TEXT,
    created_at TEXT NOT NULL,
    data TEXT NOT NULL,
    UNIQUE(project_id, chain_sequence)
);
CREATE INDEX receipts_project_chain_idx ON receipts(project_id, chain_sequence);

CREATE TABLE artifacts (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id TEXT REFERENCES tasks(id) ON DELETE SET NULL,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX artifacts_project_idx ON artifacts(project_id, created_at DESC);

CREATE TABLE worktrees (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
    task_id TEXT NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    run_id TEXT REFERENCES runs(id) ON DELETE SET NULL,
    status TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    data TEXT NOT NULL
);
CREATE INDEX worktrees_project_status_idx ON worktrees(project_id, status, updated_at DESC);

CREATE TABLE run_events (
    run_id TEXT NOT NULL REFERENCES runs(id) ON DELETE CASCADE,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    kind TEXT NOT NULL,
    payload TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY(run_id, sequence)
) WITHOUT ROWID;
CREATE INDEX run_events_created_idx ON run_events(created_at);

CREATE TABLE runtime_logs (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    project_id TEXT REFERENCES projects(id) ON DELETE CASCADE,
    run_id TEXT REFERENCES runs(id) ON DELETE CASCADE,
    level TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX runtime_logs_created_idx ON runtime_logs(created_at);
