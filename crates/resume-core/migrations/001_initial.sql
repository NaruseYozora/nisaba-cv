CREATE TABLE migrations(version INTEGER PRIMARY KEY, checksum TEXT NOT NULL, applied_at INTEGER NOT NULL);
CREATE TABLE assets(id TEXT PRIMARY KEY, kind TEXT NOT NULL CHECK(kind IN ('photo','pdf')), mime TEXT NOT NULL, data BLOB NOT NULL, checksum TEXT NOT NULL, created_at INTEGER NOT NULL);
CREATE TABLE profile(singleton INTEGER PRIMARY KEY CHECK(singleton=1), revision INTEGER NOT NULL CHECK(revision>0), content TEXT NOT NULL CHECK(json_valid(content)));
CREATE TABLE library_items(id TEXT PRIMARY KEY, revision INTEGER NOT NULL CHECK(revision>0), kind TEXT NOT NULL, title TEXT NOT NULL, content TEXT NOT NULL CHECK(json_valid(content)), tags TEXT NOT NULL CHECK(json_valid(tags)), notes TEXT NOT NULL, updated_at INTEGER NOT NULL);
CREATE TABLE achievement_items(id TEXT PRIMARY KEY, item_id TEXT NOT NULL REFERENCES library_items(id) ON DELETE CASCADE, text TEXT NOT NULL, position INTEGER NOT NULL CHECK(position>=0), UNIQUE(item_id,position));
CREATE TABLE resumes(id TEXT PRIMARY KEY, revision INTEGER NOT NULL CHECK(revision>0), name TEXT NOT NULL, company TEXT NOT NULL, role TEXT NOT NULL, document TEXT NOT NULL CHECK(json_valid(document)), updated_at INTEGER NOT NULL);
CREATE TABLE presets(id TEXT PRIMARY KEY, revision INTEGER NOT NULL CHECK(revision>0), content TEXT NOT NULL CHECK(json_valid(content)));
CREATE TABLE snapshots(id TEXT PRIMARY KEY, resume_id TEXT NOT NULL REFERENCES resumes(id) ON DELETE CASCADE, name TEXT NOT NULL, document TEXT NOT NULL CHECK(json_valid(document)), pdf_asset_id TEXT REFERENCES assets(id), created_at INTEGER NOT NULL);
CREATE TABLE asset_refs(owner_kind TEXT NOT NULL CHECK(owner_kind IN ('profile','resume','snapshot')), owner_id TEXT NOT NULL, asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE RESTRICT, PRIMARY KEY(owner_kind,owner_id,asset_id));
CREATE TABLE app_settings(key TEXT PRIMARY KEY, revision INTEGER NOT NULL CHECK(revision>0), content TEXT NOT NULL CHECK(json_valid(content)));
CREATE TRIGGER snapshots_immutable BEFORE UPDATE ON snapshots BEGIN SELECT RAISE(ABORT,'snapshot is immutable'); END;

