ALTER TABLE snapshots ADD COLUMN company TEXT NOT NULL DEFAULT '';
ALTER TABLE snapshots ADD COLUMN role TEXT NOT NULL DEFAULT '';
ALTER TABLE snapshots ADD COLUMN source_revision INTEGER NOT NULL DEFAULT 0 CHECK(source_revision>=0);
CREATE INDEX snapshots_history ON snapshots(resume_id,created_at);
