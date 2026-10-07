use resume_core::{
    Error, Result, Store, actor::Actor, backup::RestoreFault, files, migrations, model::*,
};
use rusqlite::{Connection, params};
use std::io::{Read, Write};
use std::{fs, path::Path, process::Command, sync::mpsc, time::Duration};
use tempfile::TempDir;
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn temp() -> TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.test-output");
    fs::create_dir_all(&root).unwrap();
    TempDir::new_in(root).unwrap()
}
fn work() -> ItemDraft {
    ItemDraft {
        content: ItemContent::Work {
            company: "示例公司".into(),
            role: "工程师".into(),
            department: "平台".into(),
            location: "上海".into(),
            dates: DateRange {
                start: Some(PartialDate {
                    year: 2021,
                    month: None,
                }),
                end: None,
                ongoing: true,
            },
        },
        tags: vec!["后端".into()],
        notes: "内部备注不得进入简历".into(),
        achievements: vec![
            AchievementDraft {
                id: None,
                text: "把接口延迟降低 30%".into(),
            },
            AchievementDraft {
                id: None,
                text: "为团队建立知识库".into(),
            },
        ],
    }
}
fn selected(item: &LibraryItem) -> Selection {
    Selection {
        custom_field_ids: vec![],
        sections: None,
        profile_fields: vec![ProfileField::Name, ProfileField::Photo],
        items: vec![SelectionItem {
            item_id: item.id.clone(),
            achievement_ids: vec![item.achievements[0].id.clone()],
            include_background: false,
        }],
    }
}
fn update_name(store: &mut Store, name: &str) -> Result<()> {
    let profile = store.profile()?;
    let mut draft = profile.content;
    draft.name = name.into();
    store.save_profile(draft, profile.revision)?;
    Ok(())
}
fn real_pdf() -> Vec<u8> {
    include_bytes!("fixtures/anonymous.pdf").to_vec()
}
fn photo() -> Vec<u8> {
    include_bytes!("fixtures/photo.png").to_vec()
}
fn complete_fixture(store: &mut Store) -> Result<(String, String, String)> {
    let photo = store.import_asset("photo", "image/png", &photo())?;
    let pdf = store.import_asset("pdf", "application/pdf", &real_pdf())?;
    let p = store.profile()?;
    let mut content = p.content;
    content.name = "匿名用户".into();
    content.email = "test@example.com".into();
    content.photo_asset_id = Some(photo);
    store.save_profile(content, p.revision)?;
    let item = store.save_item(None, None, work())?;
    let selection = selected(&item);
    let resume = store.create_resume("后端岗位", &selection, Style::default())?;
    let snapshot = store.create_snapshot(&resume.id, resume.revision, "投递前", Some(&pdf))?;
    store.save_preset(
        None,
        None,
        PresetDraft {
            name: "后端选材".into(),
            selection,
            style: Style::default(),
        },
    )?;
    store.save_backup_settings(
        0,
        BackupSettings {
            directory: Some("D:\\备份".into()),
            ..Default::default()
        },
    )?;
    Ok((item.id, resume.id, snapshot.id))
}

