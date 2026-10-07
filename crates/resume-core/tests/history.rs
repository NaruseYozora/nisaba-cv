use resume_core::{Store, backup::RestoreFault, files, migrations, model::*};
use rusqlite::{Connection, params};
use std::{fs, path::Path};
fn temp() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.test-output");
    fs::create_dir_all(&root).unwrap();
    tempfile::TempDir::new_in(root).unwrap()
}
fn pdf() -> Vec<u8> {
    include_bytes!("fixtures/anonymous.pdf").to_vec()
}
fn resume(store: &mut Store) -> Resume {
    let p = store.profile().unwrap();
    store
        .save_profile(
            ProfileDraft {
                name: "导出前姓名".into(),
                ..p.content
            },
            p.revision,
        )
        .unwrap();
    store
        .create_resume_target(
            "草稿",
            "公司",
            "岗位",
            &Selection {
                custom_field_ids: vec![],
                sections: None,
                profile_fields: vec![ProfileField::Name],
                items: vec![],
            },
            Style::default(),
        )
        .unwrap()
}
#[test]
fn frozen_export_commits_original_content_metadata_and_pdf_after_later_edit() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let r = resume(&mut s);
    let frozen = s.capture_export(&r.id, r.revision).unwrap();
    let mut document = r.document.clone();
    document.profile.name = "后续编辑".into();
    document.style.font_size = 14.0;
    let newer = s
        .save_resume(
            &r.id,
            r.revision,
            ResumeDraft {
                name: r.name.clone(),
                company: "新公司".into(),
                role: "新岗位".into(),
                document,
            },
        )
        .unwrap();
    let snapshot = s.record_export(&frozen, &pdf()).unwrap();
    assert_eq!(snapshot.document, frozen.document);
    assert_eq!(snapshot.company, "公司");
    assert_eq!(snapshot.role, "岗位");
    assert_eq!(snapshot.source_revision, r.revision);
    assert_eq!(s.snapshot_pdf(&snapshot.id).unwrap(), pdf());
    assert_eq!(s.resume(&r.id).unwrap(), newer);
    let restored = s
        .restore_snapshot_as_resume(&snapshot.id, "从历史恢复")
        .unwrap();
    assert_eq!(restored.document, frozen.document);
    assert_eq!(restored.role, "岗位");
    assert_ne!(restored.id, r.id);
    drop(s);
    let s = Store::open(t.path()).unwrap();
    assert_eq!(s.snapshot(&snapshot.id).unwrap(), snapshot);
    assert_eq!(s.snapshot_pdf(&snapshot.id).unwrap(), pdf());
}
#[test]
fn incomplete_pdf_and_trashed_or_missing_parent_do_not_commit_assets() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let r = resume(&mut s);
    let before = s.overview().unwrap().counts;
    assert!(s.record_export(&r, b"%PDF-partial").is_err());
    assert_eq!(s.overview().unwrap().counts, before);
    let trashed = s
        .set_resume_state(&r.id, r.revision, RecordState::Trashed)
        .unwrap();
    assert!(s.capture_export(&r.id, trashed.revision).is_err());
    assert!(s.record_export(&r, &pdf()).is_err());
    assert!(
        s.create_snapshot(&r.id, trashed.revision, "不能保存", None)
            .is_err()
    );
    assert_eq!(s.overview().unwrap().counts.assets, 0);
    s.delete_resume_permanently(&r.id, trashed.revision)
        .unwrap();
    assert!(s.record_export(&r, &pdf()).is_err());
    assert_eq!(s.overview().unwrap().counts.assets, 0);
}
#[test]
fn original_pdf_and_snapshot_survive_backup_and_style_changes() {
    let t = temp();
    let mut s = Store::open(t.path().join("first")).unwrap();
    let r = resume(&mut s);
    let snapshot = s.record_export(&r, &pdf()).unwrap();
    let named = s
        .create_snapshot(&r.id, r.revision, "命名历史", None)
        .unwrap();
    let mut document = r.document;
    document.style.font_family = "resume-serif-sc".into();
    document
        .style
        .module_titles
        .insert("work".into(), "任职经历".into());
    s.save_resume(
        &r.id,
        r.revision,
        ResumeDraft {
            name: r.name,
            company: r.company,
            role: r.role,
            document,
        },
    )
    .unwrap();
    assert_eq!(s.snapshot(&named.id).unwrap(), named);
    assert_eq!(s.snapshot_pdf(&snapshot.id).unwrap(), pdf());
    let backup = t.path().join("history.rslbackup");
    s.backup(&backup).unwrap();
    let mut new = Store::open(t.path().join("empty")).unwrap();
    let preview = new.inspect_backup(&backup).unwrap();
    new.restore(&backup, &preview.package_sha256, RestoreFault::None)
        .unwrap();
    assert_eq!(new.snapshot(&snapshot.id).unwrap(), snapshot);
    assert_eq!(new.snapshot_pdf(&snapshot.id).unwrap(), pdf());
    assert_eq!(new.snapshots(&r.id).unwrap().len(), 2);
}
#[test]
fn publication_rejects_changed_target_and_keeps_locked_file_then_retries() {
    let t = temp();
    let target = t.path().join("已有文件.pdf");
    fs::write(&target, "旧文件".as_bytes()).unwrap();
    let expected = files::pdf_target_hash(&target).unwrap().unwrap();
    fs::write(&target, "已被其他程序改写".as_bytes()).unwrap();
    assert!(files::publish_pdf(&target, &pdf(), Some(&expected)).is_err());
    assert_eq!(fs::read(&target).unwrap(), "已被其他程序改写".as_bytes());
    let expected = files::pdf_target_hash(&target).unwrap().unwrap();
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        let lock = fs::OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&target)
            .unwrap();
        assert!(files::publish_pdf(&target, &pdf(), Some(&expected)).is_err());
        drop(lock);
        assert_eq!(fs::read(&target).unwrap(), "已被其他程序改写".as_bytes());
    }
    files::publish_pdf(&target, &pdf(), Some(&expected)).unwrap();
    assert_eq!(fs::read(&target).unwrap(), pdf());
    assert!(files::publish_pdf(&target, b"not PDF", Some(&files::hash(&pdf()))).is_err());
    assert_eq!(fs::read(&target).unwrap(), pdf());
    assert_eq!(fs::read_dir(t.path()).unwrap().count(), 1);
}
#[test]
fn old_style_defaults_and_new_style_bounds_are_validated() {
    let old = r##"{"template":"single-column","templateVersion":1,"fontSize":11.0,"marginMm":15.0,"accent":"#245764"}"##;
    let mut style: Style = serde_json::from_str(old).unwrap();
    assert_eq!(style, Style::default());
    style.font_family = "resume-serif-sc".into();
    style.line_height = 1.8;
    style.module_titles.insert("work".into(), "职业经历".into());
    style.validate().unwrap();
    style.module_titles.insert("invalid".into(), "无效".into());
    assert!(style.validate().is_err());
    style.module_titles.clear();
    style.line_height = f64::NAN;
    assert!(style.validate().is_err());
}
#[test]
fn v3_upgrade_preserves_immutable_snapshot_json_and_adds_metadata_defaults() {
    let t = temp();
    let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
    let sql = [
        include_str!("../migrations/001_initial.sql"),
        include_str!("../migrations/002_lifecycle.sql"),
        include_str!("../migrations/003_preset_kinds.sql"),
    ];
    for (i, sql) in sql.iter().enumerate() {
        db.execute_batch(sql).unwrap();
        db.execute(
            "INSERT INTO migrations VALUES (?,?,1)",
            params![(i + 1) as i64, files::hash(sql.as_bytes())],
        )
        .unwrap();
    }
    db.pragma_update(None, "application_id", migrations::APPLICATION_ID)
        .unwrap();
    db.pragma_update(None, "user_version", 3).unwrap();
    db.execute(
        "INSERT INTO profile VALUES (1,1,?)",
        [serde_json::to_string(&ProfileDraft::default()).unwrap()],
    )
    .unwrap();
    let document = ResumeDocument {
        sections: vec![],
        format_version: 1,
        profile: ProfileDraft::default(),
        profile_source_revision: 1,
        blocks: vec![],
        style: Style::default(),
    };
    let mut json = serde_json::to_value(&document).unwrap();
    for key in ["fontFamily", "lineHeight", "sectionGapMm", "moduleTitles"] {
        json["style"].as_object_mut().unwrap().remove(key);
    }
    let original = serde_json::to_string(&json).unwrap();
    db.execute("INSERT INTO resumes(id,revision,name,company,role,document,updated_at) VALUES ('r',1,'旧草稿','','',?,1)",[&original]).unwrap();
    db.execute(
        "INSERT INTO snapshots VALUES ('s','r','旧快照',?,NULL,1)",
        [&original],
    )
    .unwrap();
    drop(db);
    let s = Store::open(t.path()).unwrap();
    let snap = s.snapshot("s").unwrap();
    assert_eq!(snap.document, document);
    assert_eq!(snap.source_revision, 0);
    assert_eq!(s.overview().unwrap().schema_version, migrations::SCHEMA);
    drop(s);
    let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("SELECT document FROM snapshots WHERE id='s'", [], |r| {
            r.get::<_, String>(0)
        })
        .unwrap(),
        original
    );
    assert!(
        db.execute("UPDATE snapshots SET role='覆盖' WHERE id='s'", [])
            .is_err()
    );
}
