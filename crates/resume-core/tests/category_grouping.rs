use resume_core::{
    Error, Store,
    backup::{Manifest, RestoreFault},
    catalog::{CategoryRevision, ProfileEditorDraft},
    files, grouping, migrations,
    model::*,
};
use rusqlite::{Connection, params};
use std::{fs, io::Write, path::Path};
use tempfile::TempDir;
use zip::{ZipWriter, write::SimpleFileOptions};
fn temp() -> TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.test-output");
    fs::create_dir_all(&root).unwrap();
    TempDir::new_in(root).unwrap()
}
fn skill(name: &str) -> ItemDraft {
    ItemDraft {
        content: ItemContent::Skill {
            name: name.into(),
            category: "语言与工具".into(),
            description: "原始技能说明".into(),
        },
        tags: vec![],
        notes: "不进入简历".into(),
        achievements: vec![],
    }
}
fn custom(name: &str, category: &str) -> ItemDraft {
    ItemDraft {
        content: ItemContent::Custom {
            category: category.into(),
            title: name.into(),
            subtitle: "证书信息".into(),
            dates: Default::default(),
        },
        tags: vec![],
        notes: String::new(),
        achievements: vec![],
    }
}
fn pick(item: &LibraryItem) -> SelectionItem {
    SelectionItem {
        item_id: item.id.clone(),
        achievement_ids: vec![],
        include_background: false,
    }
}
fn selection(items: &[&LibraryItem]) -> Selection {
    let mut sections: Vec<SelectionSection> = vec![];
    for item in items {
        if let Some(s) = sections
            .iter_mut()
            .find(|s| s.category_id == item.category_id)
        {
            s.items.push(pick(item));
        } else {
            sections.push(SelectionSection {
                category_id: item.category_id.clone(),
                items: vec![pick(item)],
            });
        }
    }
    Selection {
        profile_fields: vec![ProfileField::Name],
        sections: Some(sections),
        ..Default::default()
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
fn field(label: &str, value: &str) -> CustomField {
    CustomField {
        id: resume_core::store::id(),
        label: label.into(),
        value: value.into(),
    }
}

#[test]
fn native_profile_retains_legacy_summary_and_selects_custom_fields_by_stable_id() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let p = s.profile().unwrap();
    let mut old = p.content;
    old.summary = "升级前个人简介".into();
    s.save_profile(old, p.revision).unwrap();
    let a = field("工作许可", "无需担保");
    let b = field("GitHub", "example");
    let p = s
        .save_profile_editor(
            ProfileEditorDraft {
                name: "欧阳喆".into(),
                custom_fields: vec![a.clone(), b.clone()],
                ..Default::default()
            },
            2,
        )
        .unwrap();
    assert_eq!(p.content.summary, "升级前个人简介");
    let selected = Selection {
        sections: Some(vec![]),
        custom_field_ids: vec![b.id.clone(), a.id.clone()],
        ..Default::default()
    };
    let r = s
        .create_resume("新简历", &selected, Style::default())
        .unwrap();
    assert_eq!(r.document.format_version, 2);
    assert_eq!(r.document.profile.custom_fields, vec![b.clone(), a.clone()]);
    assert!(r.document.profile.summary.is_empty());
    let mut changed = p.content.clone();
    changed.custom_fields[0].value = "新值".into();
    s.save_profile(changed, p.revision).unwrap();
    assert_eq!(
        s.resume(&r.id).unwrap().document.profile.custom_fields[1].value,
        "无需担保"
    );
    assert!(matches!(
        s.save_profile_editor(ProfileEditorDraft::default(), p.revision),
        Err(Error::Conflict)
    ));
    let mut invalid = s.profile().unwrap().content;
    invalid.custom_fields.push(a);
    assert!(invalid.validate().is_err());
    invalid.custom_fields = vec![field("GitHub", "a"), field(" github ", "b")];
    assert!(invalid.validate().is_err());
    invalid.custom_fields = vec![field(" ", "a")];
    assert!(invalid.validate().is_err());
    drop(s);
    let s = Store::open(t.path()).unwrap();
    assert_eq!(s.profile().unwrap().content.custom_fields[0].value, "新值");
}
#[test]
fn categories_have_stable_ids_cas_names_and_safe_deletion() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    assert_eq!(s.categories().unwrap().len(), 5);
    let c = s.create_category("  证书  ", "custom").unwrap();
    assert_eq!(c.name, "证书");
    assert!(s.create_category("证书", "custom").is_err());
    let item = s
        .save_item_in_category(None, None, &c.id, custom("证书甲", "旧文本"))
        .unwrap();
    let renamed = s.rename_category(&c.id, c.revision, "资格证书").unwrap();
    assert_eq!(renamed.id, c.id);
    assert_eq!(s.item(&item.id).unwrap().category_id, c.id);
    assert!(matches!(
        s.rename_category(&c.id, c.revision, "过期修改"),
        Err(Error::Conflict)
    ));
    assert!(s.delete_category(&c.id, renamed.revision).is_err());
    let item = s
        .set_item_state(&item.id, item.revision, RecordState::Trashed)
        .unwrap();
    assert!(s.delete_category(&c.id, renamed.revision).is_err());
    let token = s.change_token().unwrap();
    assert!(
        s.move_item_category(&item.id, item.revision, "builtin:skill")
            .is_err()
    );
    assert_eq!(s.change_token().unwrap(), token);
    s.move_item_category(&item.id, item.revision, "builtin:custom")
        .unwrap();
    s.delete_category(&c.id, renamed.revision).unwrap();
    assert!(s.delete_category("builtin:skill", 1).is_err());
    let c = s.create_category("API", "custom").unwrap();
    assert!(s.create_category("api", "custom").is_err());
    let a = s
        .save_item_in_category(None, None, &c.id, custom("A", "旧名称"))
        .unwrap();
    let copied = s.copy_item(&a.id, a.revision).unwrap();
    assert_eq!(copied.category_id, c.id);
}
#[test]
fn category_order_conflicts_and_invalid_item_writes_roll_back_every_change() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let initial = s.categories().unwrap();
    let mut order: Vec<_> = initial
        .iter()
        .rev()
        .map(|c| CategoryRevision {
            id: c.id.clone(),
            revision: c.revision,
        })
        .collect();
    s.reorder_categories(&order).unwrap();
    let categories = s.categories().unwrap();
    let token = s.change_token().unwrap();
    assert!(matches!(s.reorder_categories(&order), Err(Error::Conflict)));
    assert_eq!(s.categories().unwrap(), categories);
    assert_eq!(s.change_token().unwrap(), token);
    order.pop();
    assert!(s.reorder_categories(&order).is_err());
    assert!(
        s.save_item_in_category(None, None, "missing", skill("A"))
            .is_err()
    );
    assert!(
        s.save_item_in_category(None, None, "builtin:education", skill("A"))
            .is_err()
    );
    let mut bad = custom("新条目", "不应留下的类别");
    bad.achievements.push(AchievementDraft {
        id: Some("not-a-uuid".into()),
        text: "成果".into(),
    });
    assert!(s.save_item(None, None, bad).is_err());
    assert_eq!(s.categories().unwrap(), categories);
    assert_eq!(s.overview().unwrap().counts.items, 0);
    assert_eq!(s.change_token().unwrap(), token);
}
#[test]
fn grouped_selection_orders_categories_and_items_without_leaking_notes() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let a = s.save_item(None, None, skill("Rust")).unwrap();
    let b = s.save_item(None, None, skill("SQL")).unwrap();
    let c = s.create_category("证书", "custom").unwrap();
    let x = s
        .save_item_in_category(None, None, &c.id, custom("证书甲", "不作为大标题"))
        .unwrap();
    let r = s
        .create_resume("简历", &selection(&[&x, &b, &a]), Style::default())
        .unwrap();
    assert_eq!(
        r.document
            .sections
            .iter()
            .map(|s| s.title.as_str())
            .collect::<Vec<_>>(),
        vec!["证书", "技能"]
    );
    assert_eq!(
        r.document
            .blocks
            .iter()
            .map(|b| b.content.title())
            .collect::<Vec<_>>(),
        vec!["证书甲", "SQL", "Rust"]
    );
    assert!(
        !serde_json::to_string(&r.document)
            .unwrap()
            .contains("不进入简历")
    );
    s.rename_category(&c.id, c.revision, "资格").unwrap();
    assert_eq!(s.resume(&r.id).unwrap().document.sections[0].title, "证书");
    let mut bad = selection(&[&a]);
    bad.sections.as_mut().unwrap().push(SelectionSection {
        category_id: "builtin:custom".into(),
        items: vec![pick(&a)],
    });
    assert!(bad.validate().is_err());
    let mut mixed = selection(&[&a]);
    mixed.items.push(pick(&b));
    assert!(mixed.validate().is_err());
}
#[test]
fn category_and_custom_field_changes_are_reported_by_saved_presets() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let a = s.save_item(None, None, skill("A")).unwrap();
    let c = s.create_category("其他技能", "skill").unwrap();
    let mut selected = selection(&[&a]);
    let f = field("网站", "本地测试");
    let mut p = s.profile().unwrap();
    p.content.custom_fields.push(f.clone());
    s.save_profile(p.content, p.revision).unwrap();
    selected.custom_field_ids.push(f.id);
    let preset = s
        .save_preset(
            None,
            None,
            PresetDraft {
                name: "选材".into(),
                selection: selected.clone(),
                style: Style::default(),
            },
        )
        .unwrap();
    s.move_item_category(&a.id, a.revision, &c.id).unwrap();
    let p = s.profile().unwrap();
    let mut content = p.content;
    content.custom_fields.clear();
    s.save_profile(content, p.revision).unwrap();
    let warnings = s
        .inspect_selection(&s.preset(&preset.id).unwrap().content.selection)
        .unwrap();
    assert_eq!(warnings.len(), 2);
    assert!(
        s.create_resume("失效选材", &selected, Style::default())
            .is_err()
    );
    assert_eq!(s.overview().unwrap().counts.resumes, 0);
    assert!(
        s.save_preset_kind(
            None,
            None,
            PresetKind::Layout,
            PresetDraft {
                name: "不合法版式".into(),
                selection: selected,
                style: Style::default()
            }
        )
        .is_err()
    );
}
#[test]
fn grouped_append_stays_with_existing_category_and_rejects_flat_input() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let a = s.save_item(None, None, skill("A")).unwrap();
    let b = s.save_item(None, None, skill("B")).unwrap();
    let c = s.save_item(None, None, custom("证书", "证书")).unwrap();
    let r = s
        .create_resume("分组", &selection(&[&a, &c]), Style::default())
        .unwrap();
    let mut more = selection(&[&b]);
    more.profile_fields.clear();
    let r = s.add_resume_selection(&r.id, r.revision, &more).unwrap();
    assert_eq!(
        r.document
            .blocks
            .iter()
            .map(|b| b.content.title())
            .collect::<Vec<_>>(),
        vec!["A", "B", "证书"]
    );
    assert_eq!(r.document.sections.len(), 2);
    assert!(s.add_resume_selection(&r.id, r.revision, &more).is_err());
    let d = s.save_item(None, None, skill("D")).unwrap();
    let flat = Selection {
        items: vec![pick(&d)],
        ..Default::default()
    };
    assert!(s.add_resume_selection(&r.id, r.revision, &flat).is_err());
    assert_eq!(s.resume(&r.id).unwrap(), r);
}
#[test]
fn two_level_reorder_preserves_blocks_and_invalid_edits_are_atomic() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let a = s.save_item(None, None, skill("A")).unwrap();
    let b = s.save_item(None, None, skill("B")).unwrap();
    let c = s.save_item(None, None, custom("C", "证书")).unwrap();
    let r = s
        .create_resume("R", &selection(&[&a, &b, &c]), Style::default())
        .unwrap();
    let mut doc = r.document.clone();
    let section = doc.sections[0].id.clone();
    let order = doc.sections[0]
        .block_ids
        .iter()
        .rev()
        .cloned()
        .collect::<Vec<_>>();
    grouping::reorder_section_items(&mut doc, &section, &order).unwrap();
    let order = doc
        .sections
        .iter()
        .rev()
        .map(|s| s.id.clone())
        .collect::<Vec<_>>();
    grouping::reorder_sections(&mut doc, &order).unwrap();
    assert_eq!(
        doc.blocks
            .iter()
            .map(|b| b.content.title())
            .collect::<Vec<_>>(),
        vec!["C", "B", "A"]
    );
    let before = doc.clone();
    assert!(grouping::reorder_sections(&mut doc, &["missing".into()]).is_err());
    assert_eq!(doc, before);
    let mut edited = draft(&r);
    edited.document = doc;
    let saved = s.save_resume(&r.id, r.revision, edited.clone()).unwrap();
    assert!(matches!(
        s.save_resume(&r.id, r.revision, edited),
        Err(Error::Conflict)
    ));
    let mut invalid = draft(&saved);
    invalid.document.sections[0].block_ids.clear();
    assert!(s.save_resume(&r.id, saved.revision, invalid).is_err());
    let mut downgraded = draft(&saved);
    downgraded.document.format_version = 1;
    downgraded.document.sections.clear();
    assert!(s.save_resume(&r.id, saved.revision, downgraded).is_err());
    assert_eq!(s.resume(&r.id).unwrap(), saved);
}