#[test]
fn profile_reopens_and_stale_write_is_rejected() {
    let t = temp();
    {
        let mut s = Store::open(t.path()).unwrap();
        let p = s.profile().unwrap();
        update_name(&mut s, "中文姓名").unwrap();
        assert!(matches!(
            s.save_profile(p.content, p.revision),
            Err(Error::Conflict)
        ));
    }
    let s = Store::open(t.path()).unwrap();
    assert_eq!(s.profile().unwrap().content.name, "中文姓名");
}
#[test]
fn dates_and_typed_items_preserve_precision_and_reject_invalid_input() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let item = s.save_item(None, None, work()).unwrap();
    assert!(matches!(
        item.content,
        ItemContent::Work {
            dates: DateRange {
                start: Some(PartialDate { month: None, .. }),
                ongoing: true,
                ..
            },
            ..
        }
    ));
    assert_eq!(s.items(RecordState::Active, "知识库").unwrap().len(), 1);
    assert!(
        DateRange {
            start: None,
            end: Some(PartialDate {
                year: 2025,
                month: Some(13)
            }),
            ongoing: false
        }
        .validate()
        .is_err()
    );
    assert!(
        DateRange {
            start: None,
            end: Some(PartialDate {
                year: 2025,
                month: None
            }),
            ongoing: true
        }
        .validate()
        .is_err()
    );
    let skill = ItemDraft {
        content: ItemContent::Skill {
            name: "Rust".into(),
            category: "开发".into(),
            description: "熟悉异步开发".into(),
        },
        tags: vec![],
        notes: String::new(),
        achievements: vec![],
    };
    s.save_item(None, None, skill).unwrap();
    for content in [
        ItemContent::Education {
            school: "示例大学".into(),
            degree: "本科".into(),
            major: "计算机".into(),
            dates: DateRange::default(),
        },
        ItemContent::Project {
            name: "示例项目".into(),
            role: "负责人".into(),
            url: None,
            background: "背景".into(),
            dates: DateRange::default(),
        },
        ItemContent::Custom {
            category: "证书".into(),
            title: "示例证书".into(),
            subtitle: "2025".into(),
            dates: DateRange::default(),
        },
    ] {
        s.save_item(
            None,
            None,
            ItemDraft {
                content,
                tags: vec![],
                notes: String::new(),
                achievements: vec![],
            },
        )
        .unwrap();
    }
    assert_eq!(s.overview().unwrap().counts.items, 5);
}
#[test]
fn achievements_keep_identity_on_reorder_and_foreign_id_failure_rolls_back() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let a = s.save_item(None, None, work()).unwrap();
    let b = s.save_item(None, None, work()).unwrap();
    let mut draft = work();
    draft.achievements = a
        .achievements
        .iter()
        .rev()
        .map(|a| AchievementDraft {
            id: Some(a.id.clone()),
            text: a.text.clone(),
        })
        .collect();
    let changed = s.save_item(Some(&a.id), Some(a.revision), draft).unwrap();
    assert_eq!(changed.achievements[0].id, a.achievements[1].id);
    let mut invalid = work();
    invalid.achievements[0].id = Some(b.achievements[0].id.clone());
    assert!(
        s.save_item(Some(&changed.id), Some(changed.revision), invalid)
            .is_err()
    );
    assert_eq!(s.item(&a.id).unwrap(), changed);
}
#[test]
fn library_to_resume_is_atomic_granular_and_does_not_leak_notes() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let a = s.save_item(None, None, work()).unwrap();
    let mut selection = selected(&a);
    selection.items.push(SelectionItem {
        item_id: "missing".into(),
        achievement_ids: vec![],
        include_background: false,
    });
    assert!(
        s.create_resume("不可创建", &selection, Style::default())
            .is_err()
    );
    assert_eq!(s.overview().unwrap().counts.resumes, 0);
    selection.items.pop();
    let resume = s
        .create_resume("选一条成果", &selection, Style::default())
        .unwrap();
    assert_eq!(resume.document.blocks[0].achievements.len(), 1);
    assert!(resume.document.profile.email.is_empty());
    assert!(
        !serde_json::to_string(&resume.document)
            .unwrap()
            .contains("内部备注")
    );
}
#[test]
fn copies_remain_independent_after_source_changes_and_deletion() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let item = s.save_item(None, None, work()).unwrap();
    let a = s
        .create_resume("A", &selected(&item), Style::default())
        .unwrap();
    let b = s.copy_resume(&a.id, "B").unwrap();
    let mut document = a.document.clone();
    document.blocks[0].achievements[0].text = "仅为 A 改写".into();
    s.save_resume(
        &a.id,
        a.revision,
        ResumeDraft {
            name: a.name,
            company: "目标公司".into(),
            role: "岗位".into(),
            document,
        },
    )
    .unwrap();
    assert_eq!(s.resume(&b.id).unwrap().document, b.document);
    let item = s
        .set_item_state(&item.id, item.revision, RecordState::Trashed)
        .unwrap();
    s.delete_item_permanently(&item.id, item.revision).unwrap();
    assert_eq!(s.resume(&b.id).unwrap().document, b.document);
}
#[test]
fn snapshots_and_trashed_resumes_keep_assets_until_permanent_delete() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let (_, resume, snapshot) = complete_fixture(&mut s).unwrap();
    let snap = s.snapshot(&snapshot).unwrap();
    let p = s.profile().unwrap();
    let mut content = p.content;
    content.photo_asset_id = None;
    s.save_profile(content, p.revision).unwrap();
    let r = s.resume(&resume).unwrap();
    let r = s
        .set_resume_state(&resume, r.revision, RecordState::Trashed)
        .unwrap();
    assert_eq!(s.collect_unreferenced_assets().unwrap(), 0);
    assert_eq!(s.snapshot(&snapshot).unwrap(), snap);
    let restored = s
        .restore_snapshot_as_resume(&snapshot, "从历史恢复")
        .unwrap();
    s.delete_resume_permanently(&resume, r.revision).unwrap();
    assert_eq!(s.collect_unreferenced_assets().unwrap(), 1);
    assert!(
        s.asset(restored.document.profile.photo_asset_id.as_ref().unwrap())
            .is_ok()
    );
}
#[test]
fn presets_use_latest_copy_and_report_missing_or_archived_sources() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let item = s.save_item(None, None, work()).unwrap();
    let preset = s
        .save_preset(
            None,
            None,
            PresetDraft {
                name: "岗位组合".into(),
                selection: selected(&item),
                style: Style::default(),
            },
        )
        .unwrap();
    let mut draft = work();
    draft.achievements = item
        .achievements
        .iter()
        .map(|a| AchievementDraft {
            id: Some(a.id.clone()),
            text: format!("最新：{}", a.text),
        })
        .collect();
    let item = s
        .save_item(Some(&item.id), Some(item.revision), draft)
        .unwrap();
    let r = s
        .create_resume(
            "最新",
            &preset.content.selection,
            preset.content.style.clone(),
        )
        .unwrap();
    assert!(
        r.document.blocks[0].achievements[0]
            .text
            .starts_with("最新")
    );
    s.set_item_state(&item.id, item.revision, RecordState::Archived)
        .unwrap();
    assert!(
        !s.inspect_selection(&preset.content.selection)
            .unwrap()
            .is_empty()
    );
    assert!(
        s.create_resume("拒绝静默缺失", &preset.content.selection, Style::default())
            .is_err()
    );
}
#[test]
fn live_wal_backup_restores_complete_data_to_an_empty_store() {
    let a = temp();
    let b = temp();
    let mut s = Store::open(a.path()).unwrap();
    let (item, resume, snapshot) = complete_fixture(&mut s).unwrap();
    let package = a.path().join("完整备份.rslbackup");
    let manifest = s.backup(&package).unwrap();
    let preview = s.inspect_backup(&package).unwrap();
    assert_eq!(manifest.counts, s.overview().unwrap().counts);
    let mut target = Store::open(b.path()).unwrap();
    target
        .restore(&package, &preview.package_sha256, RestoreFault::None)
        .unwrap();
    assert_eq!(target.item(&item).unwrap(), s.item(&item).unwrap());
    assert_eq!(target.resume(&resume).unwrap(), s.resume(&resume).unwrap());
    let snap = target.snapshot(&snapshot).unwrap();
    assert_eq!(
        target.asset(snap.pdf_asset_id.as_ref().unwrap()).unwrap(),
        real_pdf()
    );
    assert_eq!(
        target.backup_settings().unwrap(),
        s.backup_settings().unwrap()
    );
}
fn rewrite_package(
    source: &Path,
    target: &Path,
    mutate: impl FnOnce(&mut serde_json::Value, &mut Vec<u8>),
) {
    let mut archive = ZipArchive::new(fs::File::open(source).unwrap()).unwrap();
    let mut metadata = String::new();
    archive
        .by_name("manifest.json")
        .unwrap()
        .read_to_string(&mut metadata)
        .unwrap();
    let mut manifest: serde_json::Value = serde_json::from_str(&metadata).unwrap();
    let mut data = Vec::new();
    archive
        .by_name("data.sqlite3")
        .unwrap()
        .read_to_end(&mut data)
        .unwrap();
    mutate(&mut manifest, &mut data);
    let mut out = ZipWriter::new(fs::File::create(target).unwrap());
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    out.start_file("manifest.json", options).unwrap();
    out.write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    out.start_file("data.sqlite3", options).unwrap();
    out.write_all(&data).unwrap();
    out.finish().unwrap();
}
#[test]
fn corrupted_newer_truncated_or_changed_packages_preserve_current_data() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    update_name(&mut s, "原始").unwrap();
    let good = t.path().join("good.rslbackup");
    s.backup(&good).unwrap();
    let preview = s.inspect_backup(&good).unwrap();
    update_name(&mut s, "最新资料").unwrap();
    let newer = t.path().join("newer.rslbackup");
    rewrite_package(&good, &newer, |m, _| m["schemaVersion"] = 99.into());
    assert!(matches!(s.inspect_backup(&newer), Err(Error::Newer)));
    let bad = t.path().join("bad.rslbackup");
    fs::write(&bad, b"truncated zip").unwrap();
    assert!(s.restore(&bad, "invalid", RestoreFault::None).is_err());
    rewrite_package(&good, &bad, |_, data| data[0] ^= 1);
    assert!(s.inspect_backup(&bad).is_err());
    assert!(
        s.restore(&good, "changed-after-preview", RestoreFault::None)
            .is_err()
    );
    assert_eq!(s.profile().unwrap().content.name, "最新资料");
    assert!(!preview.package_sha256.is_empty());
}
#[test]
fn tampered_resource_or_reference_is_rejected_even_with_recomputed_package_hash() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    complete_fixture(&mut s).unwrap();
    let good = t.path().join("good.rslbackup");
    s.backup(&good).unwrap();
    let bad = t.path().join("bad.rslbackup");
    for sql in ["UPDATE assets SET data=x'010203'", "DELETE FROM asset_refs"] {
        rewrite_package(&good, &bad, |m, data| {
            let p = t.path().join("mutate.sqlite3");
            fs::write(&p, &*data).unwrap();
            let db = Connection::open(&p).unwrap();
            db.execute_batch(sql).unwrap();
            drop(db);
            *data = fs::read(p).unwrap();
            m["sha256"] = files::hash(data).into();
            m["databaseBytes"] = (data.len() as u64).into();
        });
        assert!(s.inspect_backup(&bad).is_err());
    }
}
#[test]
fn restore_failures_roll_back_and_preserve_pre_restore_backup() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    update_name(&mut s, "备份内容").unwrap();
    let package = t.path().join("old.rslbackup");
    s.backup(&package).unwrap();
    let preview = s.inspect_backup(&package).unwrap();
    update_name(&mut s, "最新已保存").unwrap();
    for fault in [
        RestoreFault::BeforeSwitch,
        RestoreFault::AfterOldMoved,
        RestoreFault::AfterNewInstalled,
    ] {
        assert!(s.restore(&package, &preview.package_sha256, fault).is_err());
        assert_eq!(s.profile().unwrap().content.name, "最新已保存");
        assert!(!t.path().join("restore-journal.json").exists());
    }
    let backups: Vec<_> = fs::read_dir(t.path().join("backups"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert_eq!(backups.len(), 3);
    for backup in backups {
        assert!(s.inspect_backup(&backup).is_ok());
    }
}
#[test]
fn interrupted_restore_recovers_on_restart_at_each_switch_boundary() {
    for (point, expected) in [
        ("oldMoved", "最新"),
        ("newInstalled", "最新"),
        ("committed", "备份"),
    ] {
        let t = temp();
        let package = t.path().join("old.rslbackup");
        {
            let mut s = Store::open(t.path()).unwrap();
            update_name(&mut s, "备份").unwrap();
            s.backup(&package).unwrap();
            update_name(&mut s, "最新").unwrap();
        }
        let result = Command::new(env!("CARGO_BIN_EXE_resume-core-probe"))
            .arg("restore")
            .arg(t.path())
            .arg(&package)
            .env("RESUME_CORE_TEST_CRASH", point)
            .status()
            .unwrap();
        assert_eq!(result.code(), Some(73));
        let s = Store::open(t.path()).unwrap();
        assert_eq!(s.profile().unwrap().content.name, expected);
        assert!(!t.path().join("restore-journal.json").exists());
    }
}
#[test]
fn process_exit_keeps_committed_data_and_library_lock_prevents_second_instance() {
    let t = temp();
    let result = Command::new(env!("CARGO_BIN_EXE_resume-core-probe"))
        .arg("commitExit")
        .arg(t.path())
        .status()
        .unwrap();
    assert_eq!(result.code(), Some(74));
    let s = Store::open(t.path()).unwrap();
    assert_eq!(s.profile().unwrap().content.name, "退出前已提交");
    assert!(Store::open(t.path()).is_err());
    drop(s);
    assert!(Store::open(t.path()).is_ok());
}
#[test]
fn restore_drains_earlier_writes_and_blocks_new_writes() {
    let t = temp();
    let package = t.path().join("before.rslbackup");
    let hash;
    {
        let s = Store::open(t.path()).unwrap();
        s.backup(&package).unwrap();
        hash = s.inspect_backup(&package).unwrap().package_sha256;
    }
    let actor = Actor::open(t.path()).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let save = actor
        .submit(true, move |s| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            update_name(s, "队列中修改")
        })
        .unwrap();
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let restore = actor.restore(package, hash, RestoreFault::None).unwrap();
    assert!(matches!(
        actor.submit(true, |_| Ok(())),
        Err(Error::Restoring)
    ));
    release_tx.send(()).unwrap();
    save.recv().unwrap().unwrap();
    restore.recv().unwrap().unwrap();
    assert_eq!(
        actor
            .call(false, |s| Ok(s.profile()?.content.name))
            .unwrap(),
        ""
    );
    let prebackup = fs::read_dir(t.path().join("backups"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let other = temp();
    let mut s = Store::open(other.path()).unwrap();
    let preview = s.inspect_backup(&prebackup).unwrap();
    s.restore(&prebackup, &preview.package_sha256, RestoreFault::None)
        .unwrap();
    assert_eq!(s.profile().unwrap().content.name, "队列中修改");
}
#[test]
fn failed_save_aborts_restore_until_successful_retry_is_acknowledged() {
    let t = temp();
    let package = t.path().join("old.rslbackup");
    let hash;
    {
        let s = Store::open(t.path()).unwrap();
        s.backup(&package).unwrap();
        hash = s.inspect_backup(&package).unwrap().package_sha256;
    }
    let actor = Actor::open(t.path()).unwrap();
    assert!(
        actor
            .call(true, |_| Err::<(), _>(Error::Invalid(
                "模拟保存失败".into()
            )))
            .is_err()
    );
    assert!(matches!(
        actor
            .restore(package.clone(), hash.clone(), RestoreFault::None)
            .unwrap()
            .recv()
            .unwrap(),
        Err(Error::Unsaved)
    ));
    actor.call(true, |s| update_name(s, "重试成功")).unwrap();
    actor.acknowledge_saved().unwrap();
    actor
        .restore(package, hash, RestoreFault::None)
        .unwrap()
        .recv()
        .unwrap()
        .unwrap();
}
#[test]
fn known_v1_migration_keeps_data_and_creates_backup() {
    let t = temp();
    let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
    let sql = include_str!("../migrations/001_initial.sql");
    db.execute_batch(sql).unwrap();
    db.execute(
        "INSERT INTO migrations VALUES (1,?,1)",
        [files::hash(sql.as_bytes())],
    )
    .unwrap();
    db.pragma_update(None, "application_id", migrations::APPLICATION_ID)
        .unwrap();
    db.pragma_update(None, "user_version", 1).unwrap();
    let p = ProfileDraft {
        name: "迁移前中文".into(),
        ..Default::default()
    };
    db.execute(
        "INSERT INTO profile VALUES (1,4,?)",
        [serde_json::to_string(&p).unwrap()],
    )
    .unwrap();
    drop(db);
    let s = Store::open(t.path()).unwrap();
    assert_eq!(s.profile().unwrap().content.name, "迁移前中文");
    assert_eq!(s.profile().unwrap().revision, 4);
    assert_eq!(s.overview().unwrap().schema_version, migrations::SCHEMA);
    assert!(fs::read_dir(t.path()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("before-migration-")
    }));
}
#[test]
fn unsupported_database_is_rejected_without_modifying_its_bytes() {
    let t = temp();
    let path = t.path().join("library.sqlite3");
    let db = Connection::open(&path).unwrap();
    db.execute_batch("CREATE TABLE test(value TEXT); PRAGMA user_version=99;")
        .unwrap();
    drop(db);
    let before = fs::read(&path).unwrap();
    assert!(matches!(Store::open(t.path()), Err(Error::Newer)));
    assert_eq!(before, fs::read(&path).unwrap());
}
#[test]
fn snapshot_sql_update_is_rejected_and_settings_use_revision_checks() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let (_, _, snapshot) = complete_fixture(&mut s).unwrap();
    let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
    assert!(
        db.execute(
            "UPDATE snapshots SET name='改写' WHERE id=?",
            params![snapshot]
        )
        .is_err()
    );
    assert!(matches!(
        s.save_backup_settings(0, BackupSettings::default()),
        Err(Error::Conflict)
    ));
}
#[test]
fn failed_migration_rolls_back_all_schema_changes_and_keeps_original_data() {
    let t = temp();
    let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
    let sql = include_str!("../migrations/001_initial.sql");
    db.execute_batch(sql).unwrap();
    db.execute(
        "INSERT INTO migrations VALUES (1,?,1)",
        [files::hash(sql.as_bytes())],
    )
    .unwrap();
    db.pragma_update(None, "application_id", migrations::APPLICATION_ID)
        .unwrap();
    db.pragma_update(None, "user_version", 1).unwrap();
    let p = ProfileDraft {
        name: "迁移故障也要保留".into(),
        ..Default::default()
    };
    db.execute(
        "INSERT INTO profile VALUES (1,1,?)",
        [serde_json::to_string(&p).unwrap()],
    )
    .unwrap();
    // A conflicting schema forces the second migration to fail after it starts.
    db.execute_batch("ALTER TABLE resumes ADD COLUMN state TEXT;")
        .unwrap();
    drop(db);
    assert!(Store::open(t.path()).is_err());
    let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
    assert!(db.prepare("SELECT state FROM library_items").is_err());
    let saved: String = db
        .query_row("SELECT content FROM profile", [], |r| r.get(0))
        .unwrap();
    assert!(saved.contains("迁移故障也要保留"));
}
#[test]
fn failed_pre_restore_backup_never_switches_current_library() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let package = t.path().join("old.rslbackup");
    s.backup(&package).unwrap();
    let preview = s.inspect_backup(&package).unwrap();
    update_name(&mut s, "不可丢失").unwrap();
    fs::write(t.path().join("backups"), b"directory is not writable").unwrap();
    assert!(
        s.restore(&package, &preview.package_sha256, RestoreFault::None)
            .is_err()
    );
    assert_eq!(s.profile().unwrap().content.name, "不可丢失");
    assert!(!t.path().join("restore-journal.json").exists());
}
#[test]
fn damaged_rollback_file_is_preserved_and_startup_refuses_to_guess() {
    let t = temp();
    let package = t.path().join("old.rslbackup");
    {
        let mut s = Store::open(t.path()).unwrap();
        s.backup(&package).unwrap();
        update_name(&mut s, "最新").unwrap();
    }
    let result = Command::new(env!("CARGO_BIN_EXE_resume-core-probe"))
        .arg("restore")
        .arg(t.path())
        .arg(&package)
        .env("RESUME_CORE_TEST_CRASH", "oldMoved")
        .status()
        .unwrap();
    assert_eq!(result.code(), Some(73));
    let journal: serde_json::Value =
        serde_json::from_slice(&fs::read(t.path().join("restore-journal.json")).unwrap()).unwrap();
    let old = t
        .path()
        .join("recovery")
        .join(journal["operation"].as_str().unwrap())
        .join("previous.sqlite3");
    let damaged = b"damaged rollback file";
    fs::write(&old, damaged).unwrap();
    assert!(Store::open(t.path()).is_err());
    assert_eq!(fs::read(&old).unwrap(), damaged);
    assert!(t.path().join("restore-journal.json").exists());
    assert!(
        fs::read_dir(t.path().join("backups"))
            .unwrap()
            .next()
            .is_some()
    );
}
#[cfg(windows)]
#[test]
fn locked_backup_destination_preserves_existing_file_and_allows_retry() {
    use std::os::windows::fs::OpenOptionsExt;
    let t = temp();
    let s = Store::open(t.path()).unwrap();
    let target = t.path().join("locked.rslbackup");
    fs::write(&target, b"old file").unwrap();
    let lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&target)
        .unwrap();
    assert!(s.backup(&target).is_err());
    assert_eq!(fs::read(&target).unwrap(), b"old file");
    drop(lock);
    assert!(s.backup(&target).is_ok());
    assert!(s.inspect_backup(&target).is_ok());
}
#[test]
fn blocked_wal_checkpoint_aborts_restore_without_losing_latest_commits() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let package = t.path().join("old.rslbackup");
    s.backup(&package).unwrap();
    let hash = s.inspect_backup(&package).unwrap().package_sha256;
    let reader = Connection::open(t.path().join("library.sqlite3")).unwrap();
    reader
        .execute_batch("BEGIN; SELECT * FROM profile;")
        .unwrap();
    update_name(&mut s, "读锁期间提交").unwrap();
    assert!(s.restore(&package, &hash, RestoreFault::None).is_err());
    assert_eq!(s.profile().unwrap().content.name, "读锁期间提交");
    assert!(!t.path().join("restore-journal.json").exists());
    reader.execute_batch("ROLLBACK").unwrap();
    drop(reader);
    s.restore(&package, &hash, RestoreFault::None).unwrap();
    assert_eq!(s.profile().unwrap().content.name, "");
}
#[test]
fn known_old_backup_is_migrated_before_restore() {
    let t = temp();
    let package = t.path().join("old.rslbackup");
    let s = Store::open(t.path()).unwrap();
    s.backup(&package).unwrap();
    drop(s);
    let v1 = t.path().join("v1.sqlite3");
    let db = Connection::open(&v1).unwrap();
    let sql = include_str!("../migrations/001_initial.sql");
    db.execute_batch(sql).unwrap();
    db.execute(
        "INSERT INTO migrations VALUES (1,?,1)",
        [files::hash(sql.as_bytes())],
    )
    .unwrap();
    db.pragma_update(None, "application_id", migrations::APPLICATION_ID)
        .unwrap();
    db.pragma_update(None, "user_version", 1).unwrap();
    let p = ProfileDraft {
        name: "旧版备份内容".into(),
        ..Default::default()
    };
    db.execute(
        "INSERT INTO profile VALUES (1,1,?)",
        [serde_json::to_string(&p).unwrap()],
    )
    .unwrap();
    drop(db);
    let converted = t.path().join("converted.rslbackup");
    rewrite_package(&package, &converted, |m, data| {
        *data = fs::read(&v1).unwrap();
        m["schemaVersion"] = 1.into();
        m["sha256"] = files::hash(data).into();
        m["databaseBytes"] = (data.len() as u64).into();
    });
    let empty = temp();
    let mut target = Store::open(empty.path()).unwrap();
    let preview = target.inspect_backup(&converted).unwrap();
    target
        .restore(&converted, &preview.package_sha256, RestoreFault::None)
        .unwrap();
    assert_eq!(target.profile().unwrap().content.name, "旧版备份内容");
    assert_eq!(
        target.overview().unwrap().schema_version,
        migrations::SCHEMA
    );
}
