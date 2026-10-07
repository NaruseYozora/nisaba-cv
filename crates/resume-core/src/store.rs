use crate::{Error, Result, error::require, files, migrations, model::*};
use rusqlite::{Connection, OpenFlags, OptionalExtension, params};
use std::{
    fs::File,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}
#[cfg(test)]
mod storage_fault_tests {
    use super::*;
    #[test]
    fn sqlite_full_rolls_back_revision_content_and_change_marker_then_retries() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.test-output");
        std::fs::create_dir_all(&root).unwrap();
        let temp = tempfile::TempDir::new_in(root).unwrap();
        let mut store = Store::open(temp.path()).unwrap();
        let original = store.profile().unwrap();
        let token = store.change_token().unwrap();
        let pages: i64 = store
            .conn()
            .unwrap()
            .query_row("PRAGMA page_count", [], |r| r.get(0))
            .unwrap();
        store
            .conn()
            .unwrap()
            .pragma_update(None, "max_page_count", pages)
            .unwrap();
        let draft = ProfileDraft {
            summary: "数据容量故障后的内容".repeat(800),
            ..original.content.clone()
        };
        let error = store
            .save_profile(draft.clone(), original.revision)
            .unwrap_err();
        assert!(error.to_string().contains("full"), "{error}");
        assert_eq!(store.profile().unwrap(), original);
        assert_eq!(store.change_token().unwrap(), token);
        store
            .conn()
            .unwrap()
            .pragma_update(None, "max_page_count", 10000)
            .unwrap();
        store
            .save_profile(draft.clone(), original.revision)
            .unwrap();
        drop(store);
        let store = Store::open(temp.path()).unwrap();
        assert_eq!(store.profile().unwrap().content, draft);
        assert_ne!(store.change_token().unwrap(), token);
    }
}
pub struct Store {
    pub(crate) root: PathBuf,
    pub(crate) db: Option<Connection>,
    pub(crate) automatic: Option<crate::automatic_backup::AutomaticBackup>,
    _lock: File,
}
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub profile: Profile,
    pub counts: Counts,
    pub schema_version: i64,
    pub data_directory: String,
}
impl Store {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref().to_path_buf();
        let lock = files::lock(&root)?;
        crate::backup::recover(&root)?;
        let path = root.join("library.sqlite3");
        if path.exists() {
            let read = Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
            migrations::header(&read)?;
        }
        let mut db = Connection::open(&path)?;
        db.pragma_update(None, "foreign_keys", true)?;
        migrations::migrate(&mut db, &root)?;
        configure(&db)?;
        db.execute(
            "INSERT OR IGNORE INTO profile VALUES (1,1,?)",
            [serde_json::to_string(&ProfileDraft::default())?],
        )?;
        validate_connection(&db)?;
        let automatic = Some(crate::automatic_backup::AutomaticBackup::load(&root));
        Ok(Self {
            root,
            db: Some(db),
            automatic,
            _lock: lock,
        })
    }
    pub(crate) fn conn(&self) -> Result<&Connection> {
        self.db.as_ref().ok_or(Error::Unavailable)
    }
    pub(crate) fn conn_mut(&mut self) -> Result<&mut Connection> {
        self.db.as_mut().ok_or(Error::Unavailable)
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn overview(&self) -> Result<Overview> {
        Ok(Overview {
            profile: self.profile()?,
            counts: counts(self.conn()?)?,
            schema_version: migrations::SCHEMA,
            data_directory: self.root.display().to_string(),
        })
    }
    pub fn profile(&self) -> Result<Profile> {
        profile_on(self.conn()?)
    }
    pub fn save_profile(&mut self, content: ProfileDraft, revision: i64) -> Result<Profile> {
        content.validate()?;
        let tx = self.conn_mut()?.transaction()?;
        if let Some(asset) = &content.photo_asset_id {
            require_asset(&tx, asset, "photo")?;
        }
        require_changed(tx.execute(
            "UPDATE profile SET revision=revision+1,content=? WHERE singleton=1 AND revision=?",
            params![serde_json::to_string(&content)?, revision],
        )?)?;
        sync_refs(&tx, "profile", "1", content.photo_asset_id.iter().cloned())?;
        tx.commit()?;
        self.profile()
    }
    pub fn save_item(
        &mut self,
        item_id: Option<&str>,
        revision: Option<i64>,
        draft: ItemDraft,
    ) -> Result<LibraryItem> {
        self.save_item_with_category(item_id, revision, draft, None)
    }
    pub fn save_item_in_category(
        &mut self,
        item_id: Option<&str>,
        revision: Option<i64>,
        category_id: &str,
        draft: ItemDraft,
    ) -> Result<LibraryItem> {
        self.save_item_with_category(item_id, revision, draft, Some(category_id))
    }
    fn save_item_with_category(
        &mut self,
        item_id: Option<&str>,
        revision: Option<i64>,
        draft: ItemDraft,
        category_id: Option<&str>,
    ) -> Result<LibraryItem> {
        draft.validate()?;
        require(
            item_id.is_some() == revision.is_some(),
            "更新素材需要 ID 与修订号",
        )?;
        let item_id = item_id.map(str::to_owned).unwrap_or_else(id);
        let tx = self.conn_mut()?.transaction()?;
        let category_id = match category_id {
            Some(category_id) => category_id.to_owned(),
            None if revision.is_some() => item_on(&tx, &item_id)?.category_id,
            None => crate::catalog::legacy_category(&tx, &draft.content)?,
        };
        require(
            crate::catalog::category_on(&tx, &category_id)?.kind == draft.content.kind(),
            "素材与类别结构类型不一致",
        )?;
        if let Some(revision) = revision {
            let old = item_on(&tx, &item_id)?;
            require(
                old.state != RecordState::Trashed,
                "回收站素材不能编辑，请先恢复",
            )?;
            require(
                old.content.kind() == draft.content.kind(),
                "不能修改素材类别",
            )?;
            require_changed(tx.execute("UPDATE library_items SET revision=revision+1,title=?,content=?,tags=?,notes=?,updated_at=? WHERE id=? AND revision=?",params![draft.content.title(),serde_json::to_string(&draft.content)?,serde_json::to_string(&draft.tags)?,draft.notes,now() as i64,item_id,revision])?)?;
        } else {
            tx.execute("INSERT INTO library_items(id,revision,kind,title,content,tags,notes,updated_at) VALUES (?,1,?,?,?,?,?,?)",params![item_id,draft.content.kind(),draft.content.title(),serde_json::to_string(&draft.content)?,serde_json::to_string(&draft.tags)?,draft.notes,now() as i64])?;
        }
        for a in &draft.achievements {
            if let Some(a_id) = &a.id {
                let parent: Option<String> = tx
                    .query_row(
                        "SELECT item_id FROM achievement_items WHERE id=?",
                        [a_id],
                        |r| r.get(0),
                    )
                    .optional()?;
                require(
                    parent.as_deref() == Some(&item_id)
                        || (parent.is_none() && uuid::Uuid::parse_str(a_id).is_ok()),
                    "成果 ID 不属于该素材或新 ID 格式无效",
                )?;
            }
        }
        tx.execute("DELETE FROM achievement_items WHERE item_id=?", [&item_id])?;
        for (position, a) in draft.achievements.iter().enumerate() {
            tx.execute(
                "INSERT INTO achievement_items VALUES (?,?,?,?)",
                params![
                    a.id.clone().unwrap_or_else(id),
                    item_id,
                    a.text,
                    position as i64
                ],
            )?;
        }
        tx.execute("INSERT INTO item_categories VALUES (?,?) ON CONFLICT(item_id) DO UPDATE SET category_id=excluded.category_id",params![item_id,category_id])?;
        tx.commit()?;
        self.item(&item_id)
    }
    pub fn item(&self, item_id: &str) -> Result<LibraryItem> {
        item_on(self.conn()?, item_id)
    }
    pub fn copy_item(&mut self, item_id: &str, revision: i64) -> Result<LibraryItem> {
        let source = self.item(item_id)?;
        if source.revision != revision {
            return Err(Error::Conflict);
        }
        require(
            source.state != RecordState::Trashed,
            "请先恢复回收站中的素材",
        )?;
        self.save_item_in_category(
            None,
            None,
            &source.category_id,
            ItemDraft {
                content: source.content,
                tags: source.tags,
                notes: source.notes,
                achievements: source
                    .achievements
                    .into_iter()
                    .map(|a| AchievementDraft {
                        id: None,
                        text: a.text,
                    })
                    .collect(),
            },
        )
    }
    pub fn import_profile_photo(&mut self, revision: i64, data: &[u8]) -> Result<Profile> {
        let data = crate::photo::normalize(data)?;
        let mut content = self.profile()?.content;
        let asset_id = id();
        content.photo_asset_id = Some(asset_id.clone());
        let tx = self.conn_mut()?.transaction()?;
        tx.execute(
            "INSERT INTO assets VALUES (?,?,?,?,?,?)",
            params![
                asset_id,
                "photo",
                "image/png",
                data,
                files::hash(&data),
                now() as i64
            ],
        )?;
        require_changed(tx.execute(
            "UPDATE profile SET revision=revision+1,content=? WHERE singleton=1 AND revision=?",
            params![serde_json::to_string(&content)?, revision],
        )?)?;
        sync_refs(&tx, "profile", "1", [asset_id].into_iter())?;
        tx.commit()?;
        self.profile()
    }
    pub fn photo_asset(&self, asset_id: &str) -> Result<(String, Vec<u8>)> {
        require_asset(self.conn()?, asset_id, "photo")?;
        Ok(self
            .conn()?
            .query_row("SELECT mime,data FROM assets WHERE id=?", [asset_id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?)
    }
    pub fn items(&self, state: RecordState, query: &str) -> Result<Vec<LibraryItem>> {
        text(query, 256)?;
        let db = self.conn()?;
        let mut statement =
            db.prepare("SELECT id FROM library_items WHERE state=? ORDER BY updated_at DESC,id")?;
        let ids = statement
            .query_map([state.as_str()], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let query = query.to_lowercase();
        let mut result = Vec::new();
        for id in ids {
            let item = item_on(db, &id)?;
            let haystack = format!(
                "{} {} {} {}",
                serde_json::to_string(&item.content)?,
                item.tags.join(" "),
                item.notes,
                item.achievements
                    .iter()
                    .map(|a| a.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" ")
            )
            .to_lowercase();
            if query.is_empty() || haystack.contains(&query) {
                result.push(item);
            }
        }
        Ok(result)
    }
    pub fn set_item_state(
        &mut self,
        item_id: &str,
        revision: i64,
        state: RecordState,
    ) -> Result<LibraryItem> {
        require_changed(self.conn()?.execute("UPDATE library_items SET state=?,revision=revision+1,updated_at=? WHERE id=? AND revision=?",params![state.as_str(),now() as i64,item_id,revision])?)?;
        self.item(item_id)
    }
    pub fn delete_item_permanently(&mut self, item_id: &str, revision: i64) -> Result<()> {
        let item = self.item(item_id)?;
        require(item.state == RecordState::Trashed, "永久删除前须移入回收站")?;
        require_changed(self.conn()?.execute(
            "DELETE FROM library_items WHERE id=? AND revision=?",
            params![item_id, revision],
        )?)?;
        Ok(())
    }
    pub fn inspect_selection(&self, selection: &Selection) -> Result<Vec<String>> {
        inspect_selection_on(self.conn()?, selection)
    }
    pub fn create_resume(
        &mut self,
        name: &str,
        selection: &Selection,
        style: Style,
    ) -> Result<Resume> {
        self.create_resume_target(name, "", "", selection, style)
    }
    pub fn create_resume_target(
        &mut self,
        name: &str,
        company: &str,
        role: &str,
        selection: &Selection,
        style: Style,
    ) -> Result<Resume> {
        require(!name.trim().is_empty(), "简历名称不能为空")?;
        text(name, 256)?;
        text(company, 256)?;
        text(role, 256)?;
        selection.validate()?;
        style.validate()?;
        let tx = self.conn_mut()?.transaction()?;
        let missing = inspect_selection_on(&tx, selection)?;
        require(
            missing.is_empty(),
            &format!("所选内容不可用：{}", missing.join("、")),
        )?;
        let document = selection_document(&tx, selection, style)?;
        let resume_id = id();
        insert_resume(&tx, &resume_id, name, company, role, &document)?;
        tx.commit()?;
        self.resume(&resume_id)
    }
    pub fn resume(&self, resume_id: &str) -> Result<Resume> {
        resume_on(self.conn()?, resume_id)
    }
    pub fn add_resume_selection(
        &mut self,
        resume_id: &str,
        revision: i64,
        selection: &Selection,
    ) -> Result<Resume> {
        selection.validate()?;
        require(
            selection.profile_fields.is_empty() && selection.custom_field_ids.is_empty(),
            "追加素材不替换简历个人档案",
        )?;
        let tx = self.conn_mut()?.transaction()?;
        let mut current = resume_on(&tx, resume_id)?;
        if current.revision != revision {
            return Err(Error::Conflict);
        }
        require(
            current.state != RecordState::Trashed,
            "请先恢复回收站中的简历",
        )?;
        require(
            selection.selected_items().iter().all(|s| {
                !current
                    .document
                    .blocks
                    .iter()
                    .any(|b| b.source.item_id == s.item_id)
            }),
            "已有该来源素材，请先移除旧项再重新选入；旧项的简历内修改不会自动合并",
        )?;
        let missing = inspect_selection_on(&tx, selection)?;
        require(
            missing.is_empty(),
            &format!("所选内容不可用：{}", missing.join("、")),
        )?;
        let addition = selection_document(&tx, selection, current.document.style.clone())?;
        if current.document.format_version == DOCUMENT_VERSION {
            require(
                addition.format_version == DOCUMENT_VERSION,
                "分组简历须使用分组选材追加",
            )?;
            crate::grouping::append_document(&mut current.document, addition)?;
        } else {
            require(
                addition.format_version == LEGACY_DOCUMENT_VERSION,
                "请先将旧简历转换为分组格式",
            )?;
            current.document.blocks.extend(addition.blocks);
        }
        current.document.validate()?;
        require_changed(tx.execute("UPDATE resumes SET revision=revision+1,document=?,updated_at=? WHERE id=? AND revision=?",params![serde_json::to_string(&current.document)?,now() as i64,resume_id,revision])?)?;
        tx.commit()?;
        self.resume(resume_id)
    }
    pub fn resume_block_as_item(
        &mut self,
        resume_id: &str,
        revision: i64,
        block_id: &str,
    ) -> Result<LibraryItem> {
        self.resume_block_as_item_target(resume_id, revision, block_id, None)
    }
    pub fn resume_block_as_item_in_category(
        &mut self,
        resume_id: &str,
        revision: i64,
        block_id: &str,
        category_id: &str,
    ) -> Result<LibraryItem> {
        self.resume_block_as_item_target(resume_id, revision, block_id, Some(category_id))
    }
    fn resume_block_as_item_target(
        &mut self,
        resume_id: &str,
        revision: i64,
        block_id: &str,
        category_id: Option<&str>,
    ) -> Result<LibraryItem> {
        let current = self.resume(resume_id)?;
        if current.revision != revision {
            return Err(Error::Conflict);
        }
        let block = current
            .document
            .blocks
            .iter()
            .find(|b| b.id == block_id)
            .ok_or_else(|| Error::Missing(block_id.into()))?;
        let category_id = category_id.or_else(|| {
            current
                .document
                .sections
                .iter()
                .find(|section| section.block_ids.iter().any(|id| id == block_id))
                .map(|section| section.category_id.as_str())
        });
        self.save_item_with_category(
            None,
            None,
            ItemDraft {
                content: block.content.clone(),
                tags: vec![],
                notes: String::new(),
                achievements: block
                    .achievements
                    .iter()
                    .map(|a| AchievementDraft {
                        id: None,
                        text: a.text.clone(),
                    })
                    .filter(|a| !a.text.trim().is_empty())
                    .collect(),
            },
            category_id,
        )
    }
    pub fn resumes(&self, state: RecordState) -> Result<Vec<Resume>> {
        let db = self.conn()?;
        let mut s =
            db.prepare("SELECT id FROM resumes WHERE state=? ORDER BY updated_at DESC,id")?;
        let ids = s
            .query_map([state.as_str()], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.iter().map(|id| resume_on(db, id)).collect()
    }
    pub fn save_resume(
        &mut self,
        resume_id: &str,
        revision: i64,
        draft: ResumeDraft,
    ) -> Result<Resume> {
        draft.validate()?;
        let tx = self.conn_mut()?.transaction()?;
        require(
            resume_on(&tx, resume_id)?.document.format_version == draft.document.format_version,
            "简历格式改变须使用分组转换接口，不能覆盖或降级",
        )?;
        validate_document_assets(&tx, &draft.document)?;
        require(
            resume_on(&tx, resume_id)?.state != RecordState::Trashed,
            "请先恢复回收站中的简历",
        )?;
        require_changed(tx.execute("UPDATE resumes SET revision=revision+1,name=?,company=?,role=?,document=?,updated_at=? WHERE id=? AND revision=?",params![draft.name,draft.company,draft.role,serde_json::to_string(&draft.document)?,now() as i64,resume_id,revision])?)?;
        sync_refs(
            &tx,
            "resume",
            resume_id,
            draft.document.profile.photo_asset_id.iter().cloned(),
        )?;
        tx.commit()?;
        self.resume(resume_id)
    }
    pub fn copy_resume(&mut self, resume_id: &str, name: &str) -> Result<Resume> {
        self.copy_resume_checked(resume_id, None, name)
    }
    pub fn copy_resume_checked(
        &mut self,
        resume_id: &str,
        revision: Option<i64>,
        name: &str,
    ) -> Result<Resume> {
        text(name, 256)?;
        require(!name.trim().is_empty(), "简历名称不能为空")?;
        let tx = self.conn_mut()?.transaction()?;
        let source = resume_on(&tx, resume_id)?;
        if revision.is_some_and(|r| r != source.revision) {
            return Err(Error::Conflict);
        }
        require(
            source.state != RecordState::Trashed,
            "请先恢复回收站中的简历",
        )?;
        let new_id = id();
        insert_resume(
            &tx,
            &new_id,
            name,
            &source.company,
            &source.role,
            &source.document,
        )?;
        tx.commit()?;
        self.resume(&new_id)
    }
    pub fn set_resume_state(
        &mut self,
        resume_id: &str,
        revision: i64,
        state: RecordState,
    ) -> Result<Resume> {
        require_changed(self.conn()?.execute(
            "UPDATE resumes SET state=?,revision=revision+1,updated_at=? WHERE id=? AND revision=?",
            params![state.as_str(), now() as i64, resume_id, revision],
        )?)?;
        self.resume(resume_id)
    }
    pub fn delete_resume_permanently(&mut self, resume_id: &str, revision: i64) -> Result<()> {
        let tx = self.conn_mut()?.transaction()?;
        let resume = resume_on(&tx, resume_id)?;
        require(
            resume.state == RecordState::Trashed,
            "永久删除前须移入回收站",
        )?;
        require(resume.revision == revision, "简历已更新")?;
        tx.execute("DELETE FROM asset_refs WHERE (owner_kind='resume' AND owner_id=?) OR (owner_kind='snapshot' AND owner_id IN (SELECT id FROM snapshots WHERE resume_id=?))",params![resume_id,resume_id])?;
        require_changed(tx.execute(
            "DELETE FROM resumes WHERE id=? AND revision=?",
            params![resume_id, revision],
        )?)?;
        tx.commit()?;
        Ok(())
    }
    pub fn create_snapshot(
        &mut self,
        resume_id: &str,
        revision: i64,
        name: &str,
        pdf_asset_id: Option<&str>,
    ) -> Result<Snapshot> {
        text(name, 256)?;
        require(!name.trim().is_empty(), "快照名称不能为空")?;
        let tx = self.conn_mut()?.transaction()?;
        let resume = resume_on(&tx, resume_id)?;
        if resume.revision != revision {
            return Err(Error::Conflict);
        }
        require(
            resume.state != RecordState::Trashed,
            "回收站简历不能创建快照",
        )?;
        if let Some(asset) = pdf_asset_id {
            require_asset(&tx, asset, "pdf")?;
        }
        let snapshot_id = id();
        tx.execute(
            "INSERT INTO snapshots(id,resume_id,name,document,pdf_asset_id,created_at,company,role,source_revision) VALUES (?,?,?,?,?,?,?,?,?)",
            params![
                snapshot_id,
                resume_id,
                name,
                serde_json::to_string(&resume.document)?,
                pdf_asset_id,
                now() as i64,
                resume.company,
                resume.role,
                resume.revision
            ],
        )?;
        sync_refs(
            &tx,
            "snapshot",
            &snapshot_id,
            resume
                .document
                .profile
                .photo_asset_id
                .iter()
                .cloned()
                .chain(pdf_asset_id.map(str::to_owned)),
        )?;
        tx.commit()?;
        self.snapshot(&snapshot_id)
    }
    pub fn snapshot(&self, snapshot_id: &str) -> Result<Snapshot> {
        snapshot_on(self.conn()?, snapshot_id)
    }
    pub fn snapshots(&self, resume_id: &str) -> Result<Vec<Snapshot>> {
        self.resume(resume_id)?;
        let mut stmt = self.conn()?.prepare(
            "SELECT id FROM snapshots WHERE resume_id=? ORDER BY created_at DESC,id DESC",
        )?;
        let ids = stmt
            .query_map([resume_id], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.iter().map(|id| self.snapshot(id)).collect()
    }
    pub fn capture_export(&self, resume_id: &str, revision: i64) -> Result<Resume> {
        let resume = self.resume(resume_id)?;
        if resume.revision != revision {
            return Err(Error::Conflict);
        }
        require(
            resume.state != RecordState::Trashed,
            "请先恢复回收站中的简历",
        )?;
        resume.document.validate()?;
        validate_document_assets(self.conn()?, &resume.document)?;
        Ok(resume)
    }
    /// Commit the frozen document and actual PDF together, even if the draft has since changed.
    pub fn record_export(&mut self, frozen: &Resume, pdf: &[u8]) -> Result<Snapshot> {
        ResumeDraft {
            name: frozen.name.clone(),
            company: frozen.company.clone(),
            role: frozen.role.clone(),
            document: frozen.document.clone(),
        }
        .validate()?;
        validate_asset("pdf", "application/pdf", pdf)?;
        require(
            pdf.len() >= 12
                && pdf[pdf.len().saturating_sub(1024)..]
                    .windows(5)
                    .any(|s| s == b"%%EOF"),
            "PDF 未完整生成",
        )?;
        let tx = self.conn_mut()?.transaction()?;
        require(
            resume_on(&tx, &frozen.id)?.state != RecordState::Trashed,
            "简历已进入回收站，导出未提交",
        )?;
        validate_document_assets(&tx, &frozen.document)?;
        let asset_id = id();
        let snapshot_id = id();
        let created = now() as i64;
        tx.execute(
            "INSERT INTO assets VALUES (?,?,?,?,?,?)",
            params![
                asset_id,
                "pdf",
                "application/pdf",
                pdf,
                files::hash(pdf),
                created
            ],
        )?;
        let name = format!(
            "PDF · {}",
            frozen.name.chars().take(230).collect::<String>()
        );
        tx.execute("INSERT INTO snapshots(id,resume_id,name,document,pdf_asset_id,created_at,company,role,source_revision) VALUES (?,?,?,?,?,?,?,?,?)",params![snapshot_id,frozen.id,name,serde_json::to_string(&frozen.document)?,asset_id,created,frozen.company,frozen.role,frozen.revision])?;
        sync_refs(
            &tx,
            "snapshot",
            &snapshot_id,
            frozen
                .document
                .profile
                .photo_asset_id
                .iter()
                .cloned()
                .chain(Some(asset_id)),
        )?;
        tx.commit()?;
        self.snapshot(&snapshot_id)
    }
    pub fn snapshot_pdf(&self, snapshot_id: &str) -> Result<Vec<u8>> {
        let snapshot = self.snapshot(snapshot_id)?;
        let asset = snapshot
            .pdf_asset_id
            .ok_or_else(|| Error::Invalid("这份快照没有导出 PDF".into()))?;
        require_asset(self.conn()?, &asset, "pdf")?;
        self.asset(&asset)
    }
    pub fn restore_snapshot_as_resume(&mut self, snapshot_id: &str, name: &str) -> Result<Resume> {
        text(name, 256)?;
        require(!name.trim().is_empty(), "简历名称不能为空")?;
        let tx = self.conn_mut()?.transaction()?;
        let snapshot = snapshot_on(&tx, snapshot_id)?;
        let new_id = id();
        insert_resume(
            &tx,
            &new_id,
            name,
            &snapshot.company,
            &snapshot.role,
            &snapshot.document,
        )?;
        tx.commit()?;
        self.resume(&new_id)
    }
    pub fn save_preset(
        &mut self,
        preset_id: Option<&str>,
        revision: Option<i64>,
        draft: PresetDraft,
    ) -> Result<Preset> {
        self.save_preset_kind(preset_id, revision, PresetKind::Selection, draft)
    }
    pub fn save_preset_kind(
        &mut self,
        preset_id: Option<&str>,
        revision: Option<i64>,
        kind: PresetKind,
        draft: PresetDraft,
    ) -> Result<Preset> {
        draft.validate()?;
        if kind == PresetKind::Layout {
            require(draft.selection.is_empty(), "版式预设不能包含选材")?;
        }
        require(
            preset_id.is_some() == revision.is_some(),
            "更新预设需要 ID 与修订号",
        )?;
        let preset_id = preset_id.map(str::to_owned).unwrap_or_else(id);
        if let Some(revision) = revision {
            require_changed(self.conn()?.execute(
                "UPDATE presets SET revision=revision+1,content=? WHERE id=? AND revision=? AND kind=?",
                params![serde_json::to_string(&draft)?, preset_id, revision, kind.as_str()],
            )?)?;
        } else {
            self.conn()?.execute(
                "INSERT INTO presets(id,revision,content,kind) VALUES (?,1,?,?)",
                params![preset_id, serde_json::to_string(&draft)?, kind.as_str()],
            )?;
        }
        self.preset(&preset_id)
    }
    pub fn preset(&self, preset_id: &str) -> Result<Preset> {
        let (revision, content, kind): (i64, String, String) = self
            .conn()?
            .query_row(
                "SELECT revision,content,kind FROM presets WHERE id=?",
                [preset_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?
            .ok_or_else(|| Error::Missing(preset_id.into()))?;
        Ok(Preset {
            id: preset_id.into(),
            revision,
            kind: match kind.as_str() {
                "selection" => PresetKind::Selection,
                "layout" => PresetKind::Layout,
                _ => return Err(Error::Invalid("预设类别无效".into())),
            },
            content: serde_json::from_str(&content)?,
        })
    }
    pub fn presets(&self, kind: PresetKind) -> Result<Vec<Preset>> {
        let mut statement = self
            .conn()?
            .prepare("SELECT id FROM presets WHERE kind=? ORDER BY id")?;
        let ids = statement
            .query_map([kind.as_str()], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        ids.iter().map(|id| self.preset(id)).collect()
    }
    pub fn delete_preset(&mut self, preset_id: &str, revision: i64) -> Result<()> {
        require_changed(self.conn()?.execute(
            "DELETE FROM presets WHERE id=? AND revision=?",
            params![preset_id, revision],
        )?)?;
        Ok(())
    }
    pub fn import_asset(&mut self, kind: &str, mime: &str, data: &[u8]) -> Result<String> {
        validate_asset(kind, mime, data)?;
        let asset_id = id();
        self.conn()?.execute(
            "INSERT INTO assets VALUES (?,?,?,?,?,?)",
            params![asset_id, kind, mime, data, files::hash(data), now() as i64],
        )?;
        Ok(asset_id)
    }
    pub fn asset(&self, asset_id: &str) -> Result<Vec<u8>> {
        self.conn()?
            .query_row("SELECT data FROM assets WHERE id=?", [asset_id], |r| {
                r.get(0)
            })
            .optional()?
            .ok_or_else(|| Error::Missing(asset_id.into()))
    }
    pub fn collect_unreferenced_assets(&mut self) -> Result<usize> {
        Ok(self.conn()?.execute(
            "DELETE FROM assets WHERE id NOT IN (SELECT asset_id FROM asset_refs)",
            [],
        )?)
    }
    pub fn backup_settings(&self) -> Result<(i64, BackupSettings)> {
        let value: Option<(i64, String)> = self
            .conn()?
            .query_row(
                "SELECT revision,content FROM app_settings WHERE key='backup'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        match value {
            Some((revision, content)) => Ok((revision, serde_json::from_str(&content)?)),
            None => Ok((0, BackupSettings::default())),
        }
    }
    pub fn save_backup_settings(
        &mut self,
        revision: i64,
        settings: BackupSettings,
    ) -> Result<(i64, BackupSettings)> {
        settings.validate()?;
        if revision == 0 {
            let changed = self.conn()?.execute(
                "INSERT OR IGNORE INTO app_settings VALUES ('backup',1,?)",
                [serde_json::to_string(&settings)?],
            )?;
            require_changed(changed)?;
        } else {
            require_changed(self.conn()?.execute("UPDATE app_settings SET revision=revision+1,content=? WHERE key='backup' AND revision=?",params![serde_json::to_string(&settings)?,revision])?)?;
        }
        self.backup_settings()
    }
    pub fn flush(&self) -> Result<()> {
        self.conn()?
            .execute_batch("PRAGMA wal_checkpoint(PASSIVE);")?;
        Ok(())
    }
}
pub(crate) fn configure(db: &Connection) -> Result<()> {
    db.busy_timeout(Duration::from_secs(5))?;
    db.execute_batch("PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL; PRAGMA trusted_schema=OFF;")?;
    Ok(())
}
pub(crate) fn require_changed(count: usize) -> Result<()> {
    if count == 1 {
        Ok(())
    } else {
        Err(Error::Conflict)
    }
}
fn state(value: &str) -> Result<RecordState> {
    match value {
        "active" => Ok(RecordState::Active),
        "archived" => Ok(RecordState::Archived),
        "trashed" => Ok(RecordState::Trashed),
        _ => Err(Error::Invalid("记录状态无效".into())),
    }
}
pub(crate) fn profile_on(db: &Connection) -> Result<Profile> {
    let (revision, content): (i64, String) = db.query_row(
        "SELECT revision,content FROM profile WHERE singleton=1",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(Profile {
        revision,
        content: serde_json::from_str(&content)?,
    })
}
pub(crate) fn item_on(db: &Connection, item_id: &str) -> Result<LibraryItem> {
    let (revision, content, tags, notes, state_str, updated_at): (
        i64,
        String,
        String,
        String,
        String,
        u64,
    ) = db
        .query_row(
            "SELECT revision,content,tags,notes,state,updated_at FROM library_items WHERE id=?",
            [item_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    get_u64(r, 5)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| Error::Missing(item_id.into()))?;
    let mut s = db.prepare(
        "SELECT id,text,position FROM achievement_items WHERE item_id=? ORDER BY position",
    )?;
    let achievements = s
        .query_map([item_id], |r| {
            Ok(Achievement {
                id: r.get(0)?,
                text: r.get(1)?,
                position: r.get::<_, u32>(2)? as usize,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(LibraryItem {
        id: item_id.into(),
        category_id: db.query_row(
            "SELECT category_id FROM item_categories WHERE item_id=?",
            [item_id],
            |r| r.get(0),
        )?,
        revision,
        state: state(&state_str)?,
        content: serde_json::from_str(&content)?,
        tags: serde_json::from_str(&tags)?,
        notes,
        achievements,
        updated_at,
    })
}
pub(crate) fn resume_on(db: &Connection, resume_id: &str) -> Result<Resume> {
    let (revision, name, company, role, document, state_str, updated_at): (
        i64,
        String,
        String,
        String,
        String,
        String,
        u64,
    ) = db
        .query_row(
            "SELECT revision,name,company,role,document,state,updated_at FROM resumes WHERE id=?",
            [resume_id],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    get_u64(r, 6)?,
                ))
            },
        )
        .optional()?
        .ok_or_else(|| Error::Missing(resume_id.into()))?;
    Ok(Resume {
        id: resume_id.into(),
        revision,
        name,
        company,
        role,
        state: state(&state_str)?,
        document: serde_json::from_str(&document)?,
        updated_at,
    })
}
fn snapshot_on(db: &Connection, snapshot_id: &str) -> Result<Snapshot> {
    let (resume_id, name, document, pdf_asset_id, created_at, company, role, source_revision): (
        String,
        String,
        String,
        Option<String>,
        u64,
        String,
        String,
        i64,
    ) = db
        .query_row(
            "SELECT resume_id,name,document,pdf_asset_id,created_at,company,role,source_revision FROM snapshots WHERE id=?",
            [snapshot_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, get_u64(r, 4)?,r.get(5)?,r.get(6)?,r.get(7)?)),
        )
        .optional()?
        .ok_or_else(|| Error::Missing(snapshot_id.into()))?;
    Ok(Snapshot {
        id: snapshot_id.into(),
        resume_id,
        name,
        document: serde_json::from_str(&document)?,
        pdf_asset_id,
        created_at,
        company,
        role,
        source_revision,
    })
}
fn insert_resume(
    db: &Connection,
    resume_id: &str,
    name: &str,
    company: &str,
    role: &str,
    document: &ResumeDocument,
) -> Result<()> {
    validate_document_assets(db, document)?;
    db.execute("INSERT INTO resumes(id,revision,name,company,role,document,updated_at) VALUES (?,1,?,?,?,?,?)",params![resume_id,name,company,role,serde_json::to_string(document)?,now() as i64])?;
    sync_refs(
        db,
        "resume",
        resume_id,
        document.profile.photo_asset_id.iter().cloned(),
    )
}
fn require_asset(db: &Connection, asset_id: &str, kind: &str) -> Result<()> {
    let actual: Option<String> = db
        .query_row("SELECT kind FROM assets WHERE id=?", [asset_id], |r| {
            r.get(0)
        })
        .optional()?;
    require(actual.as_deref() == Some(kind), "资源不存在或类型不符")
}
fn validate_document_assets(db: &Connection, document: &ResumeDocument) -> Result<()> {
    if let Some(asset) = &document.profile.photo_asset_id {
        require_asset(db, asset, "photo")?;
    }
    Ok(())
}
pub(crate) fn sync_refs(
    db: &Connection,
    kind: &str,
    owner: &str,
    assets: impl Iterator<Item = String>,
) -> Result<()> {
    db.execute(
        "DELETE FROM asset_refs WHERE owner_kind=? AND owner_id=?",
        params![kind, owner],
    )?;
    for asset in assets {
        db.execute(
            "INSERT OR IGNORE INTO asset_refs VALUES (?,?,?)",
            params![kind, owner, asset],
        )?;
    }
    Ok(())
}
pub(crate) fn inspect_selection_on(db: &Connection, selection: &Selection) -> Result<Vec<String>> {
    selection.validate()?;
    let mut missing = Vec::new();
    let profile = profile_on(db)?;
    for field_id in &selection.custom_field_ids {
        if !profile
            .content
            .custom_fields
            .iter()
            .any(|f| &f.id == field_id)
        {
            missing.push(format!("个人信息 {field_id}"));
        }
    }
    if let Some(sections) = &selection.sections {
        for section in sections {
            match crate::catalog::category_on(db, &section.category_id) {
                Ok(_) => {}
                Err(Error::Missing(_)) => missing.push(format!("类别 {}", section.category_id)),
                Err(e) => return Err(e),
            }
            for selected in &section.items {
                if let Ok(item) = item_on(db, &selected.item_id)
                    && item.category_id != section.category_id
                {
                    missing.push(format!("{}（类别已改变）", item.content.title()));
                }
            }
        }
    }
    for selected in selection.selected_items() {
        match item_on(db, &selected.item_id) {
            Ok(item) => {
                if item.state != RecordState::Active {
                    missing.push(format!("{}（已归档或删除）", item.content.title()));
                }
                for achievement in &selected.achievement_ids {
                    if !item.achievements.iter().any(|a| &a.id == achievement) {
                        missing.push(format!("成果 {achievement}"));
                    }
                }
            }
            Err(Error::Missing(_)) => missing.push(format!("素材 {}", selected.item_id)),
            Err(error) => return Err(error),
        }
    }
    Ok(missing)
}
pub(crate) fn counts(db: &Connection) -> Result<Counts> {
    fn count(db: &Connection, table: &str) -> Result<usize> {
        Ok(
            db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| {
                r.get::<_, i64>(0)
            })? as usize,
        )
    }
    Ok(Counts {
        items: count(db, "library_items")?,
        achievements: count(db, "achievement_items")?,
        resumes: count(db, "resumes")?,
        snapshots: count(db, "snapshots")?,
        assets: count(db, "assets")?,
        presets: count(db, "presets")?,
    })
}
fn get_u64(row: &rusqlite::Row<'_>, index: usize) -> rusqlite::Result<u64> {
    u64::try_from(row.get::<_, i64>(index)?).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            index,
            rusqlite::types::Type::Integer,
            Box::new(e),
        )
    })
}
pub(crate) fn validate_connection(db: &Connection) -> Result<()> {
    migrations::validate_history(db)?;
    let token: String = db.query_row(
        "SELECT token FROM change_state WHERE singleton=1",
        [],
        |r| r.get(0),
    )?;
    require(
        token.len() == 32 && token.bytes().all(|b| b.is_ascii_hexdigit()),
        "变更标记无效",
    )?;
    require(
        db.query_row("PRAGMA integrity_check", [], |r| r.get::<_, String>(0))? == "ok",
        "数据库完整性检查失败",
    )?;
    require(
        !db.prepare("PRAGMA foreign_key_check")?.exists([])?,
        "数据库资源引用无效",
    )?;
    profile_on(db)?.content.validate()?;
    crate::catalog::validate_catalog(db)?;
    for table in ["library_items", "resumes", "snapshots", "presets"] {
        let mut stmt = db.prepare(&format!("SELECT id FROM {table}"))?;
        let ids = stmt
            .query_map([], |r| r.get::<_, String>(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        for id in ids {
            match table {
                "library_items" => {
                    let item = item_on(db, &id)?;
                    require(
                        crate::catalog::category_on(db, &item.category_id)?.kind
                            == item.content.kind(),
                        "素材内容与类别结构不一致",
                    )?;
                    let draft = ItemDraft {
                        content: item.content,
                        tags: item.tags,
                        notes: item.notes,
                        achievements: item
                            .achievements
                            .iter()
                            .map(|a| AchievementDraft {
                                id: Some(a.id.clone()),
                                text: a.text.clone(),
                            })
                            .collect(),
                    };
                    draft.validate()?;
                }
                "resumes" => {
                    let r = resume_on(db, &id)?;
                    ResumeDraft {
                        name: r.name,
                        company: r.company,
                        role: r.role,
                        document: r.document,
                    }
                    .validate()?;
                }
                "snapshots" => {
                    let snapshot = snapshot_on(db, &id)?;
                    text(&snapshot.name, 256)?;
                    text(&snapshot.company, 256)?;
                    text(&snapshot.role, 256)?;
                    require(snapshot.source_revision >= 0, "快照来源修订无效")?;
                    snapshot.document.validate()?;
                }
                _ => {
                    let (content, kind): (String, String) =
                        db.query_row("SELECT content,kind FROM presets WHERE id=?", [&id], |r| {
                            Ok((r.get(0)?, r.get(1)?))
                        })?;
                    let draft = serde_json::from_str::<PresetDraft>(&content)?;
                    draft.validate()?;
                    require(
                        kind == "selection" || (kind == "layout" && draft.selection.is_empty()),
                        "预设类别或选材无效",
                    )?;
                }
            }
        }
    }
    // Verify the reference index against documents, including archived and trashed owners.
    let mut expected = std::collections::BTreeSet::new();
    if let Some(asset) = profile_on(db)?.content.photo_asset_id {
        require_asset(db, &asset, "photo")?;
        expected.insert(("profile".to_owned(), "1".to_owned(), asset));
    }
    for table in ["resumes", "snapshots"] {
        let mut s = db.prepare(&format!("SELECT id FROM {table}"))?;
        for id in s.query_map([], |r| r.get::<_, String>(0))? {
            let id = id?;
            if table == "resumes" {
                let r = resume_on(db, &id)?;
                if let Some(a) = r.document.profile.photo_asset_id {
                    require_asset(db, &a, "photo")?;
                    expected.insert(("resume".into(), id, a));
                }
            } else {
                let s = snapshot_on(db, &id)?;
                if let Some(a) = s.document.profile.photo_asset_id {
                    require_asset(db, &a, "photo")?;
                    expected.insert(("snapshot".into(), id.clone(), a));
                }
                if let Some(a) = s.pdf_asset_id {
                    require_asset(db, &a, "pdf")?;
                    expected.insert(("snapshot".into(), id, a));
                }
            }
        }
    }
    let mut refs = db.prepare("SELECT owner_kind,owner_id,asset_id FROM asset_refs")?;
    let actual = refs
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<std::result::Result<std::collections::BTreeSet<_>, _>>()?;
    require(actual == expected, "资源引用索引与资料不一致")?;
    let mut assets = db.prepare("SELECT data,checksum,kind,mime FROM assets")?;
    for row in assets.query_map([], |r| {
        Ok((
            r.get::<_, Vec<u8>>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
        ))
    })? {
        let (data, checksum, kind, mime) = row?;
        require(files::hash(&data) == checksum, "资源校验失败")?;
        validate_asset(&kind, &mime, &data)?;
    }
    let mut settings = db.prepare("SELECT key,content FROM app_settings")?;
    for row in settings.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))? {
        let (key, content) = row?;
        require(key == "backup", "设置项格式不支持")?;
        serde_json::from_str::<BackupSettings>(&content)?.validate()?;
    }
    Ok(())
}
fn validate_asset(kind: &str, mime: &str, data: &[u8]) -> Result<()> {
    require(
        !data.is_empty() && data.len() <= 32 * 1024 * 1024,
        "资源必须在 32MB 以内",
    )?;
    let valid = match (kind, mime) {
        ("photo", "image/png") => data.starts_with(b"\x89PNG\r\n\x1a\n"),
        ("photo", "image/jpeg") => data.starts_with(&[0xff, 0xd8, 0xff]),
        ("pdf", "application/pdf") => data.starts_with(b"%PDF-"),
        _ => false,
    };
    require(valid, "资源类型或格式无效")
}

fn selection_document(
    db: &Connection,
    selection: &Selection,
    style: Style,
) -> Result<ResumeDocument> {
    let source_profile = profile_on(db)?;
    let mut profile = ProfileDraft::default();
    for field in &selection.profile_fields {
        match field {
            ProfileField::Name => profile.name = source_profile.content.name.clone(),
            ProfileField::Title => profile.title = source_profile.content.title.clone(),
            ProfileField::Phone => profile.phone = source_profile.content.phone.clone(),
            ProfileField::Email => profile.email = source_profile.content.email.clone(),
            ProfileField::Location => profile.location = source_profile.content.location.clone(),
            ProfileField::Summary => profile.summary = source_profile.content.summary.clone(),
            ProfileField::Links => profile.links = source_profile.content.links.clone(),
            ProfileField::Photo => {
                profile.photo_asset_id = source_profile.content.photo_asset_id.clone()
            }
        }
    }
    let mut blocks = Vec::new();
    for field_id in &selection.custom_field_ids {
        let field = source_profile
            .content
            .custom_fields
            .iter()
            .find(|f| &f.id == field_id)
            .ok_or_else(|| Error::Missing(field_id.clone()))?;
        profile.custom_fields.push(field.clone());
    }
    for selection_item in selection.selected_items() {
        let source = item_on(db, &selection_item.item_id)?;
        let mut content = source.content;
        if !selection_item.include_background
            && let ItemContent::Project { background, .. } = &mut content
        {
            background.clear();
        }
        let mut achievements = Vec::new();
        for achievement_id in &selection_item.achievement_ids {
            let a = source
                .achievements
                .iter()
                .find(|a| &a.id == achievement_id)
                .ok_or_else(|| Error::Missing(achievement_id.clone()))?;
            achievements.push(ResumeAchievement {
                id: id(),
                source_id: a.id.clone(),
                text: a.text.clone(),
            });
        }
        blocks.push(ResumeBlock {
            id: id(),
            source: Source {
                item_id: source.id,
                revision: source.revision,
            },
            content,
            achievements,
            visible: true,
            page_break_before: false,
        });
    }
    let mut document = ResumeDocument {
        format_version: if selection.sections.is_some() {
            DOCUMENT_VERSION
        } else {
            LEGACY_DOCUMENT_VERSION
        },
        profile,
        profile_source_revision: source_profile.revision,
        blocks,
        style,
        sections: vec![],
    };
    if let Some(sections) = &selection.sections {
        let mut offset = 0;
        for selected in sections {
            let category = crate::catalog::category_on(db, &selected.category_id)?;
            // A renamed category is a user heading, not a generic layout label.
            let title = if category.builtin
                && crate::catalog::BUILTINS
                    .iter()
                    .any(|(kind, name)| *kind == category.kind && *name == category.name)
            {
                document
                    .style
                    .module_titles
                    .get(&category.kind)
                    .filter(|s| !s.trim().is_empty())
                    .cloned()
                    .unwrap_or(category.name)
            } else {
                category.name
            };
            let block_ids = document.blocks[offset..offset + selected.items.len()]
                .iter()
                .map(|b| b.id.clone())
                .collect();
            offset += selected.items.len();
            document.sections.push(ResumeSection {
                id: id(),
                category_id: category.id,
                title,
                kind: category.kind,
                block_ids,
            });
        }
    }
    document.validate()?;
    Ok(document)
}