fn v5(root: &Path) -> (String, String) {
    let db = Connection::open(root.join("library.sqlite3")).unwrap();
    db.pragma_update(None, "foreign_keys", true).unwrap();
    for (i, sql) in [
        include_str!("../migrations/001_initial.sql"),
        include_str!("../migrations/002_lifecycle.sql"),
        include_str!("../migrations/003_preset_kinds.sql"),
        include_str!("../migrations/004_snapshot_metadata.sql"),
        include_str!("../migrations/005_change_tracking.sql"),
    ]
    .iter()
    .enumerate()
    {
        db.execute_batch(sql).unwrap();
        db.execute(
            "INSERT INTO migrations VALUES (?,?,1)",
            params![i as i64 + 1, files::hash(sql.as_bytes())],
        )
        .unwrap();
    }
    db.pragma_update(None, "application_id", migrations::APPLICATION_ID)
        .unwrap();
    db.pragma_update(None, "user_version", 5).unwrap();
    let profile = ProfileDraft {
        name: "旧用户".into(),
        summary: "旧简介必须保留".into(),
        ..Default::default()
    };
    let profile_json = serde_json::to_string(&profile).unwrap();
    db.execute("INSERT INTO profile VALUES (1,8,?)", [&profile_json])
        .unwrap();
    let contents = vec![
        skill("A").content,
        custom("C", "证书").content,
        skill("B").content,
        custom("同名类型", "技能").content,
        custom("未命名", "").content,
    ];
    let mut blocks = vec![];
    for (i, content) in contents.into_iter().enumerate() {
        let item_id = format!("i{i}");
        db.execute("INSERT INTO library_items(id,revision,kind,title,content,tags,notes,updated_at,state) VALUES (?,3,?,?,?,'[]','旧备注',1,?)",params![item_id,content.kind(),content.title(),serde_json::to_string(&content).unwrap(),if i==3 {"archived"}else if i==4 {"trashed"}else{"active"}]).unwrap();
        blocks.push(ResumeBlock {
            id: format!("b{i}"),
            source: Source {
                item_id,
                revision: 3,
            },
            content,
            achievements: vec![],
            visible: true,
            page_break_before: i == 2,
        });
    }
    let document = ResumeDocument {
        format_version: 1,
        profile,
        profile_source_revision: 8,
        blocks,
        style: Style::default(),
        sections: vec![],
    };
    let json = serde_json::to_string(&document).unwrap();
    db.execute("INSERT INTO resumes(id,revision,name,company,role,document,updated_at) VALUES ('r',7,'旧简历','公司','岗位',?,1)",[&json]).unwrap();
    let pdf = include_bytes!("fixtures/anonymous.pdf");
    db.execute(
        "INSERT INTO assets VALUES ('pdf','pdf','application/pdf',?,?,1)",
        params![pdf.as_slice(), files::hash(pdf)],
    )
    .unwrap();
    db.execute("INSERT INTO snapshots(id,resume_id,name,document,pdf_asset_id,created_at,company,role,source_revision) VALUES ('s','r','旧导出',?,'pdf',1,'公司','岗位',7)",[&json]).unwrap();
    db.execute("INSERT INTO asset_refs VALUES ('snapshot','s','pdf')", [])
        .unwrap();
    let preset = PresetDraft {
        name: "旧选材".into(),
        selection: Selection {
            profile_fields: vec![ProfileField::Name],
            items: vec![SelectionItem {
                item_id: "i0".into(),
                achievement_ids: vec![],
                include_background: false,
            }],
            ..Default::default()
        },
        style: Style::default(),
    };
    db.execute(
        "INSERT INTO presets VALUES ('p',2,?,'selection')",
        [serde_json::to_string(&preset).unwrap()],
    )
    .unwrap();
    (json, profile_json)
}
fn old_backup(root: &Path, destination: &Path) {
    let data = fs::read(root.join("library.sqlite3")).unwrap();
    let manifest = Manifest {
        backup_format: 1,
        app_version: "0.7.0".into(),
        schema_version: 5,
        created_at: 1,
        sha256: files::hash(&data),
        database_bytes: data.len() as u64,
        counts: Counts {
            items: 5,
            achievements: 0,
            resumes: 1,
            snapshots: 1,
            assets: 1,
            presets: 1,
        },
    };
    let mut zip = ZipWriter::new(fs::File::create(destination).unwrap());
    zip.start_file("manifest.json", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    zip.start_file("data.sqlite3", SimpleFileOptions::default())
        .unwrap();
    zip.write_all(&data).unwrap();
    zip.finish().unwrap();
}
#[test]
fn schema5_migration_preserves_all_original_json_pdf_and_revisions() {
    let t = temp();
    let (json, profile) = v5(t.path());
    let s = Store::open(t.path()).unwrap();
    assert_eq!(s.overview().unwrap().schema_version, 6);
    assert_eq!(
        s.item("i0").unwrap().category_id,
        s.item("i2").unwrap().category_id
    );
    assert_eq!(s.item("i0").unwrap().category_id, "builtin:skill");
    assert_ne!(s.item("i3").unwrap().category_id, "builtin:skill");
    assert_eq!(s.item("i4").unwrap().category_id, "builtin:custom");
    assert_eq!(s.item("i0").unwrap().revision, 3);
    assert_eq!(s.profile().unwrap().revision, 8);
    assert_eq!(
        s.snapshot_pdf("s").unwrap(),
        include_bytes!("fixtures/anonymous.pdf")
    );
    assert_eq!(s.preset("p").unwrap().revision, 2);
    drop(s);
    let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
    for table in ["resumes", "snapshots"] {
        assert_eq!(
            db.query_row(&format!("SELECT document FROM {table}"), [], |r| r
                .get::<_, String>(0))
                .unwrap(),
            json
        );
    }
    assert_eq!(
        db.query_row("SELECT content FROM profile", [], |r| r.get::<_, String>(0))
            .unwrap(),
        profile
    );
    let backups: Vec<_> = fs::read_dir(t.path())
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| {
            p.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("before-migration-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    let old = Connection::open(&backups[0]).unwrap();
    assert_eq!(
        old.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
}
#[test]
fn conversion_is_explicit_grouped_and_snapshot_preserving() {
    let t = temp();
    let (json, _) = v5(t.path());
    let mut s = Store::open(t.path()).unwrap();
    let original = s.resume("r").unwrap();
    assert_eq!(original.document.format_version, 1);
    let preview = grouping::grouped_copy(&original.document).unwrap();
    assert_eq!(
        preview
            .blocks
            .iter()
            .map(|b| b.id.as_str())
            .collect::<Vec<_>>(),
        vec!["b0", "b2", "b1", "b3", "b4"]
    );
    assert!(preview.blocks[1].page_break_before);
    assert_eq!(s.resume("r").unwrap(), original);
    let converted = s.convert_resume_to_grouped("r", 7).unwrap();
    assert_eq!(converted.document, preview);
    assert_eq!(converted.document.profile.summary, "旧简介必须保留");
    assert_eq!(s.snapshots("r").unwrap().len(), 2);
    assert!(matches!(
        s.convert_resume_to_grouped("r", 7),
        Err(Error::Conflict)
    ));
    assert_eq!(s.convert_resume_to_grouped("r", 8).unwrap(), converted);
    assert_eq!(s.snapshots("r").unwrap().len(), 2);
    let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
    assert_eq!(
        db.query_row(
            "SELECT document FROM snapshots WHERE name='分组转换前'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        json
    );
    let restored = s.restore_snapshot_as_resume("s", "恢复旧简历").unwrap();
    assert_eq!(restored.document, original.document);
    assert_eq!(
        s.snapshot_pdf("s").unwrap(),
        include_bytes!("fixtures/anonymous.pdf")
    );
}
#[test]
fn conversion_failure_rolls_back_new_snapshot_revision_and_change_token() {
    let t = temp();
    v5(t.path());
    let mut s = Store::open(t.path()).unwrap();
    let token = s.change_token().unwrap();
    let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
    db.execute_batch("CREATE TRIGGER fail_conversion BEFORE UPDATE ON resumes BEGIN SELECT RAISE(ABORT,'injected failure'); END;").unwrap();
    assert!(s.convert_resume_to_grouped("r", 7).is_err());
    assert_eq!(s.snapshots("r").unwrap().len(), 1);
    assert_eq!(s.resume("r").unwrap().revision, 7);
    assert_eq!(s.change_token().unwrap(), token);
}
#[test]
fn invalid_migration_data_rolls_back_schema6_and_keeps_original_copy() {
    let t = temp();
    v5(t.path());
    let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
    db.execute("UPDATE profile SET content='{}'", []).unwrap();
    drop(db);
    assert!(Store::open(t.path()).is_err());
    let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
    assert_eq!(
        db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        5
    );
    assert!(db.prepare("SELECT * FROM categories").is_err());
    assert_eq!(
        db.query_row("SELECT content FROM profile", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "{}"
    );
}
#[test]
fn old_backup_and_new_backup_restore_categories_fields_grouping_and_pdf() {
    let old = temp();
    v5(old.path());
    let target = temp();
    let package = target.path().join("old.rslbackup");
    old_backup(old.path(), &package);
    let mut s = Store::open(target.path().join("data")).unwrap();
    let preview = s.inspect_backup(&package).unwrap();
    assert_eq!(preview.manifest.schema_version, 5);
    s.restore(&package, &preview.package_sha256, RestoreFault::None)
        .unwrap();
    let converted = s.convert_resume_to_grouped("r", 7).unwrap();
    let category = s.create_category("兴趣", "custom").unwrap();
    let profile = s.profile().unwrap();
    let mut content = profile.content;
    content.custom_fields = vec![field("附加信息", "保留")];
    s.save_profile(content, profile.revision).unwrap();
    let backup = target.path().join("new.rslbackup");
    let manifest = s.backup(&backup).unwrap();
    assert_eq!(manifest.schema_version, 6);
    assert_eq!(manifest.backup_format, 1);
    let other = temp();
    let mut restored = Store::open(other.path()).unwrap();
    let preview = restored.inspect_backup(&backup).unwrap();
    restored
        .restore(&backup, &preview.package_sha256, RestoreFault::None)
        .unwrap();
    assert_eq!(restored.category(&category.id).unwrap(), category);
    assert_eq!(restored.resume("r").unwrap(), converted);
    assert_eq!(restored.profile().unwrap(), s.profile().unwrap());
    assert_eq!(
        restored.snapshot_pdf("s").unwrap(),
        s.snapshot_pdf("s").unwrap()
    );
}
#[test]
fn deleted_library_category_does_not_destroy_frozen_resume_or_pdf() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let c = s.create_category("证书", "custom").unwrap();
    let item = s
        .save_item_in_category(None, None, &c.id, custom("证书甲", "证书"))
        .unwrap();
    let r = s
        .create_resume("简历", &selection(&[&item]), Style::default())
        .unwrap();
    let snap = s
        .record_export(&r, include_bytes!("fixtures/anonymous.pdf"))
        .unwrap();
    let item = s
        .set_item_state(&item.id, item.revision, RecordState::Trashed)
        .unwrap();
    s.delete_item_permanently(&item.id, item.revision).unwrap();
    s.delete_category(&c.id, c.revision).unwrap();
    assert_eq!(s.resume(&r.id).unwrap(), r);
    assert_eq!(
        s.snapshot_pdf(&snap.id).unwrap(),
        include_bytes!("fixtures/anonymous.pdf")
    );
    drop(s);
    assert!(Store::open(t.path()).is_ok());
}
#[test]
fn missing_mapping_and_invalid_group_documents_are_rejected_when_opening() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let item = s.save_item(None, None, skill("A")).unwrap();
    drop(s);
    let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
    db.execute("DELETE FROM item_categories WHERE item_id=?", [item.id])
        .unwrap();
    drop(db);
    assert!(Store::open(t.path()).is_err());
    let mut doc = ResumeDocument {
        format_version: 2,
        profile: Default::default(),
        profile_source_revision: 1,
        blocks: vec![],
        style: Default::default(),
        sections: vec![ResumeSection {
            id: "s".into(),
            category_id: "c".into(),
            kind: "skill".into(),
            title: "技能".into(),
            block_ids: vec!["missing".into()],
        }],
    };
    assert!(doc.validate().is_err());
    doc.sections.clear();
    doc.format_version = 3;
    assert!(doc.validate().is_err());
}
#[test]
fn category_only_changes_trigger_automatic_backup_and_are_restorable() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let clock = resume_core::automatic_backup::BackupClock {
        millis: 1_790_000_000_000,
        day: "2026-10-06".into(),
    };
    let first = s.automatic_backup_at(false, &clock).unwrap();
    let token = s.change_token().unwrap();
    s.rename_category("builtin:skill", 1, "专业技能").unwrap();
    assert_ne!(s.change_token().unwrap(), token);
    let second = s
        .automatic_backup_at(
            false,
            &resume_core::automatic_backup::BackupClock {
                millis: clock.millis + 31 * 60 * 1000,
                ..clock
            },
        )
        .unwrap();
    assert!(second.last_error.is_none());
    assert_eq!(first.managed_count, 2);
    assert_eq!(second.managed_count, 3);
    let index: serde_json::Value =
        serde_json::from_slice(&fs::read(t.path().join("automatic-backup.json")).unwrap()).unwrap();
    let token = s.change_token().unwrap();
    let record = index["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["token"].as_str() == Some(&token))
        .unwrap();
    let package = Path::new(&second.directory).join(record["name"].as_str().unwrap());
    let other = temp();
    let mut restored = Store::open(other.path()).unwrap();
    let preview = restored.inspect_backup(&package).unwrap();
    restored
        .restore(&package, &preview.package_sha256, RestoreFault::None)
        .unwrap();
    assert_eq!(restored.category("builtin:skill").unwrap().name, "专业技能");
}

#[test]
fn legacy_presets_convert_in_memory_and_never_drop_unavailable_sources() {
    let t = temp();
    v5(t.path());
    let mut s = Store::open(t.path()).unwrap();
    let original = s.preset("p").unwrap();
    let grouped = s.grouped_selection(&original.content.selection).unwrap();
    assert!(grouped.items.is_empty());
    assert_eq!(
        grouped.sections.as_ref().unwrap()[0].category_id,
        "builtin:skill"
    );
    assert_eq!(s.preset("p").unwrap(), original);
    s.create_resume("旧预设的新简历", &grouped, Style::default())
        .unwrap();
    let item = s.item("i0").unwrap();
    s.set_item_state(&item.id, item.revision, RecordState::Archived)
        .unwrap();
    assert!(s.grouped_selection(&original.content.selection).is_err());
}
#[test]
fn direct_format_upgrade_cannot_skip_the_pre_conversion_snapshot() {
    let t = temp();
    v5(t.path());
    let mut s = Store::open(t.path()).unwrap();
    let original = s.resume("r").unwrap();
    let mut edited = draft(&original);
    edited.document = grouping::grouped_copy(&original.document).unwrap();
    assert!(s.save_resume("r", 7, edited).is_err());
    assert_eq!(s.resume("r").unwrap(), original);
    assert_eq!(s.snapshots("r").unwrap().len(), 1);
}
#[test]
fn saved_back_grouped_material_uses_its_category_and_accepts_explicit_replacement() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let c = s.create_category("资格", "custom").unwrap();
    let item = s
        .save_item_in_category(None, None, &c.id, custom("证书", "过期旧标签"))
        .unwrap();
    let r = s
        .create_resume("R", &selection(&[&item]), Style::default())
        .unwrap();
    let block = &r.document.blocks[0];
    let saved = s
        .resume_block_as_item(&r.id, r.revision, &block.id)
        .unwrap();
    assert_eq!(saved.category_id, c.id);
    let new = s.create_category("其他资格", "custom").unwrap();
    let saved = s
        .resume_block_as_item_in_category(&r.id, r.revision, &block.id, &new.id)
        .unwrap();
    assert_eq!(saved.category_id, new.id);
    assert_eq!(s.resume(&r.id).unwrap(), r);
}

#[test]
fn conversion_snapshot_keeps_photo_after_profile_and_draft_remove_it() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let profile = s
        .import_profile_photo(1, include_bytes!("fixtures/photo.png"))
        .unwrap();
    let asset = profile.content.photo_asset_id.clone().unwrap();
    let old = s
        .create_resume(
            "旧照片简历",
            &Selection {
                profile_fields: vec![ProfileField::Photo],
                ..Default::default()
            },
            Style::default(),
        )
        .unwrap();
    let converted = s.convert_resume_to_grouped(&old.id, old.revision).unwrap();
    let snapshot = s.snapshots(&old.id).unwrap().remove(0);
    let mut content = profile.content;
    content.photo_asset_id = None;
    s.save_profile(content, profile.revision).unwrap();
    let mut edited = draft(&converted);
    edited.document.profile.photo_asset_id = None;
    s.save_resume(&converted.id, converted.revision, edited)
        .unwrap();
    s.collect_unreferenced_assets().unwrap();
    assert!(!s.asset(&asset).unwrap().is_empty());
    assert_eq!(
        s.snapshot(&snapshot.id)
            .unwrap()
            .document
            .profile
            .photo_asset_id,
        Some(asset)
    );
    let package = t.path().join("photo.rslbackup");
    s.backup(&package).unwrap();
    assert!(s.inspect_backup(&package).is_ok());
}
