use nisaba_cv::{
    app::{App, Page},
    editors::{Picker, ResumeEditor},
    history_pdf, render,
};
use resume_core::{Store, model::*};
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};

fn bundle() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime")
}
fn temp() -> tempfile::TempDir {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.test-output");
    fs::create_dir_all(&root).unwrap();
    tempfile::tempdir_in(root).unwrap()
}
fn fixture(s: &mut Store, photo: bool) -> Resume {
    let p = s.profile().unwrap();
    s.save_profile(
        ProfileDraft {
            name: "匿名测试".into(),
            ..Default::default()
        },
        p.revision,
    )
    .unwrap();
    if photo {
        let mut out = std::io::Cursor::new(Vec::new());
        image::RgbaImage::from_pixel(20, 30, image::Rgba([30, 100, 160, 255]))
            .write_to(&mut out, image::ImageFormat::Png)
            .unwrap();
        s.import_profile_photo(s.profile().unwrap().revision, &out.into_inner())
            .unwrap();
    }
    let item = s
        .save_item_in_category(
            None,
            None,
            "builtin:skill",
            ItemDraft {
                content: ItemContent::Skill {
                    name: "Rust".into(),
                    description: "中文测试".into(),
                    category: String::new(),
                },
                tags: vec![],
                notes: String::new(),
                achievements: vec![],
            },
        )
        .unwrap();
    let mut p = Picker::default();
    p.toggle_item(&item, true);
    if photo {
        p.selection.profile_fields.push(ProfileField::Photo);
    }
    s.create_resume("匿名验收", &p.selection, Style::default())
        .unwrap()
}
fn settle(a: &mut App) {
    let end = Instant::now() + Duration::from_secs(45);
    while a.busy() {
        a.poll();
        assert!(Instant::now() < end);
        std::thread::sleep(Duration::from_millis(3));
    }
}
#[test]
fn png_photo_is_not_a_preview_page_and_disposable_files_are_released() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let r = fixture(&mut s, true);
    let rendered = render::render(&s, &bundle(), t.path(), &r).unwrap();
    assert!(rendered.directory.join("photo.png").exists());
    assert_eq!(rendered.pages.len(), 1);
    assert!(
        rendered.pages[0]
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with("page-")
    );
    let path = rendered.directory.clone();
    drop(rendered);
    assert!(!path.exists());
}
#[test]
fn missing_typesetter_and_invalid_history_pdf_do_not_leave_sensitive_cache() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let r = fixture(&mut s, false);
    assert!(render::render(&s, &t.path().join("missing"), t.path(), &r).is_err());
    assert!(history_pdf::render(t.path(), "bad", b"not a PDF").is_err());
    assert_eq!(fs::read_dir(t.path().join("previews")).unwrap().count(), 0);
}

#[test]
fn missing_glyph_is_rejected_before_export_but_rare_cjk_is_supported() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let mut r = fixture(&mut s, false);
    r.document.profile.name = "𠮷喆龘".into();
    nisaba_cv::font_check::validate(&bundle(), &r.document).unwrap();
    r.document.profile.name = "缺字\u{10FFFF}".into();
    let error = render::render(&s, &bundle(), t.path(), &r)
        .err()
        .unwrap()
        .to_string();
    assert!(error.contains("U+10FFFF"), "{error}");
    assert!(!t.path().join("previews").exists());
    r.document.profile.name = "匿名".into();
    if let ItemContent::Skill { description, .. } = &mut r.document.blocks[0].content {
        *description = "隐藏\u{10FFFF}".into();
    }
    r.document.blocks[0].visible = false;
    nisaba_cv::font_check::validate(&bundle(), &r.document).unwrap();
}
#[test]
fn original_history_preview_does_not_retypeset_or_change_pdf_bytes() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let r = fixture(&mut s, false);
    let rendered = render::render(&s, &bundle(), t.path(), &r).unwrap();
    let bytes = fs::read(rendered.directory.join("resume.pdf")).unwrap();
    let snap = s.record_export(&r, &bytes).unwrap();
    drop(rendered);
    let mut draft = ResumeDraft {
        name: r.name.clone(),
        company: r.company.clone(),
        role: r.role.clone(),
        document: r.document.clone(),
    };
    draft.document.profile.name = "后来修改".into();
    s.save_resume(&r.id, r.revision, draft).unwrap();
    assert_eq!(s.snapshot_pdf(&snap.id).unwrap(), bytes);
    drop(s);
    let mut app = App::open(t.path().to_owned(), t.path().join("no-typst-installed")).unwrap();
    app.open_history(r.id);
    settle(&mut app);
    app.preview_history(snap.id);
    settle(&mut app);
    assert!(app.error.is_empty(), "{}", app.error);
    let path = app.rendered.as_ref().unwrap().directory.clone();
    assert_eq!(fs::read(path.join("resume.pdf")).unwrap(), bytes);
    assert_eq!(app.rendered.as_ref().unwrap().pages.len(), 1);
    app.go(Page::Library);
    assert!(!path.exists());
}
#[test]
fn abandoned_cleanup_requires_our_marker_and_keeps_user_folders() {
    let t = temp();
    let old = render::Rendered::new(t.path(), "x".into(), 1)
        .unwrap()
        .preserve();
    let unknown = t.path().join("previews/nisaba-render-user");
    fs::create_dir_all(&unknown).unwrap();
    fs::write(unknown.join("keep.txt"), b"user file").unwrap();
    render::clean_abandoned(t.path()).unwrap();
    assert!(!old.exists());
    assert!(unknown.join("keep.txt").exists());
}
#[test]
fn preview_replacement_and_failed_export_retry_keep_saved_document() {
    let t = temp();
    let mut s = Store::open(t.path()).unwrap();
    let r = fixture(&mut s, true);
    drop(s);
    let mut app = App::open(t.path().to_owned(), bundle()).unwrap();
    let editor = ResumeEditor::new(r.clone());
    app.open_resume(r.clone());
    app.render_resume(&editor, None);
    settle(&mut app);
    assert!(app.error.is_empty(), "{}", app.error);
    let old = app.rendered.as_ref().unwrap().directory.clone();
    app.render_resume(&editor, None);
    settle(&mut app);
    assert!(!old.exists());
    let target = t.path().join("export.pdf");
    app.render_resume(&editor, Some(target.clone()));
    settle(&mut app);
    assert!(app.error.is_empty(), "{}", app.error);
    assert!(target.exists());
    let stored = app.actor.call(false, move |s| s.resume(&r.id)).unwrap();
    assert_eq!(stored.document, editor.draft.document);
}
