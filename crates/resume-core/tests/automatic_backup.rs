use resume_core::{
    Store,
    automatic_backup::{AutomaticBackupStatus, BackupClock},
    backup::RestoreFault,
    files, migrations,
    model::*,
};
use rusqlite::{Connection, params};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn temp() -> tempfile::TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.test-output");
    fs::create_dir_all(&root).unwrap();
    tempfile::TempDir::new_in(root).unwrap()
}
fn clock(minutes: u64, day: u8) -> BackupClock {
    BackupClock {
        millis: 1_790_000_000_000 + minutes * 60_000,
        day: format!("2026-10-{day:02}"),
    }
}
fn edit(s: &mut Store, name: &str) {
    let p = s.profile().unwrap();
    s.save_profile(
        ProfileDraft {
            name: name.into(),
            ..p.content
        },
        p.revision,
    )
    .unwrap();
}
fn run(s: &mut Store, minutes: u64, day: u8) -> AutomaticBackupStatus {
    let status = s.automatic_backup_at(false, &clock(minutes, day)).unwrap();
    assert!(status.last_error.is_none(), "{:?}", status.last_error);
    status
}
fn index(root: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(root.join("automatic-backup.json")).unwrap()).unwrap()
}
fn packages(path: &str) -> Vec<PathBuf> {
    fs::read_dir(path)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "rslbackup"))
        .collect()
}
#[test]
fn changes_schedule_both_tiers_and_survive_restart_without_redundant_backups() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    edit(&mut s, "首次");
    let first = run(&mut s, 0, 5);
    assert_eq!(first.managed_count, 2);
    let token = s.change_token().unwrap();
    assert_eq!(run(&mut s, 10, 5).managed_count, 2);
    edit(&mut s, "第二次");
    assert_ne!(s.change_token().unwrap(), token);
    assert_eq!(run(&mut s, 29, 5).managed_count, 2);
    assert_eq!(run(&mut s, 30, 5).managed_count, 3);
    drop(s);
    let mut s = Store::open(t.path()).unwrap();
    assert_eq!(run(&mut s, 1440, 6).managed_count, 3);
    edit(&mut s, "次日修改");
    let status = run(&mut s, 1441, 6);
    assert_eq!(status.managed_count, 5);
    assert_eq!(status.last_daily_at, Some(clock(1441, 6).millis));
    for p in packages(&status.directory) {
        assert_eq!(
            s.inspect_backup(&p).unwrap().manifest.schema_version,
            migrations::SCHEMA
        );
    }
}
#[test]
fn rotation_retains_limits_and_never_deletes_manual_or_unregistered_files() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    s.save_backup_settings(
        0,
        BackupSettings {
            rolling_keep: 3,
            daily_keep: 2,
            ..Default::default()
        },
    )
    .unwrap();
    let first = run(&mut s, 0, 1);
    let manual = Path::new(&first.directory).join("手动保留.rslbackup");
    s.backup(&manual).unwrap();
    let bytes = fs::read(&manual).unwrap();
    let unknown =
        Path::new(&first.directory).join(format!("rolling-{}.rslbackup", resume_core::store::id()));
    fs::write(&unknown, b"not registered").unwrap();
    for day in 2..=9 {
        edit(&mut s, &format!("day{day}"));
        run(&mut s, (day as u64) * 1440, day);
    }
    let status = s.automatic_backup_status().unwrap();
    assert_eq!(status.managed_count, 5);
    assert_eq!(packages(&status.directory).len(), 7);
    assert_eq!(fs::read(manual).unwrap(), bytes);
    assert_eq!(fs::read(unknown).unwrap(), b"not registered");
    let records = index(t.path());
    let rows = records["records"].as_array().unwrap();
    assert_eq!(rows.iter().filter(|r| r["kind"] == "rolling").count(), 3);
    assert_eq!(rows.iter().filter(|r| r["kind"] == "daily").count(), 2);
    assert!(
        rows.iter()
            .filter(|r| r["kind"] == "daily")
            .all(|r| ["2026-10-08", "2026-10-09"].contains(&r["day"].as_str().unwrap()))
    );
}
#[test]
fn failed_backup_is_visible_preserves_data_and_old_backups_and_can_retry() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let first = run(&mut s, 0, 5);
    let old = packages(&first.directory);
    let before: Vec<_> = old.iter().map(|p| fs::read(p).unwrap()).collect();
    let blocked = t.path().join("blocked");
    fs::write(&blocked, b"file instead of directory").unwrap();
    s.save_backup_settings(
        0,
        BackupSettings {
            directory: Some(blocked.display().to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    edit(&mut s, "失败时仍然保存");
    let failed = s.automatic_backup_at(false, &clock(40, 5)).unwrap();
    assert!(failed.last_error.is_some());
    assert!(failed.last_success_at.is_none());
    assert_eq!(s.profile().unwrap().content.name, "失败时仍然保存");
    s.save_backup_settings(1, BackupSettings::default())
        .unwrap();
    let recovered = run(&mut s, 41, 5);
    assert!(recovered.last_success_at.is_some());
    for (p, bytes) in old.iter().zip(before) {
        assert_eq!(fs::read(p).unwrap(), bytes);
    }
}
#[test]
fn externally_modified_old_backup_stops_rotation_and_keeps_new_backup() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    s.save_backup_settings(
        0,
        BackupSettings {
            rolling_keep: 1,
            ..Default::default()
        },
    )
    .unwrap();
    let first = run(&mut s, 0, 5);
    let state = index(t.path());
    let old = state["records"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "rolling")
        .unwrap()["name"]
        .as_str()
        .unwrap();
    let path = Path::new(&first.directory).join(old);
    fs::write(&path, b"externally edited backup").unwrap();
    edit(&mut s, "later");
    let status = s.automatic_backup_at(false, &clock(31, 5)).unwrap();
    assert!(status.last_error.as_ref().unwrap().contains("外部修改"));
    assert_eq!(fs::read(path).unwrap(), b"externally edited backup");
    assert_eq!(status.managed_count, 3);
    assert_eq!(status.last_success_at, Some(clock(31, 5).millis));
}
#[test]
fn unreadable_index_is_preserved_before_explicit_retry_creates_new_namespace() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let first = run(&mut s, 0, 5);
    let old = packages(&first.directory);
    drop(s);
    fs::write(t.path().join("automatic-backup.json"), b"broken index").unwrap();
    let mut s = Store::open(t.path()).unwrap();
    assert!(s.automatic_backup_status().unwrap().last_error.is_some());
    let failed = s.automatic_backup_at(false, &clock(31, 5)).unwrap();
    assert!(failed.last_error.is_some());
    s.backup(&t.path().join("manual.rslbackup")).unwrap();
    let repaired = s.automatic_backup_at(true, &clock(32, 5)).unwrap();
    assert!(repaired.last_error.is_none());
    assert_ne!(repaired.directory, first.directory);
    assert!(old.iter().all(|p| p.exists()));
    assert!(fs::read_dir(t.path()).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("automatic-backup-unreadable-")
    }));
}
#[test]
fn change_marker_is_transactional_and_restore_of_another_branch_is_detected() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    edit(&mut s, "分支 A");
    let a = s.change_token().unwrap();
    let package = t.path().join("a.rslbackup");
    s.backup(&package).unwrap();
    let preview = s.inspect_backup(&package).unwrap();
    run(&mut s, 0, 5);
    assert!(s.save_profile(ProfileDraft::default(), 9999).is_err());
    assert_eq!(s.change_token().unwrap(), a);
    edit(&mut s, "分支 B");
    run(&mut s, 31, 5);
    let b = s.change_token().unwrap();
    assert_ne!(a, b);
    s.restore(&package, &preview.package_sha256, RestoreFault::None)
        .unwrap();
    assert_eq!(s.change_token().unwrap(), a);
    assert_eq!(run(&mut s, 62, 5).managed_count, 4);
}
#[test]
fn disabled_backup_and_directory_changes_are_respected() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    s.save_backup_settings(
        0,
        BackupSettings {
            rolling_enabled: false,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(run(&mut s, 0, 5).managed_count, 0);
    let manual = s.automatic_backup_at(true, &clock(1, 5)).unwrap();
    assert_eq!(manual.managed_count, 2);
    let old = packages(&manual.directory);
    s.save_backup_settings(
        1,
        BackupSettings {
            directory: Some(t.path().join("other").display().to_string()),
            ..Default::default()
        },
    )
    .unwrap();
    let new = run(&mut s, 2, 5);
    assert_eq!(new.managed_count, 2);
    assert_ne!(new.directory, manual.directory);
    assert!(old.iter().all(|p| p.exists()));
}
#[test]
fn v4_migration_preserves_content_and_failed_migration_keeps_v4_usable() {
    for fail in [false, true] {
        let t = temp();
        let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
        for (version, sql) in [
            (1, include_str!("../migrations/001_initial.sql")),
            (2, include_str!("../migrations/002_lifecycle.sql")),
            (3, include_str!("../migrations/003_preset_kinds.sql")),
            (4, include_str!("../migrations/004_snapshot_metadata.sql")),
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
        db.pragma_update(None, "user_version", 4).unwrap();
        let content = serde_json::to_string(&ProfileDraft {
            name: "已保存档案".into(),
            ..Default::default()
        })
        .unwrap();
        db.execute("INSERT INTO profile VALUES(1,8,?)", [&content])
            .unwrap();
        if fail {
            db.execute_batch("CREATE TRIGGER changed_profile_INSERT AFTER INSERT ON profile BEGIN SELECT 1; END;").unwrap();
        }
        drop(db);
        if fail {
            assert!(Store::open(t.path()).is_err());
            let db = Connection::open(t.path().join("library.sqlite3")).unwrap();
            assert_eq!(
                db.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
                    .unwrap(),
                4
            );
            assert!(db.prepare("SELECT token FROM change_state").is_err());
            assert_eq!(
                db.query_row("SELECT content FROM profile", [], |r| r.get::<_, String>(0))
                    .unwrap(),
                content
            );
        } else {
            let s = Store::open(t.path()).unwrap();
            assert_eq!(s.profile().unwrap().revision, 8);
            assert_eq!(s.profile().unwrap().content.name, "已保存档案");
            assert_eq!(s.change_token().unwrap().len(), 32);
        }
        assert!(fs::read_dir(t.path()).unwrap().any(|e| {
            e.unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with("before-migration-")
        }));
    }
}
#[test]
fn trashed_library_item_rejects_hidden_backend_edits() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let draft = ItemDraft {
        content: ItemContent::Skill {
            name: "Rust".into(),
            category: "开发".into(),
            description: "保存前内容".into(),
        },
        tags: vec![],
        notes: String::new(),
        achievements: vec![],
    };
    let item = s.save_item(None, None, draft.clone()).unwrap();
    let trashed = s
        .set_item_state(&item.id, item.revision, RecordState::Trashed)
        .unwrap();
    let token = s.change_token().unwrap();
    assert!(
        s.save_item(Some(&item.id), Some(trashed.revision), draft)
            .is_err()
    );
    assert_eq!(s.item(&item.id).unwrap(), trashed);
    assert_eq!(s.change_token().unwrap(), token);
}
#[test]
fn backward_calendar_change_does_not_duplicate_an_existing_daily_backup() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    run(&mut s, 0, 5);
    edit(&mut s, "第二天");
    run(&mut s, 1440, 6);
    edit(&mut s, "时区回到昨天");
    run(&mut s, 1471, 5);
    let state = index(t.path());
    assert_eq!(
        state["records"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| r["kind"] == "daily" && r["day"] == "2026-10-05")
            .count(),
        1
    );
}
