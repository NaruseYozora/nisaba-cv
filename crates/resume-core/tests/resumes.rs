use resume_core::{Error, Store, backup::RestoreFault, files, migrations, model::*};
use rusqlite::{Connection, params};
use std::{fs, io::Write, path::Path};
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};
fn temp() -> TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.test-output");
    fs::create_dir_all(&root).unwrap();
    TempDir::new_in(root).unwrap()
}
fn item(store: &mut Store, name: &str) -> LibraryItem {
    store
        .save_item(
            None,
            None,
            ItemDraft {
                content: ItemContent::Project {
                    name: name.into(),
                    role: "开发".into(),
                    url: None,
                    background: "原始背景".into(),
                    dates: DateRange::default(),
                },
                tags: vec!["内部标签".into()],
                notes: "内部备注".into(),
                achievements: vec![
                    AchievementDraft {
                        id: None,
                        text: "成果一".into(),
                    },
                    AchievementDraft {
                        id: None,
                        text: "成果二".into(),
                    },
                ],
            },
        )
        .unwrap()
}
fn selection(items: &[&LibraryItem]) -> Selection {
    Selection {
        custom_field_ids: vec![],
        sections: None,
        profile_fields: vec![ProfileField::Name],
        items: items
            .iter()
            .map(|i| SelectionItem {
                item_id: i.id.clone(),
                achievement_ids: vec![i.achievements[1].id.clone(), i.achievements[0].id.clone()],
                include_background: false,
            })
            .collect(),
    }
}
fn draft(r: &Resume) -> ResumeDraft {
    ResumeDraft {
        name: r.name.clone(),
        company: r.company.clone(),
        role: r.role.clone(),
        document: r.document.clone(),
    }
}
#[test]
fn target_metadata_granular_selection_and_order_commit_together() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let a = item(&mut s, "项目 A");
    let b = item(&mut s, "项目 B");
    let r = s
        .create_resume_target(
            "定制简历",
            "目标公司",
            "岗位",
            &selection(&[&b, &a]),
            Style::default(),
        )
        .unwrap();
    assert_eq!(r.company, "目标公司");
    assert_eq!(r.role, "岗位");
    assert_eq!(r.document.blocks[0].source.item_id, b.id);
    assert_eq!(r.document.blocks[0].achievements[0].text, "成果二");
    assert!(!serde_json::to_string(&r.document).unwrap().contains("内部"));
    assert!(
        matches!(&r.document.blocks[0].content,ItemContent::Project{background,..} if background.is_empty())
    );
}
#[test]
fn append_is_atomic_rejects_duplicates_missing_sources_and_stale_revisions() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let a = item(&mut s, "A");
    let b = item(&mut s, "B");
    let r = s
        .create_resume("简历", &selection(&[&a]), Style::default())
        .unwrap();
    let mut pick = selection(&[&b]);
    pick.profile_fields.clear();
    let appended = s.add_resume_selection(&r.id, r.revision, &pick).unwrap();
    assert_eq!(appended.document.blocks.len(), 2);
    assert_eq!(appended.document.profile, r.document.profile);
    assert!(matches!(
        s.add_resume_selection(&r.id, r.revision, &pick),
        Err(Error::Conflict)
    ));
    assert!(
        s.add_resume_selection(&r.id, appended.revision, &pick)
            .is_err()
    );
    pick.items[0].item_id = "missing".into();
    assert!(
        s.add_resume_selection(&r.id, appended.revision, &pick)
            .is_err()
    );
    assert_eq!(s.resume(&r.id).unwrap(), appended);
}
#[test]
fn temporary_content_and_save_as_item_never_modify_existing_source() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let a = item(&mut s, "来源");
    let r = s
        .create_resume("简历", &selection(&[&a]), Style::default())
        .unwrap();
    let mut d = draft(&r);
    d.document.blocks[0].achievements[0].text = "只在简历内改写".into();
    let mut block = d.document.blocks[0].clone();
    block.id = resume_core::store::id();
    block.source = Source {
        item_id: String::new(),
        revision: 0,
    };
    block.achievements.clear();
    d.document.blocks.push(block);
    let changed = s.save_resume(&r.id, r.revision, d).unwrap();
    let created = s
        .resume_block_as_item(&r.id, changed.revision, &changed.document.blocks[0].id)
        .unwrap();
    assert_ne!(created.id, a.id);
    assert_ne!(created.achievements[0].id, a.achievements[1].id);
    assert_eq!(created.achievements[0].text, "只在简历内改写");
    assert_eq!(s.item(&a.id).unwrap(), a);
    assert!(created.tags.is_empty());
    assert!(created.notes.is_empty());
    drop(s);
    let reopened = Store::open(t.path()).unwrap();
    assert_eq!(reopened.resume(&r.id).unwrap(), changed);
}
#[test]
fn copied_reordered_hidden_resume_survives_source_permanent_deletion() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let a = item(&mut s, "A");
    let b = item(&mut s, "B");
    let r = s
        .create_resume("A 简历", &selection(&[&a, &b]), Style::default())
        .unwrap();
    let mut d = draft(&r);
    d.document.blocks.reverse();
    d.document.blocks[0].visible = false;
    d.document.blocks[1].page_break_before = true;
    d.document.blocks[1].achievements[0].text = "岗位定制".into();
    d.document.style.accent = "#aabbcc".into();
    let changed = s.save_resume(&r.id, r.revision, d).unwrap();
    let copy = s
        .copy_resume_checked(&changed.id, Some(changed.revision), "副本")
        .unwrap();
    assert!(matches!(
        s.copy_resume_checked(&r.id, Some(r.revision), "过期"),
        Err(Error::Conflict)
    ));
    let trashed = s
        .set_item_state(&a.id, a.revision, RecordState::Trashed)
        .unwrap();
    s.delete_item_permanently(&a.id, trashed.revision).unwrap();
    assert_eq!(s.resume(&copy.id).unwrap().document, changed.document);
    let mut d = draft(&copy);
    d.document.blocks[1].achievements[0].text = "只改副本".into();
    s.save_resume(&copy.id, copy.revision, d).unwrap();
    assert_eq!(s.resume(&r.id).unwrap(), changed);
}
#[test]
fn preset_kinds_updates_deletes_and_backup_remain_independent() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let a = item(&mut s, "素材");
    let recipe = s
        .save_preset(
            None,
            None,
            PresetDraft {
                name: "选材".into(),
                selection: selection(&[&a]),
                style: Style::default(),
            },
        )
        .unwrap();
    let layout = s
        .save_preset_kind(
            None,
            None,
            PresetKind::Layout,
            PresetDraft {
                name: "版式".into(),
                selection: Selection::default(),
                style: Style {
                    font_size: 10.0,
                    ..Default::default()
                },
            },
        )
        .unwrap();
    assert_eq!(
        s.presets(PresetKind::Selection).unwrap(),
        vec![recipe.clone()]
    );
    assert_eq!(s.presets(PresetKind::Layout).unwrap(), vec![layout.clone()]);
    let r = s
        .create_resume(
            "简历",
            &recipe.content.selection,
            layout.content.style.clone(),
        )
        .unwrap();
    assert!(
        s.save_preset_kind(
            Some(&recipe.id),
            Some(recipe.revision),
            PresetKind::Layout,
            layout.content.clone()
        )
        .is_err()
    );
    let mut update = layout.content.clone();
    update.style.font_size = 14.0;
    let next = s
        .save_preset_kind(
            Some(&layout.id),
            Some(layout.revision),
            PresetKind::Layout,
            update,
        )
        .unwrap();
    assert_eq!(s.resume(&r.id).unwrap().document.style.font_size, 10.0);
    assert!(s.delete_preset(&layout.id, layout.revision).is_err());
    let backup = t.path().join("presets.rslbackup");
    s.backup(&backup).unwrap();
    s.delete_preset(&layout.id, next.revision).unwrap();
    s.delete_preset(&recipe.id, recipe.revision).unwrap();
    assert_eq!(s.resume(&r.id).unwrap(), r);
    let preview = s.inspect_backup(&backup).unwrap();
    s.restore(&backup, &preview.package_sha256, RestoreFault::None)
        .unwrap();
    assert_eq!(s.presets(PresetKind::Layout).unwrap(), vec![next]);
    assert_eq!(s.presets(PresetKind::Selection).unwrap(), vec![recipe]);
}
#[test]
fn layout_preset_rejects_selection_and_trashed_resume_rejects_mutation() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let a = item(&mut s, "A");
    assert!(
        s.save_preset_kind(
            None,
            None,
            PresetKind::Layout,
            PresetDraft {
                name: "非法混合".into(),
                selection: selection(&[&a]),
                style: Style::default()
            }
        )
        .is_err()
    );
    let r = s
        .create_resume("简历", &selection(&[&a]), Style::default())
        .unwrap();
    let trashed = s
        .set_resume_state(&r.id, r.revision, RecordState::Trashed)
        .unwrap();
    assert!(
        s.save_resume(&r.id, trashed.revision, draft(&trashed))
            .is_err()
    );
    assert!(s.copy_resume(&r.id, "副本").is_err());
    assert_eq!(s.resume(&r.id).unwrap(), trashed);
}
fn v2(path: &Path) -> PresetDraft {
    let db = Connection::open(path).unwrap();
    for (version, sql) in [
        (1, include_str!("../migrations/001_initial.sql")),
        (2, include_str!("../migrations/002_lifecycle.sql")),
    ] {
        db.execute_batch(sql).unwrap();
        db.execute(
            "INSERT INTO migrations VALUES (?,?,1)",
            params![version, files::hash(sql.as_bytes())],
        )
        .unwrap();
    }
    db.pragma_update(None, "application_id", migrations::APPLICATION_ID)
        .unwrap();
    db.pragma_update(None, "user_version", 2).unwrap();
    db.execute(
        "INSERT INTO profile VALUES (1,1,?)",
        [serde_json::to_string(&ProfileDraft {
            name: "原始档案".into(),
            ..Default::default()
        })
        .unwrap()],
    )
    .unwrap();
    let p = PresetDraft {
        name: "旧选材".into(),
        selection: Selection::default(),
        style: Style::default(),
    };
    db.execute(
        "INSERT INTO presets VALUES ('old-preset',1,?)",
        [serde_json::to_string(&p).unwrap()],
    )
    .unwrap();
    p
}
#[test]
fn v2_upgrade_keeps_data_and_backup_without_rewriting_old_migrations() {
    let t = temp();
    let p = v2(&t.path().join("library.sqlite3"));
    let s = Store::open(t.path()).unwrap();
    assert_eq!(s.overview().unwrap().schema_version, migrations::SCHEMA);
    assert_eq!(s.preset("old-preset").unwrap().content, p);
    assert_eq!(s.preset("old-preset").unwrap().kind, PresetKind::Selection);
    assert_eq!(s.profile().unwrap().content.name, "原始档案");
    let retained = fs::read_dir(t.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("before-migration-")
        })
        .unwrap();
    let old = Connection::open(retained.path()).unwrap();
    assert_eq!(
        old.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        2
    );
    let original: String = old
        .query_row(
            "SELECT content FROM presets WHERE id='old-preset'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(serde_json::from_str::<PresetDraft>(&original).unwrap(), p);
    let columns: i64 = old
        .query_row(
            "SELECT COUNT(*) FROM pragma_table_info('presets') WHERE name='kind'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(columns, 0);
}
#[test]
fn v2_backup_restores_presets_and_profile_to_new_schema() {
    let t = temp();
    let p = v2(&t.path().join("old.sqlite3"));
    let bytes = fs::read(t.path().join("old.sqlite3")).unwrap();
    let manifest = serde_json::json!({"backupFormat":1,"appVersion":"0.3.0","schemaVersion":2,"createdAt":1,"sha256":files::hash(&bytes),"databaseBytes":bytes.len(),"counts":{"items":0,"achievements":0,"resumes":0,"snapshots":0,"assets":0,"presets":1}});
    let package = t.path().join("old.rslbackup");
    let mut zip = ZipWriter::new(fs::File::create(&package).unwrap());
    zip.start_file("manifest.json", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    zip.start_file("data.sqlite3", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&bytes).unwrap();
    zip.finish().unwrap();
    let original = files::hash(&fs::read(&package).unwrap());
    let mut s = Store::open(t.path().join("new")).unwrap();
    let preview = s.inspect_backup(&package).unwrap();
    s.restore(&package, &preview.package_sha256, RestoreFault::None)
        .unwrap();
    assert_eq!(s.preset("old-preset").unwrap().content, p);
    assert_eq!(s.overview().unwrap().schema_version, migrations::SCHEMA);
    assert_eq!(files::hash(&fs::read(&package).unwrap()), original);
}
