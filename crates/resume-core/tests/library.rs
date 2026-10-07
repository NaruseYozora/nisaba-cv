use image::{DynamicImage, ImageFormat, ImageReader, RgbaImage};
use resume_core::{Error, Store, backup::RestoreFault, model::*};
use std::{fs, io::Cursor, path::Path};
use tempfile::TempDir;
fn temp() -> TempDir {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.test-output");
    fs::create_dir_all(&root).unwrap();
    TempDir::new_in(root).unwrap()
}
fn draft() -> ItemDraft {
    ItemDraft {
        content: ItemContent::Work {
            company: "匿名公司".into(),
            role: "工程师".into(),
            department: String::new(),
            location: String::new(),
            dates: DateRange::default(),
        },
        tags: vec!["后端".into()],
        notes: "内部证据".into(),
        achievements: vec![AchievementDraft {
            id: Some(uuid::Uuid::new_v4().to_string()),
            text: "第一个成果".into(),
        }],
    }
}
fn png() -> Vec<u8> {
    include_bytes!("fixtures/photo.png").to_vec()
}
#[test]
fn client_achievement_ids_survive_new_edits_reordering_and_late_additions() {
    let root = temp();
    let mut store = Store::open(root.path()).unwrap();
    let mut value = draft();
    let first = store.save_item(None, None, value.clone()).unwrap();
    assert_eq!(
        first.achievements[0].id,
        value.achievements[0].id.as_ref().unwrap().as_str()
    );
    value.achievements[0].text = "保存进行中输入的第二版".into();
    value.achievements.push(AchievementDraft {
        id: Some(uuid::Uuid::new_v4().to_string()),
        text: "新增成果".into(),
    });
    value.achievements.reverse();
    let next = store
        .save_item(Some(&first.id), Some(first.revision), value.clone())
        .unwrap();
    assert_eq!(next.achievements[1].id, first.achievements[0].id);
    assert_eq!(
        next.achievements[0].id,
        value.achievements[0].id.as_ref().unwrap().as_str()
    );
    assert_eq!(next.achievements[1].text, "保存进行中输入的第二版");
}
#[test]
fn client_ids_cannot_steal_achievements_and_duplicate_ids_roll_back() {
    let root = temp();
    let mut store = Store::open(root.path()).unwrap();
    let first = store.save_item(None, None, draft()).unwrap();
    let mut foreign = draft();
    foreign.achievements[0].id = Some(first.achievements[0].id.clone());
    assert!(store.save_item(None, None, foreign).is_err());
    let mut duplicate = draft();
    duplicate
        .achievements
        .push(duplicate.achievements[0].clone());
    assert!(store.save_item(None, None, duplicate).is_err());
    assert_eq!(store.overview().unwrap().counts.items, 1);
    let mut invalid = draft();
    invalid.achievements[0].id = Some("fake-id".into());
    assert!(store.save_item(None, None, invalid).is_err());
    assert_eq!(store.overview().unwrap().counts.achievements, 1);
}
#[test]
fn copying_creates_independent_active_item_and_new_achievement_ids() {
    let root = temp();
    let mut store = Store::open(root.path()).unwrap();
    let source = store.save_item(None, None, draft()).unwrap();
    let archived = store
        .set_item_state(&source.id, source.revision, RecordState::Archived)
        .unwrap();
    assert!(matches!(
        store.copy_item(&source.id, source.revision),
        Err(Error::Conflict)
    ));
    let copy = store.copy_item(&archived.id, archived.revision).unwrap();
    assert_eq!(copy.state, RecordState::Active);
    assert_ne!(copy.id, source.id);
    assert_ne!(copy.achievements[0].id, source.achievements[0].id);
    let mut changed = draft();
    changed.achievements[0].id = Some(copy.achievements[0].id.clone());
    changed.achievements[0].text = "只改副本".into();
    store
        .save_item(Some(&copy.id), Some(copy.revision), changed)
        .unwrap();
    assert_eq!(
        store.item(&source.id).unwrap().achievements[0].text,
        "第一个成果"
    );
    let trashed = store
        .set_item_state(&source.id, archived.revision, RecordState::Trashed)
        .unwrap();
    assert!(store.copy_item(&trashed.id, trashed.revision).is_err());
}
#[test]
fn photo_import_is_atomic_and_rejects_invalid_or_stale_input() {
    let root = temp();
    let mut store = Store::open(root.path()).unwrap();
    assert!(store.import_profile_photo(1, b"\x89PNG\r\n\x1a\n").is_err());
    assert_eq!(store.overview().unwrap().counts.assets, 0);
    let profile = store.import_profile_photo(1, &png()).unwrap();
    assert_eq!(profile.revision, 2);
    let id = profile.content.photo_asset_id.clone().unwrap();
    assert!(store.photo_asset(&id).unwrap().1.starts_with(b"\x89PNG"));
    assert!(matches!(
        store.import_profile_photo(1, &png()),
        Err(Error::Conflict)
    ));
    assert_eq!(store.overview().unwrap().counts.assets, 1);
    assert_eq!(store.profile().unwrap().content.photo_asset_id, Some(id));
}
#[test]
fn imported_photo_survives_source_deletion_restart_and_backup_restore() {
    let root = temp();
    let source = root.path().join("original.png");
    fs::write(&source, png()).unwrap();
    let mut store = Store::open(root.path().join("data")).unwrap();
    let profile = store
        .import_profile_photo(1, &fs::read(&source).unwrap())
        .unwrap();
    let id = profile.content.photo_asset_id.unwrap();
    fs::remove_file(source).unwrap();
    let bytes = store.photo_asset(&id).unwrap().1;
    let backup = root.path().join("photo.rslbackup");
    store.backup(&backup).unwrap();
    drop(store);
    let reopened = Store::open(root.path().join("data")).unwrap();
    assert_eq!(reopened.photo_asset(&id).unwrap().1, bytes);
    drop(reopened);
    let mut restored = Store::open(root.path().join("restored")).unwrap();
    let preview = restored.inspect_backup(&backup).unwrap();
    restored
        .restore(&backup, &preview.package_sha256, RestoreFault::None)
        .unwrap();
    assert_eq!(
        restored.profile().unwrap().content.photo_asset_id,
        Some(id.clone())
    );
    assert_eq!(restored.photo_asset(&id).unwrap().1, bytes);
    assert_eq!(restored.collect_unreferenced_assets().unwrap(), 0);
}
#[test]
fn photo_resize_preserves_aspect_and_rejects_large_dimensions() {
    let photo = DynamicImage::ImageRgba8(RgbaImage::new(3200, 20));
    let mut bytes = Cursor::new(Vec::new());
    photo.write_to(&mut bytes, ImageFormat::Png).unwrap();
    let output = resume_core::photo::normalize(bytes.get_ref()).unwrap();
    let decoded = ImageReader::new(Cursor::new(output))
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!((decoded.width(), decoded.height()), (1600, 10));
    let photo = DynamicImage::ImageRgba8(RgbaImage::new(10_001, 1));
    let mut bytes = Cursor::new(Vec::new());
    photo.write_to(&mut bytes, ImageFormat::Png).unwrap();
    assert!(resume_core::photo::normalize(bytes.get_ref()).is_err());
}
#[test]
fn jpeg_exif_orientation_is_flattened_and_source_metadata_removed() {
    let photo = DynamicImage::new_rgb8(8, 4);
    let mut encoded = Cursor::new(Vec::new());
    photo.write_to(&mut encoded, ImageFormat::Jpeg).unwrap();
    let exif = b"Exif\0\0II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0";
    let mut input = vec![0xff, 0xd8, 0xff, 0xe1];
    input.extend_from_slice(&((exif.len() + 2) as u16).to_be_bytes());
    input.extend_from_slice(exif);
    input.extend_from_slice(&encoded.get_ref()[2..]);
    let output = resume_core::photo::normalize(&input).unwrap();
    assert!(!output.windows(4).any(|v| v == b"Exif"));
    let decoded = ImageReader::new(Cursor::new(output))
        .with_guessed_format()
        .unwrap()
        .decode()
        .unwrap();
    assert_eq!((decoded.width(), decoded.height()), (4, 8));
}
