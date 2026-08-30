ALTER TABLE tasks
ADD COLUMN agent_profile_id TEXT REFERENCES agent_profiles(id) ON DELETE RESTRICT;

CREATE INDEX tasks_agent_profile_idx ON tasks(agent_profile_id, updated_at DESC);
