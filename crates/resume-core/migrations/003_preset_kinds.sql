ALTER TABLE presets ADD COLUMN kind TEXT NOT NULL DEFAULT 'selection' CHECK(kind IN ('selection','layout'));
CREATE INDEX presets_kind ON presets(kind,id);
