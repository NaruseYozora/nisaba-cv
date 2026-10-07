ALTER TABLE library_items ADD COLUMN state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','archived','trashed'));
ALTER TABLE resumes ADD COLUMN state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','archived','trashed'));
CREATE INDEX library_kind_state ON library_items(kind,state);
CREATE INDEX achievements_order ON achievement_items(item_id,position);
CREATE INDEX snapshots_resume ON snapshots(resume_id,created_at);
CREATE INDEX assets_by_reference ON asset_refs(asset_id);

