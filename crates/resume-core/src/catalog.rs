use crate::{
    Error, Result, Store,
    error::require,
    files,
    model::*,
    store::{id, require_changed},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const BUILTINS: [(&str, &str); 4] = [
    ("education", "教育经历"),
    ("work", "工作经历"),
    ("project", "项目经历"),
    ("skill", "技能"),
];
const FIELD_KINDS: [(&str, &str); 5] = [
    ("education", "教育经历"),
    ("work", "工作经历"),
    ("project", "项目经历"),
    ("skill", "技能"),
    ("custom", "自定义"),
];
pub fn kind_title(kind: &str) -> &str {
    FIELD_KINDS
        .iter()
        .find(|(k, _)| *k == kind)
        .map(|(_, name)| *name)
        .unwrap_or(kind)
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Category {
    pub id: String,
    pub revision: i64,
    pub name: String,
    pub kind: String,
    pub builtin: bool,
    pub position: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CategoryRevision {
    pub id: String,
    pub revision: i64,
}
/// Native editor contract intentionally omits legacy summary. Existing text is retained.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileEditorDraft {
    pub name: String,
    pub title: String,
    pub phone: String,
    pub email: String,
    pub location: String,
    pub links: Vec<Link>,
    pub photo_asset_id: Option<String>,
    pub custom_fields: Vec<CustomField>,
}
pub fn valid_kind(kind: &str) -> bool {
    FIELD_KINDS.iter().any(|(k, _)| *k == kind)
}
pub(crate) fn category_on(db: &Connection, category_id: &str) -> Result<Category> {
    db.query_row(
        "SELECT id,revision,name,kind,builtin,position FROM categories WHERE id=?",
        [category_id],
        |r| {
            Ok(Category {
                id: r.get(0)?,
                revision: r.get(1)?,
                name: r.get(2)?,
                kind: r.get(3)?,
                builtin: r.get(4)?,
                position: r.get::<_, u32>(5)? as usize,
            })
        },
    )
    .optional()?
    .ok_or_else(|| Error::Missing(category_id.into()))
}
fn key(name: &str) -> String {
    name.trim().to_lowercase()
}
fn validate_name(name: &str) -> Result<()> {
    require(
        !name.trim().is_empty() && name == name.trim(),
        "类别名称不能为空或含首尾空白",
    )?;
    text(name, 256)
}
fn insert_category(
    db: &Connection,
    category_id: &str,
    name: &str,
    kind: &str,
    builtin: bool,
) -> Result<()> {
    validate_name(name)?;
    require(valid_kind(kind), "类别素材类型无效")?;
    require(
        !db.prepare("SELECT 1 FROM categories WHERE name_key=?")?
            .exists([key(name)])?,
        "类别名称已存在",
    )?;
    db.execute("INSERT INTO categories VALUES (?,1,?,?,?,?,(SELECT COALESCE(MAX(position)+1,0) FROM categories))",params![category_id,name,key(name),kind,builtin])?;
    Ok(())
}
pub(crate) fn legacy_category(db: &Connection, content: &ItemContent) -> Result<String> {
    let ItemContent::Custom { category, .. } = content else {
        return Ok(format!("builtin:{}", content.kind()));
    };
    if category.trim().is_empty() || key(category) == key("自定义") {
        require(
            db.prepare("SELECT 1 FROM categories WHERE id='builtin:custom'")?
                .exists([])?,
            "自定义素材须先创建并命名类别",
        )?;
        return Ok("builtin:custom".into());
    }
    let category_id = format!("legacy-custom:{}", files::hash(key(category).as_bytes()));
    if db
        .prepare("SELECT 1 FROM categories WHERE id=?")?
        .exists([&category_id])?
    {
        return Ok(category_id);
    }
    let mut name = category.trim().to_owned();
    let mut suffix = 0;
    while db
        .prepare("SELECT 1 FROM categories WHERE name_key=?")?
        .exists([key(&name)])?
    {
        suffix += 1;
        name = format!(
            "{}（自定义 {suffix}）",
            category.trim().chars().take(230).collect::<String>()
        );
    }
    insert_category(db, &category_id, &name, "custom", false)?;
    Ok(category_id)
}
pub(crate) fn migrate_categories(db: &Connection) -> Result<()> {
    // Schema 6 is frozen; schema 7 removes the generic custom bucket.
    for (kind, name) in FIELD_KINDS {
        insert_category(db, &format!("builtin:{kind}"), name, kind, true)?;
    }
    let mut statement = db.prepare("SELECT id,kind,content FROM library_items ORDER BY id")?;
    let rows = statement
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    for (item_id, kind, json) in rows {
        let content: ItemContent = serde_json::from_str(&json)?;
        require(content.kind() == kind, "素材类型与内容不一致")?;
        let category_id = legacy_category(db, &content)?;
        db.execute(
            "INSERT INTO item_categories VALUES (?,?)",
            params![item_id, category_id],
        )?;
    }
    Ok(())
}
pub(crate) fn validate_catalog(db: &Connection) -> Result<()> {
    let mut statement = db.prepare("SELECT id,name_key FROM categories")?;
    let mut keys = HashSet::new();
    let mut positions = HashSet::new();
    for row in statement.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (category_id, stored_key) = row?;
        let c = category_on(db, &category_id)?;
        require(!c.id.is_empty(), "类别 ID 不能为空")?;
        text(&c.id, 256)?;
        validate_name(&c.name)?;
        require(
            c.revision > 0
                && valid_kind(&c.kind)
                && stored_key == key(&c.name)
                && keys.insert(stored_key)
                && positions.insert(c.position),
            "类别结构无效",
        )?;
        require(
            c.builtin == c.id.starts_with("builtin:")
                || (c.id == "builtin:custom" && c.kind == "custom" && !c.builtin),
            "内置类别标识无效",
        )?;
        if c.builtin {
            require(
                BUILTINS.iter().any(|(kind, _)| *kind == c.kind),
                "自定义类别不能是内置类别",
            )?;
            require(c.id == format!("builtin:{}", c.kind), "内置类别类型无效")?;
        }
    }
    for (kind, _) in BUILTINS {
        require(
            category_on(db, &format!("builtin:{kind}"))?.builtin,
            "缺少内置类别",
        )?;
    }
    require(!db.prepare("SELECT 1 FROM library_items i LEFT JOIN item_categories m ON m.item_id=i.id LEFT JOIN categories c ON c.id=m.category_id WHERE c.id IS NULL OR c.kind!=i.kind")?.exists([])?,"素材类别缺失或类型不一致")
}
impl Store {
    pub fn categories(&self) -> Result<Vec<Category>> {
        let mut statement = self
            .conn()?
            .prepare("SELECT id FROM categories ORDER BY position,id")?;
        let ids = statement
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.iter().map(|id| category_on(self.conn()?, id)).collect()
    }
    pub fn category(&self, category_id: &str) -> Result<Category> {
        category_on(self.conn()?, category_id)
    }
    pub fn create_category(&mut self, name: &str, kind: &str) -> Result<Category> {
        let category_id = id();
        insert_category(self.conn()?, &category_id, name.trim(), kind, false)?;
        self.category(&category_id)
    }
    pub fn rename_category(
        &mut self,
        category_id: &str,
        revision: i64,
        name: &str,
    ) -> Result<Category> {
        let name = name.trim();
        validate_name(name)?;
        require(
            !self
                .conn()?
                .prepare("SELECT 1 FROM categories WHERE name_key=? AND id!=?")?
                .exists(params![key(name), category_id])?,
            "类别名称已存在",
        )?;
        require_changed(self.conn()?.execute(
            "UPDATE categories SET name=?,name_key=?,revision=revision+1 WHERE id=? AND revision=?",
            params![name, key(name), category_id, revision],
        )?)?;
        self.category(category_id)
    }
    pub fn reorder_categories(&mut self, order: &[CategoryRevision]) -> Result<Vec<Category>> {
        unique(order.iter().map(|c| c.id.as_str()), "类别排序包含重复 ID")?;
        let tx = self.conn_mut()?.transaction()?;
        let count = tx.query_row("SELECT COUNT(*) FROM categories", [], |r| {
            r.get::<_, u32>(0)
        })? as usize;
        require(order.len() == count, "类别排序必须包含全部类别")?;
        for (position, c) in order.iter().enumerate() {
            require_changed(tx.execute(
                "UPDATE categories SET position=?,revision=revision+1 WHERE id=? AND revision=?",
                params![position as i64, c.id, c.revision],
            )?)?;
        }
        tx.commit()?;
        self.categories()
    }
    pub fn delete_category(&mut self, category_id: &str, revision: i64) -> Result<()> {
        let tx = self.conn_mut()?.transaction()?;
        let c = category_on(&tx, category_id)?;
        if c.revision != revision {
            return Err(Error::Conflict);
        }
        require(!c.builtin, "内置类别不能删除")?;
        // The UI confirms removal of all contents; resume copies are independent JSON.
        tx.execute("DELETE FROM library_items WHERE id IN (SELECT item_id FROM item_categories WHERE category_id=?)", [category_id])?;
        require_changed(tx.execute(
            "DELETE FROM categories WHERE id=? AND revision=?",
            params![category_id, revision],
        )?)?;
        tx.commit()?;
        Ok(())
    }
    pub fn category_items(
        &self,
        category_id: &str,
        state: RecordState,
        query: &str,
    ) -> Result<Vec<LibraryItem>> {
        self.category(category_id)?;
        Ok(self
            .items(state, query)?
            .into_iter()
            .filter(|i| i.category_id == category_id)
            .collect())
    }
    pub fn move_item_category(
        &mut self,
        item_id: &str,
        revision: i64,
        category_id: &str,
    ) -> Result<LibraryItem> {
        let tx = self.conn_mut()?.transaction()?;
        let item = crate::store::item_on(&tx, item_id)?;
        require(
            item.content.kind() == category_on(&tx, category_id)?.kind,
            "不能把素材移到不同结构类型的类别",
        )?;
        require_changed(tx.execute(
            "UPDATE library_items SET revision=revision+1,updated_at=? WHERE id=? AND revision=?",
            params![crate::store::now() as i64, item_id, revision],
        )?)?;
        tx.execute(
            "UPDATE item_categories SET category_id=? WHERE item_id=?",
            params![category_id, item_id],
        )?;
        tx.commit()?;
        self.item(item_id)
    }
    pub fn save_profile_editor(
        &mut self,
        draft: ProfileEditorDraft,
        revision: i64,
    ) -> Result<Profile> {
        let original = self.profile()?;
        if original.revision != revision {
            return Err(Error::Conflict);
        }
        self.save_profile(
            ProfileDraft {
                name: draft.name,
                title: draft.title,
                phone: draft.phone,
                email: draft.email,
                location: draft.location,
                summary: original.content.summary,
                links: draft.links,
                photo_asset_id: draft.photo_asset_id,
                custom_fields: draft.custom_fields,
            },
            revision,
        )
    }
}
