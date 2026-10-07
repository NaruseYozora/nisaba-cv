use crate::{
    Error, Result, Store,
    error::require,
    files,
    model::BackupSettings,
    store::{id, now},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

const INTERVAL: u64 = 30 * 60 * 1000;
const INDEX: &str = "automatic-backup.json";

#[derive(Clone, Debug)]
pub struct BackupClock {
    pub millis: u64,
    pub day: String,
}
impl BackupClock {
    pub fn current() -> Self {
        let millis = now();
        #[cfg(windows)]
        let day = {
            let t = unsafe { windows::Win32::System::SystemInformation::GetLocalTime() };
            format!("{:04}-{:02}-{:02}", t.wYear, t.wMonth, t.wDay)
        };
        #[cfg(not(windows))]
        let day = format!("utc-day-{}", millis / 86_400_000);
        Self { millis, day }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    name: String,
    kind: String,
    at: u64,
    day: String,
    token: String,
    sha256: String,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Index {
    version: u8,
    owner: String,
    directory: String,
    records: Vec<Record>,
}
impl Default for Index {
    fn default() -> Self {
        Self {
            version: 1,
            owner: id(),
            directory: String::new(),
            records: vec![],
        }
    }
}
pub struct AutomaticBackup {
    index: Index,
    error: Option<String>,
    blocked: bool,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomaticBackupStatus {
    pub revision: i64,
    pub settings: BackupSettings,
    pub directory: String,
    pub last_success_at: Option<u64>,
    pub last_rolling_at: Option<u64>,
    pub last_daily_at: Option<u64>,
    pub last_error: Option<String>,
    pub managed_count: usize,
}
fn regular(path: &Path) -> Result<bool> {
    let m = match fs::symlink_metadata(path) {
        Ok(m) => m,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(e.into()),
    };
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        require(
            m.file_attributes() & 0x400 == 0,
            "自动备份文件不能是链接或重解析点",
        )?;
    }
    require(!m.file_type().is_symlink(), "自动备份文件不能是链接")?;
    Ok(m.is_file())
}
fn safe_directory(path: &Path) -> Result<()> {
    let m = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        require(
            m.file_attributes() & 0x400 == 0,
            "自动备份专用目录不能是重解析点",
        )?;
    }
    require(
        m.is_dir() && !m.file_type().is_symlink(),
        "自动备份专用目录无效",
    )
}
fn record_name_valid(r: &Record) -> bool {
    matches!(r.kind.as_str(), "rolling" | "daily")
        && r.name
            .strip_prefix(&format!("{}-", r.kind))
            .and_then(|n| n.strip_suffix(".rslbackup"))
            .is_some_and(|s| uuid::Uuid::parse_str(s).is_ok_and(|v| v.to_string() == s))
        && r.token.len() == 32
        && r.token.bytes().all(|c| c.is_ascii_hexdigit())
        && r.sha256.len() == 64
        && r.sha256.bytes().all(|c| c.is_ascii_hexdigit())
}
impl AutomaticBackup {
    pub(crate) fn load(root: &Path) -> Self {
        let path = root.join(INDEX);
        if !path.exists() {
            return Self {
                index: Index::default(),
                error: None,
                blocked: false,
            };
        }
        let loaded = (|| -> Result<Index> {
            require(
                regular(&path)? && fs::metadata(&path)?.len() <= 4 * 1024 * 1024,
                "自动备份索引过大或无效",
            )?;
            let index: Index = serde_json::from_slice(&fs::read(&path)?)?;
            require(
                index.version == 1
                    && uuid::Uuid::parse_str(&index.owner)
                        .is_ok_and(|v| v.to_string() == index.owner)
                    && index.records.len() <= 10000
                    && index.records.iter().all(record_name_valid),
                "自动备份索引无效",
            )?;
            Ok(index)
        })();
        match loaded {
            Ok(index) => Self {
                index,
                error: None,
                blocked: false,
            },
            Err(e) => Self {
                index: Index::default(),
                error: Some(format!(
                    "自动备份索引无法读取：{e}。点击重试将保留旧索引和全部旧备份，建立新的独立备份记录。"
                )),
                blocked: true,
            },
        }
    }
    fn persist(&self, root: &Path) -> Result<()> {
        files::atomic_write(&root.join(INDEX), &serde_json::to_vec_pretty(&self.index)?)
    }
    fn directory(&self, store: &Store, settings: &BackupSettings) -> PathBuf {
        settings
            .directory
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| store.root().join("automatic-backups"))
            .join(format!("resume-auto-{}", self.index.owner))
    }
    fn status(&self, store: &Store) -> Result<AutomaticBackupStatus> {
        let (revision, settings) = store.backup_settings()?;
        let directory = self.directory(store, &settings).display().to_string();
        let matching = self.index.directory == directory;
        let latest = |kind: &str| {
            self.index
                .records
                .iter()
                .filter(|r| matching && r.kind == kind)
                .max_by_key(|r| r.at)
                .map(|r| r.at)
        };
        Ok(AutomaticBackupStatus {
            revision,
            settings,
            directory,
            last_success_at: self
                .index
                .records
                .iter()
                .filter(|_| matching)
                .map(|r| r.at)
                .max(),
            last_rolling_at: latest("rolling"),
            last_daily_at: latest("daily"),
            last_error: self.error.clone(),
            managed_count: if matching {
                self.index.records.len()
            } else {
                0
            },
        })
    }
    fn run(&mut self, store: &Store, force: bool, clock: &BackupClock) -> Result<bool> {
        if self.blocked {
            require(
                force,
                "自动备份索引无法读取，请在设置中重试；手动备份仍可用",
            )?;
            // Preserve the unreadable index; never infer ownership from names found on disk.
            let old = store.root().join(INDEX);
            fs::rename(
                &old,
                store
                    .root()
                    .join(format!("automatic-backup-unreadable-{}.json", id())),
            )?;
            self.index = Index::default();
            self.blocked = false;
        }
        let (_, settings) = store.backup_settings()?;
        if !settings.rolling_enabled && !force {
            return Ok(false);
        }
        let directory = self.directory(store, &settings);
        fs::create_dir_all(&directory)?;
        safe_directory(&directory)?;
        if self.index.directory != directory.display().to_string() {
            self.index.directory = directory.display().to_string();
            self.index.records.clear();
            self.persist(store.root())?;
        }
        let token = store.change_token()?;
        let latest = |kind: &str| {
            self.index
                .records
                .iter()
                .filter(|r| r.kind == kind)
                .max_by_key(|r| r.at)
        };
        let rolling = match latest("rolling") {
            None => true,
            Some(r) => {
                force
                    || !regular(&directory.join(&r.name))?
                    || (r.token != token
                        && (clock.millis.saturating_sub(r.at) >= INTERVAL || clock.millis < r.at))
            }
        };
        let today = self
            .index
            .records
            .iter()
            .filter(|r| r.kind == "daily" && r.day == clock.day)
            .max_by_key(|r| r.at);
        let daily = match today {
            Some(r) => !regular(&directory.join(&r.name))?,
            None => {
                latest("daily").is_none()
                    || self
                        .index
                        .records
                        .iter()
                        .max_by_key(|r| r.at)
                        .is_none_or(|last| last.token != token)
            }
        };
        if rolling || daily {
            // Backup validates the completed package. Copy the same coherent snapshot for both tiers.
            let first_kind = if rolling { "rolling" } else { "daily" };
            let name = format!("{first_kind}-{}.rslbackup", id());
            let path = directory.join(&name);
            store.backup(&path)?;
            let bytes = fs::read(&path)?;
            let sha256 = files::hash(&bytes);
            let mut records = vec![Record {
                name,
                kind: first_kind.into(),
                at: clock.millis,
                day: clock.day.clone(),
                token: token.clone(),
                sha256: sha256.clone(),
            }];
            if rolling && daily {
                let name = format!("daily-{}.rslbackup", id());
                files::atomic_write(&directory.join(&name), &bytes)?;
                require(
                    files::hash(&fs::read(directory.join(&name))?) == sha256,
                    "日备份文件校验失败",
                )?;
                records.push(Record {
                    name,
                    kind: "daily".into(),
                    at: clock.millis,
                    day: clock.day.clone(),
                    token,
                    sha256,
                });
            }
            let previous = self.index.clone();
            self.index.records.extend(records);
            if let Err(e) = self.persist(store.root()) {
                self.index = previous;
                return Err(e);
            }
            // Only a newly validated, durably recorded backup permits rotation.
            self.rotate(
                store.root(),
                &directory,
                "rolling",
                settings.rolling_keep as usize,
            )?;
            self.rotate(
                store.root(),
                &directory,
                "daily",
                settings.daily_keep as usize,
            )?;
        }
        Ok(rolling || daily)
    }
    fn rotate(&mut self, root: &Path, directory: &Path, kind: &str, keep: usize) -> Result<()> {
        let mut records: Vec<_> = self
            .index
            .records
            .iter()
            .filter(|r| r.kind == kind)
            .cloned()
            .collect();
        records.sort_by_key(|r| r.at);
        let remove = records.len().saturating_sub(keep);
        for record in records.into_iter().take(remove) {
            safe_directory(directory)?;
            let path = directory.join(&record.name);
            if regular(&path)? {
                require(
                    files::hash(&fs::read(&path)?) == record.sha256,
                    "旧自动备份已被外部修改，已停止清理并保留文件",
                )?;
                fs::remove_file(&path)?;
                files::sync_directory(directory)?;
            } else {
                require(!path.exists(), "旧自动备份不是普通文件，已停止清理")?;
            }
            self.index.records.retain(|r| r.name != record.name);
            self.persist(root)?;
        }
        Ok(())
    }
}
impl Store {
    pub fn change_token(&self) -> Result<String> {
        Ok(self.conn()?.query_row(
            "SELECT token FROM change_state WHERE singleton=1",
            [],
            |r| r.get(0),
        )?)
    }
    pub fn automatic_backup_status(&self) -> Result<AutomaticBackupStatus> {
        self.automatic
            .as_ref()
            .ok_or(Error::Unavailable)?
            .status(self)
    }
    pub fn run_automatic_backup(&mut self, force: bool) -> Result<AutomaticBackupStatus> {
        self.automatic_backup_at(force, &BackupClock::current())
    }
    pub fn automatic_backup_at(
        &mut self,
        force: bool,
        clock: &BackupClock,
    ) -> Result<AutomaticBackupStatus> {
        let mut automatic = self.automatic.take().ok_or(Error::Unavailable)?;
        let result = automatic.run(self, force, clock);
        match result {
            Err(e) => {
                automatic.error = Some(format!("自动备份未完成：{e}。编辑不受影响，可稍后重试。"))
            }
            Ok(true) => automatic.error = None,
            Ok(false) => {}
        }
        let status = automatic.status(self);
        self.automatic = Some(automatic);
        // A backup error is a visible status, never a failed user write or a reason to block editing.
        status
    }
}
