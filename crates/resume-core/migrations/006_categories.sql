CREATE TABLE categories(
 id TEXT PRIMARY KEY,
 revision INTEGER NOT NULL CHECK(revision>0),
 name TEXT NOT NULL,
 name_key TEXT NOT NULL UNIQUE,
 kind TEXT NOT NULL CHECK(kind IN ('education','work','project','skill','custom')),
 builtin INTEGER NOT NULL CHECK(builtin IN (0,1)),
 position INTEGER NOT NULL CHECK(position>=0)
);
CREATE TABLE item_categories(
 item_id TEXT PRIMARY KEY REFERENCES library_items(id) ON DELETE CASCADE,
 category_id TEXT NOT NULL REFERENCES categories(id) ON DELETE RESTRICT
);
CREATE INDEX item_categories_category ON item_categories(category_id);
CREATE TRIGGER changed_categories_INSERT AFTER INSERT ON categories BEGIN UPDATE change_state SET token=lower(hex(randomblob(16))) WHERE singleton=1; END;
CREATE TRIGGER changed_categories_UPDATE AFTER UPDATE ON categories BEGIN UPDATE change_state SET token=lower(hex(randomblob(16))) WHERE singleton=1; END;
CREATE TRIGGER changed_categories_DELETE AFTER DELETE ON categories BEGIN UPDATE change_state SET token=lower(hex(randomblob(16))) WHERE singleton=1; END;
CREATE TRIGGER changed_item_categories_INSERT AFTER INSERT ON item_categories BEGIN UPDATE change_state SET token=lower(hex(randomblob(16))) WHERE singleton=1; END;
CREATE TRIGGER changed_item_categories_UPDATE AFTER UPDATE ON item_categories BEGIN UPDATE change_state SET token=lower(hex(randomblob(16))) WHERE singleton=1; END;
CREATE TRIGGER changed_item_categories_DELETE AFTER DELETE ON item_categories BEGIN UPDATE change_state SET token=lower(hex(randomblob(16))) WHERE singleton=1; END;
