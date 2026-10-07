use crate::{
    Error, Result, Store,
    error::require,
    files, migrations,
    model::Counts,
    store::{configure, counts, id, now, validate_connection},
};
use rusqlite::{Connection, OpenFlags, backup::Backup};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use tempfile::{NamedTempFile, TempDir};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

const MAX_DATABASE: u64 = 512 * 1024 * 1024;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub backup_format: u32,
    pub app_version: String,
    pub schema_version: i64,
    pub created_at: u64,
    pub sha256: String,
    pub database_bytes: u64,
    pub counts: Counts,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupPreview {
    pub manifest: Manifest,
    pub package_sha256: String,
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RestoreFault {
    None,
    BeforeSwitch,
    AfterOldMoved,
    AfterNewInstalled,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Journal {
    operation: String,
    old_sha256: String,
    committed: bool,
}

pub fn inspect(root: &Path, package: &Path) -> Result<BackupPreview> {
    let staging = TempDir::new_in(root)?;
    let (preview, _) = unpack(package, staging.path())?;
    Ok(preview)
}
fn unpack(package: &Path, directory: &Path) -> Result<(BackupPreview, PathBuf)> {
    let length = fs::metadata(package)?.len();
    require(
        length <= MAX_DATABASE + 1024 * 1024,
        "备份包超过当前 512MB 限制",
    )?;
    let bytes = fs::read(package)?;
    let package_sha256 = files::hash(&bytes);
    let mut archive = ZipArchive::new(std::io::Cursor::new(bytes))?;
    require(archive.len() == 2, "备份包文件数量无效")?;
    require(
        archive.by_name("manifest.json")?.size() <= 64 * 1024,
        "备份清单过大",
    )?;
    let manifest: Manifest =
        serde_json::from_reader(archive.by_name("manifest.json")?.take(64 * 1024 + 1))?;
    if manifest.schema_version > migrations::SCHEMA || manifest.backup_format > 1 {
        return Err(Error::Newer);
    }
    require(
        manifest.backup_format == 1 && manifest.schema_version >= 1,
        "备份格式不支持",
    )?;
    let mut member = archive.by_name("data.sqlite3")?;
    require(
        member.size() <= MAX_DATABASE && member.size() == manifest.database_bytes,
        "备份数据大小无效",
    )?;
    let mut data = Vec::new();
    member
        .by_ref()
        .take(MAX_DATABASE + 1)
        .read_to_end(&mut data)?;
    require(
        data.len() as u64 == manifest.database_bytes && files::hash(&data) == manifest.sha256,
        "备份校验失败",
    )?;
    let candidate = directory.join("candidate.sqlite3");
    files::atomic_write(&candidate, &data)?;
    let mut db = Connection::open(&candidate)?;
    require(
        migrations::header(&db)? == manifest.schema_version,
        "清单与数据库版本不一致",
    )?;
    migrations::migrate(&mut db, directory)?;
    validate_connection(&db)?;
    require(
        counts(&db)? == manifest.counts,
        "清单中的数量与数据库不一致",
    )?;
    drop(db);
    Ok((
        BackupPreview {
            manifest,
            package_sha256,
        },
        candidate,
    ))
}
impl Store {
    pub fn backup(&self, destination: &Path) -> Result<Manifest> {
        let destination = absolute(destination)?;
        require(
            destination != absolute(&self.root.join("library.sqlite3"))?
                && destination != absolute(&self.root.join("library.lock"))?,
            "备份不能覆盖活动库或锁文件",
        )?;
        require(
            destination.extension().is_some_and(|e| e == "rslbackup"),
            "备份文件扩展名须为 .rslbackup",
        )?;
        let parent = destination
            .parent()
            .ok_or_else(|| Error::Invalid("备份路径无效".into()))?;
        require(parent.is_dir(), "备份目录不存在")?;
        let staging = TempDir::new_in(parent)?;
        let copy = staging.path().join("data.sqlite3");
        {
            let mut target = Connection::open(&copy)?;
            Backup::new(self.conn()?, &mut target)?.run_to_completion(
                128,
                Duration::from_millis(1),
                None,
            )?;
            validate_connection(&target)?;
        }
        let data = fs::read(&copy)?;
        require(
            data.len() as u64 <= MAX_DATABASE,
            "资料库超过当前备份容量限制",
        )?;
        let copied = Connection::open_with_flags(&copy, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        let manifest = Manifest {
            backup_format: 1,
            app_version: env!("CARGO_PKG_VERSION").into(),
            schema_version: migrations::SCHEMA,
            created_at: now(),
            sha256: files::hash(&data),
            database_bytes: data.len() as u64,
            counts: counts(&copied)?,
        };
        drop(copied);
        let mut output = NamedTempFile::new_in(parent)?;
        {
            let mut zip = ZipWriter::new(output.as_file_mut());
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
            zip.start_file("manifest.json", options)?;
            zip.write_all(&serde_json::to_vec(&manifest)?)?;
            zip.start_file("data.sqlite3", options)?;
            zip.write_all(&data)?;
            zip.finish()?;
        }
        output.as_file().sync_all()?;
        // Validate the finished package before publishing it or recording success.
        inspect(staging.path(), output.path())?;
        files::replace(output.path(), &destination)?;
        files::sync_directory(parent)?;
        Ok(manifest)
    }
    pub fn inspect_backup(&self, package: &Path) -> Result<BackupPreview> {
        inspect(&self.root, package)
    }
    pub fn restore(
        &mut self,
        package: &Path,
        expected_sha256: &str,
        fault: RestoreFault,
    ) -> Result<()> {
        let operation = id();
        let directory = self.root.join("recovery").join(&operation);
        fs::create_dir_all(&directory)?;
        let (preview, candidate) = unpack(package, &directory)?;
        require(
            preview.package_sha256 == expected_sha256,
            "备份自预览后已发生变化，请重新选择",
        )?;
        let backup_dir = self.root.join("backups");
        fs::create_dir_all(&backup_dir)?;
        self.backup(&backup_dir.join(format!("before-restore-{operation}.rslbackup")))?;
        if fault == RestoreFault::BeforeSwitch {
            return Err(Error::Invalid("注入恢复切换故障".into()));
        }
        let (busy, log, checkpointed): (i64, i64, i64) =
            self.conn()?
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
                    Ok((row.get(0)?, row.get(1)?, row.get(2)?))
                })?;
        require(
            busy == 0 && (log == -1 || log == checkpointed),
            "资料库正在被占用，无法完成恢复前提交；请关闭占用程序后重试",
        )?;
        let active = self.root.join("library.sqlite3");
        let journal = Journal {
            operation: operation.clone(),
            old_sha256: files::hash(&fs::read(&active)?),
            committed: false,
        };
        let db = self.db.take().ok_or(Error::Unavailable)?;
        if let Err((db, error)) = db.close() {
            self.db = Some(db);
            return Err(error.into());
        }
        let switched = (|| -> Result<()> {
            files::atomic_write(
                &self.root.join("restore-journal.json"),
                &serde_json::to_vec(&journal)?,
            )?;
            files::replace(&active, &directory.join("previous.sqlite3"))?;
            crash_hook("oldMoved");
            if fault == RestoreFault::AfterOldMoved {
                return Err(Error::Invalid("注入旧库移开后故障".into()));
            }
            files::replace(&candidate, &active)?;
            crash_hook("newInstalled");
            if fault == RestoreFault::AfterNewInstalled {
                return Err(Error::Invalid("注入新库安装后故障".into()));
            }
            let db = Connection::open(&active)?;
            configure(&db)?;
            validate_connection(&db)?;
            self.db = Some(db);
            let committed = Journal {
                committed: true,
                ..journal
            };
            files::atomic_write(
                &self.root.join("restore-journal.json"),
                &serde_json::to_vec(&committed)?,
            )?;
            crash_hook("committed");
            let _ = fs::remove_file(self.root.join("restore-journal.json")); // Committed journal is safe to finish on next startup.
            Ok(())
        })();
        if let Err(original) = switched {
            if let Some(db) = self.db.take()
                && let Err((db, error)) = db.close()
            {
                self.db = Some(db);
                return Err(Error::Recovery(error.to_string()));
            }
            recover(&self.root)
                .map_err(|error| Error::Recovery(format!("{original}; 回退错误：{error}")))?;
            let db = Connection::open(&active)?;
            configure(&db)?;
            validate_connection(&db)?;
            self.db = Some(db);
            return Err(original);
        }
        Ok(())
    }
}
fn absolute(path: &Path) -> Result<PathBuf> {
    Ok(if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    })
}
fn crash_hook(point: &str) {
    // Explicit test-only opt-in. Normal desktop application never sets these variables.
    if std::env::var("RESUME_CORE_TEST_CRASH").ok().as_deref() == Some(point) {
        std::process::exit(73);
    }
}
pub(crate) fn recover(root: &Path) -> Result<()> {
    let journal_path = root.join("restore-journal.json");
    if !journal_path.exists() {
        return Ok(());
    }
    require(fs::metadata(&journal_path)?.len() <= 4096, "恢复记录过大")?;
    let journal: Journal = serde_json::from_slice(&fs::read(&journal_path)?)?;
    let uuid = uuid::Uuid::parse_str(&journal.operation)
        .map_err(|_| Error::Recovery("恢复操作 ID 无效".into()))?;
    require(uuid.to_string() == journal.operation, "恢复操作 ID 无效")?;
    let directory = root.join("recovery").join(&journal.operation);
    let previous = directory.join("previous.sqlite3");
    let active = root.join("library.sqlite3");
    if journal.committed && active.exists() {
        let db = Connection::open_with_flags(&active, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        if validate_connection(&db).is_ok() {
            drop(db);
            fs::remove_file(journal_path)?;
            return Ok(());
        }
    }
    if previous.exists() {
        let bytes = fs::read(&previous)?;
        require(
            files::hash(&bytes) == journal.old_sha256,
            "旧库校验失败，保留恢复记录待处理",
        )?;
        {
            let db = Connection::open_with_flags(&previous, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            validate_connection(&db)?;
        }
        if active.exists() {
            fs::copy(
                &active,
                directory.join(format!("interrupted-{}.sqlite3", id())),
            )?;
        }
        // Preserve old/candidate files until a later explicit cleanup policy. Never delete rollback data on an error.
        for suffix in ["-wal", "-shm"] {
            let path = root.join(format!("library.sqlite3{suffix}"));
            if path.exists() {
                fs::rename(
                    &path,
                    directory.join(format!("interrupted-{}{suffix}", id())),
                )?;
            }
        }
        files::atomic_write(&active, &bytes)?;
    } else {
        require(
            active.exists() && files::hash(&fs::read(&active)?) == journal.old_sha256,
            "旧库缺失，保留恢复记录待处理",
        )?;
        let db = Connection::open_with_flags(&active, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        validate_connection(&db)?;
    }
    fs::remove_file(journal_path)?;
    Ok(())
}
