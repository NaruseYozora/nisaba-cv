use crate::{Error, Result, error::require, files};
use rusqlite::{Connection, backup::Backup, params};
use std::{path::Path, time::Duration};

pub const APPLICATION_ID: i64 = 0x52534d45;
pub const SCHEMA: i64 = 6;
const SQL: [&str; 6] = [
    include_str!("../migrations/001_initial.sql"),
    include_str!("../migrations/002_lifecycle.sql"),
    include_str!("../migrations/003_preset_kinds.sql"),
    include_str!("../migrations/004_snapshot_metadata.sql"),
    include_str!("../migrations/005_change_tracking.sql"),
    include_str!("../migrations/006_categories.sql"),
];
pub fn header(db: &Connection) -> Result<i64> {
    let id: i64 = db.query_row("PRAGMA application_id", [], |r| r.get(0))?;
    let version: i64 = db.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if version > SCHEMA {
        return Err(Error::Newer);
    }
    let tables: i64 = db.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
        [],
        |r| r.get(0),
    )?;
    require(
        (id == APPLICATION_ID && (1..=SCHEMA).contains(&version))
            || (id == 0 && version == 0 && tables == 0),
        "这不是有效的 Nisaba CV 资料库文件，无法打开",
    )?;
    Ok(version)
}
pub fn migrate(db: &mut Connection, root: &Path) -> Result<()> {
    let old = header(db)?;
    if old > 0 {
        for version in 1..=old {
            let checksum: String = db.query_row(
                "SELECT checksum FROM migrations WHERE version=?",
                [version],
                |r| r.get(0),
            )?;
            require(
                checksum == files::hash(SQL[(version - 1) as usize].as_bytes()),
                "数据库迁移记录与应用不一致",
            )?;
        }
    }
    if old > 0 && old < SCHEMA {
        let path = root.join(format!("before-migration-{}.sqlite3", uuid::Uuid::new_v4()));
        let mut copy = Connection::open(&path)?;
        Backup::new(db, &mut copy)?.run_to_completion(128, Duration::from_millis(1), None)?;
        drop(copy);
        std::fs::OpenOptions::new()
            .write(true)
            .open(path)?
            .sync_all()?;
    }
    if old < SCHEMA {
        let tx = db.transaction()?;
        for version in old + 1..=SCHEMA {
            tx.execute_batch(SQL[(version - 1) as usize])?;
            if version == 6 {
                crate::catalog::migrate_categories(&tx)?;
            }
            tx.execute(
                "INSERT INTO migrations VALUES (?,?,?)",
                params![
                    version,
                    files::hash(SQL[(version - 1) as usize].as_bytes()),
                    crate::store::now() as i64
                ],
            )?;
        }
        tx.pragma_update(None, "application_id", APPLICATION_ID)?;
        tx.pragma_update(None, "user_version", SCHEMA)?;
        // Validate populated older databases before committing any schema changes.
        // Fresh stores receive their singleton profile immediately after migration.
        if old > 0 {
            crate::store::validate_connection(&tx)?;
        }
        tx.commit()?;
    }
    Ok(())
}
pub fn validate_history(db: &Connection) -> Result<()> {
    require(header(db)? == SCHEMA, "备份数据库结构版本不支持")?;
    for (index, sql) in SQL.iter().enumerate() {
        let checksum: String = db.query_row(
            "SELECT checksum FROM migrations WHERE version=?",
            [(index + 1) as i64],
            |r| r.get(0),
        )?;
        require(checksum == files::hash(sql.as_bytes()), "迁移记录校验失败")?;
    }
    Ok(())
}
