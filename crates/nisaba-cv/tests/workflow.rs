use eframe::egui::{self, epaint::Shape};
use nisaba_cv::{
    app::{App, Cache, Confirmation, Page},
    configure,
    editors::*,
};
use resume_core::{catalog::ProfileEditorDraft, model::*, store::id};
use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

fn setup() -> (tempfile::TempDir, App) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.test-output");
    std::fs::create_dir_all(&root).unwrap();
    let temp = tempfile::TempDir::new_in(root).unwrap();
    let bundle = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtime");
    let app = App::open(temp.path().join("data"), bundle).unwrap();
    (temp, app)
}
fn settle(app: &mut App) {
    let end = Instant::now() + Duration::from_secs(40);
    while app.busy() {
        app.poll();
        assert!(Instant::now() < end, "worker timed out");
        thread::sleep(Duration::from_millis(5));
    }
}
fn skill(app: &mut App, name: &str) -> LibraryItem {
    let name = name.to_string();
    app.actor
        .call(true, move |s| {
            s.save_item_in_category(
                None,
                None,
                "builtin:skill",
                ItemDraft {
                    content: ItemContent::Skill {
                        name,
                        category: String::new(),
                        description: "中文技能说明".into(),
                    },
                    tags: vec![],
                    notes: "内部备注绝不能输出".into(),
                    achievements: vec![],
                },
            )
        })
        .unwrap()
}
fn education(app: &mut App) -> LibraryItem {
    app.actor
        .call(true, |s| {
            s.save_item_in_category(
                None,
                None,
                "builtin:education",
                ItemDraft {
                    content: ItemContent::Education {
                        school: "示例大学".into(),
                        degree: "学士".into(),
                        major: "计算机科学".into(),
                        dates: DateRange::default(),
                    },
                    tags: vec![],
                    notes: String::new(),
                    achievements: vec![AchievementDraft {
                        id: None,
                        text: "匿名学习成果".into(),
                    }],
                },
            )
        })
        .unwrap()
}
fn refresh(app: &mut App) {
    app.cache = app.actor.call(false, |s| Cache::load(s, None)).unwrap();
}
fn frame(
    app: &mut App,
    ctx: &egui::Context,
    events: Vec<egui::Event>,
    size: [f32; 2],
) -> egui::FullOutput {
    let mut output = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0], size[1]),
            )),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| app.draw(ui),
    );
    // Headless tests inspect geometry without uploading atlas pixels to a GPU.
    output.textures_delta.clear();
    output
}
fn texts(output: &egui::FullOutput) -> Vec<(String, egui::Rect)> {
    fn read(shape: &Shape, out: &mut Vec<(String, egui::Rect)>) {
        match shape {
            Shape::Text(t) => out.push((
                t.galley.job.text.clone(),
                egui::Rect::from_min_size(t.pos, t.galley.size()),
            )),
            Shape::Vec(shapes) => {
                for s in shapes {
                    read(s, out)
                }
            }
            _ => {}
        }
    }
    let mut out = vec![];
    for s in &output.shapes {
        read(&s.shape, &mut out)
    }
    out
}
fn pointer(pos: egui::Pos2, pressed: bool) -> egui::Event {
    egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: egui::Modifiers::NONE,
    }
}

#[test]
fn save_confirmation_stays_or_returns_and_survives_reopening() {
    let (temp, mut app) = setup();
    app.go(Page::Profile);
    if let Some(Editor::Profile { draft, .. }) = &mut app.editor {
        draft.name = "匿名用户".into();
        draft.custom_fields.push(CustomField {
            id: id(),
            label: "工作许可".into(),
            value: "可全职".into(),
        })
    }
    assert!(app.dirty());
    app.save(false);
    settle(&mut app);
    assert!(app.error.is_empty());
    assert_eq!(app.page, Page::Profile);
    assert!(!app.dirty());
    assert_eq!(app.toast.as_ref().unwrap().0, "已保存");
    app.save(true);
    settle(&mut app);
    assert_eq!(app.page, Page::Library);
    drop(app);
    let restored = App::open(temp.path().join("data"), PathBuf::new()).unwrap();
    assert_eq!(restored.cache.profile.content.name, "匿名用户");
    assert_eq!(
        restored.cache.profile.content.custom_fields[0].label,
        "工作许可"
    );
}
#[test]
fn invalid_date_preserves_input_and_does_not_report_saved() {
    let (_temp, mut app) = setup();
    let mut e = ItemEditor::new(
        "builtin:education".into(),
        empty_item("education", "教育经历"),
        None,
    );
    if let ItemContent::Education { school, .. } = &mut e.draft.content {
        *school = "示例大学".into()
    };
    e.date.as_mut().unwrap().start = "2025-13".into();
    app.set_editor(Editor::Item(e), Page::Item);
    let before = app.editor.as_ref().unwrap().key();
    app.toast = Some(("已保存".into(), Instant::now()));
    app.save(true);
    assert!(!app.busy());
    assert!(!app.error.is_empty());
    assert!(app.toast.is_none());
    assert_eq!(before, app.editor.as_ref().unwrap().key());
    assert!(app.cache.items.is_empty());
    assert_eq!(app.page, Page::Item);
}
#[test]
fn stale_revision_preserves_draft_until_explicit_reload() {
    let (_temp, mut app) = setup();
    app.go(Page::Profile);
    if let Some(Editor::Profile { draft, .. }) = &mut app.editor {
        draft.name = "未保存输入".into()
    }
    app.actor
        .call(true, |s| {
            let p = s.profile()?;
            s.save_profile_editor(
                ProfileEditorDraft {
                    name: "另一次保存".into(),
                    ..Default::default()
                },
                p.revision,
            )
        })
        .unwrap();
    let before = app.editor.as_ref().unwrap().key();
    app.save(false);
    settle(&mut app);
    assert!(app.error.contains("更新"));
    assert_eq!(before, app.editor.as_ref().unwrap().key());
    assert!(app.dirty());
    app.confirm_action(Confirmation::ReloadEditor);
    settle(&mut app);
    assert!(!app.dirty());
    assert_eq!(app.cache.profile.content.name, "另一次保存");
}
#[test]
fn navigation_requires_explicit_discard_and_picker_is_also_guarded() {
    let (_temp, mut app) = setup();
    app.go(Page::Profile);
    if let Some(Editor::Profile { draft, .. }) = &mut app.editor {
        draft.email = "draft@example.com".into()
    };
    app.navigate(Page::Library);
    assert_eq!(app.page, Page::Profile);
    assert!(matches!(
        app.confirm,
        Some(Confirmation::Navigate(Page::Library))
    ));
    app.confirm = None;
    assert!(app.dirty());
    app.confirm_action(Confirmation::Navigate(Page::Library));
    assert_eq!(app.page, Page::Library);
    app.go(Page::Picker);
    app.picker.name = "未生成的简历".into();
    app.navigate(Page::Resumes);
    assert_eq!(app.page, Page::Picker);
    assert!(app.dirty());
}
#[test]
fn picker_creates_grouped_independent_copies_and_updates_preset_revision() {
    let (_temp, mut app) = setup();
    let a = skill(&mut app, "Rust");
    let ed = education(&mut app);
    let b = skill(&mut app, "SQLite");
    refresh(&mut app);
    app.go(Page::Picker);
    app.picker.name = "软件工程师简历".into();
    app.picker.preset_name = "技术岗位".into();
    for item in [&a, &ed, &b] {
        app.picker.toggle_item(item, true)
    }
    assert_eq!(
        app.picker.selection.sections.as_ref().unwrap()[0]
            .items
            .len(),
        2
    );
    app.save_selection_preset();
    settle(&mut app);
    assert!(app.error.is_empty());
    let first = app.picker.preset_id.clone().unwrap();
    app.save_selection_preset();
    settle(&mut app);
    assert_eq!(app.cache.presets.len(), 1);
    assert_eq!(app.picker.preset_id.as_ref().unwrap().1, first.1 + 1);
    app.create_resume();
    settle(&mut app);
    assert_eq!(app.page, Page::Resume);
    let Editor::Resume(e) = app.editor.as_ref().unwrap() else {
        panic!()
    };
    assert_eq!(e.draft.document.format_version, 2);
    assert_eq!(
        e.draft
            .document
            .blocks
            .iter()
            .map(|b| b.content.title())
            .collect::<Vec<_>>(),
        vec!["Rust", "SQLite", "示例大学"]
    );
    assert!(
        !serde_json::to_string(&e.draft)
            .unwrap()
            .contains("内部备注")
    );
}
#[test]
fn actual_pointer_drag_reorders_picker_groups_and_items() {
    let (_temp, mut app) = setup();
    let a = skill(&mut app, "Rust");
    let ed = education(&mut app);
    let b = skill(&mut app, "SQLite");
    refresh(&mut app);
    app.go(Page::Picker);
    for i in [&a, &ed, &b] {
        app.picker.toggle_item(i, true)
    }
    let ctx = egui::Context::default();
    configure(&ctx, &app.bundle).unwrap();
    let _ = frame(&mut app, &ctx, vec![], [1120., 780.]);
    let out = frame(&mut app, &ctx, vec![], [1120., 780.]);
    let handles = texts(&out)
        .into_iter()
        .filter(|(s, _)| s == "≡")
        .map(|(_, r)| r.center())
        .collect::<Vec<_>>();
    assert_eq!(handles.len(), 6); // Also includes the selected education achievement.
    // Handles: skill group, Rust, SQLite, education group, school.
    let from = handles[0];
    let to = handles[3];
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(from), pointer(from, true)],
        [1120., 780.],
    );
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(from + egui::vec2(10., 0.))],
        [1120., 780.],
    );
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(to)],
        [1120., 780.],
    );
    let _ = frame(&mut app, &ctx, vec![pointer(to, false)], [1120., 780.]);
    assert_eq!(
        app.picker.selection.sections.as_ref().unwrap()[0].category_id,
        "builtin:education"
    );
    let out = frame(&mut app, &ctx, vec![], [1120., 780.]);
    let h = texts(&out)
        .into_iter()
        .filter(|(s, _)| s == "≡")
        .map(|(_, r)| r.center())
        .collect::<Vec<_>>();
    let from = h[3];
    let to = h[4];
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(from), pointer(from, true)],
        [1120., 780.],
    );
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(from + egui::vec2(10., 0.))],
        [1120., 780.],
    );
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(to)],
        [1120., 780.],
    );
    let _ = frame(&mut app, &ctx, vec![pointer(to, false)], [1120., 780.]);
    assert_eq!(
        app.picker.selection.sections.as_ref().unwrap()[1].items[0].item_id,
        b.id
    );
}
#[test]
fn profile_has_no_summary_input_and_education_header_fits_two_rows() {
    let (_temp, mut app) = setup();
    let ctx = egui::Context::default();
    configure(&ctx, &app.bundle).unwrap();
    app.go(Page::Profile);
    let _ = frame(&mut app, &ctx, vec![], [800., 560.]);
    let out = frame(&mut app, &ctx, vec![], [800., 560.]);
    let labels = texts(&out);
    assert!(labels.iter().any(|(s, _)| s == "资料保存在此电脑"));
    assert!(
        !labels
            .iter()
            .any(|(s, _)| s.contains("个人简介") || s.contains("先从自己") || s.contains("从积累"))
    );
    let item = education(&mut app);
    app.open_item(item);
    let _ = frame(&mut app, &ctx, vec![], [800., 560.]);
    let out = frame(&mut app, &ctx, vec![], [800., 560.]);
    let labels = texts(&out);
    let school = labels.iter().find(|(s, _)| s == "学校").unwrap().1;
    let degree = labels.iter().find(|(s, _)| s == "学位").unwrap().1;
    let major = labels.iter().find(|(s, _)| s == "专业").unwrap().1;
    let dates = labels.iter().find(|(s, _)| s == "时间").unwrap().1;
    assert!((school.top() - degree.top()).abs() < 3.);
    assert!((school.top() - major.top()).abs() < 3.);
    assert!(dates.top() - school.top() < 50.);
}
#[test]
fn legacy_conversion_keeps_snapshot_and_restore_opens_separate_draft() {
    let (_temp, mut app) = setup();
    let a = skill(&mut app, "Rust");
    let item = a.id.clone();
    let r = app
        .actor
        .call(true, move |s| {
            s.create_resume(
                "旧简历",
                &Selection {
                    items: vec![SelectionItem {
                        item_id: item,
                        achievement_ids: vec![],
                        include_background: false,
                    }],
                    ..Default::default()
                },
                Style::default(),
            )
        })
        .unwrap();
    app.open_resume(r.clone());
    app.confirm_action(Confirmation::Convert(r.id.clone(), r.revision));
    settle(&mut app);
    assert!(app.error.is_empty());
    let versions = app.actor.call(false, move |s| s.snapshots(&r.id)).unwrap();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].document.format_version, 1);
    let snapshot = versions[0].id.clone();
    let restored = app
        .actor
        .call(true, move |s| {
            s.restore_snapshot_as_resume(&snapshot, "旧版恢复")
        })
        .unwrap();
    assert_eq!(restored.document.format_version, 1);
    assert_ne!(restored.id, versions[0].resume_id);
}
#[test]
fn backup_restore_refreshes_cache_and_preserves_data_after_restart() {
    let (temp, mut app) = setup();
    skill(&mut app, "Rust");
    let package = temp.path().join("saved.rslbackup");
    let p = package.clone();
    let hash = app
        .actor
        .call(false, move |s| {
            s.backup(&p)?;
            Ok(s.inspect_backup(&p)?.package_sha256)
        })
        .unwrap();
    skill(&mut app, "新增后将被恢复替换");
    refresh(&mut app);
    assert_eq!(app.cache.items.len(), 2);
    app.confirm_action(Confirmation::Restore(package, hash));
    assert!(app.busy());
    settle(&mut app);
    assert!(app.error.is_empty());
    assert_eq!(app.cache.items.len(), 1);
    assert_eq!(app.page, Page::Library);
}
#[test]
fn grouped_pdf_preview_export_and_write_failure_preserve_internal_result() {
    let (temp, mut app) = setup();
    let a = skill(&mut app, "Rust");
    let ed = education(&mut app);
    let b = skill(&mut app, "SQLite");
    refresh(&mut app);
    app.go(Page::Picker);
    app.picker.name = "软件工程师简历".into();
    for i in [&a, &ed, &b] {
        app.picker.toggle_item(i, true)
    }
    app.create_resume();
    settle(&mut app);
    let Some(Editor::Resume(e)) = app.editor.clone() else {
        panic!()
    };
    app.render_resume(&e, None);
    settle(&mut app);
    assert!(app.error.is_empty(), "{}", app.error);
    assert!(!app.rendered.as_ref().unwrap().pages.is_empty());
    assert!(app.cache.snapshots.is_empty());
    let target = temp.path().join("简历.pdf");
    app.render_resume(&e, Some(target.clone()));
    settle(&mut app);
    assert!(app.error.is_empty(), "{}", app.error);
    assert!(std::fs::read(target).unwrap().starts_with(b"%PDF-"));
    let broken = temp.path().join("不存在的目录/简历.pdf");
    app.render_resume(&e, Some(broken));
    settle(&mut app);
    assert!(app.error.contains("保存在版本历史"));
    let versions = app.actor.call(false, move |s| s.snapshots(&e.id)).unwrap();
    assert_eq!(versions.len(), 2);
    assert!(versions.iter().all(|v| v.pdf_asset_id.is_some()));
}

#[test]
fn settings_changes_are_guarded_and_async_save_updates_revision() {
    let (_temp, mut app) = setup();
    app.go(Page::Settings);
    app.settings.rolling_keep = 12;
    assert!(app.dirty());
    app.navigate(Page::Library);
    assert_eq!(app.page, Page::Settings);
    app.confirm = None;
    app.save_settings(false);
    settle(&mut app);
    assert!(app.error.is_empty());
    assert!(!app.dirty());
    assert_eq!(app.settings_revision, 1);
    assert_eq!(app.settings.rolling_keep, 12);
    app.navigate(Page::Library);
    assert_eq!(app.page, Page::Library);
}
#[test]
fn editing_invalid_preset_keeps_missing_choices_until_explicit_repair() {
    let (_temp, mut app) = setup();
    let a = skill(&mut app, "Rust");
    refresh(&mut app);
    let mut picker = Picker::default();
    picker.toggle_item(&a, true);
    let selection = picker.selection;
    let p = app
        .actor
        .call(true, move |s| {
            s.save_preset(
                None,
                None,
                PresetDraft {
                    name: "已失效选材".into(),
                    selection,
                    style: Style::default(),
                },
            )
        })
        .unwrap();
    let id = a.id;
    app.actor
        .call(true, move |s| {
            s.set_item_state(&id, a.revision, RecordState::Archived)
        })
        .unwrap();
    refresh(&mut app);
    app.go(Page::Picker);
    app.apply_preset(p.clone());
    assert!(app.error.contains("不可用"));
    assert!(app.picker.selection.selected_items().is_empty());
    app.edit_preset(p.clone());
    assert_eq!(app.picker.selection.selected_items().len(), 1);
    app.repair_picker();
    assert!(app.picker.selection.selected_items().is_empty());
    app.save_selection_preset();
    settle(&mut app);
    assert!(app.error.is_empty());
    assert_eq!(app.cache.presets[0].revision, p.revision + 1);
}
#[test]
fn pointer_drag_in_resume_updates_both_orders_and_persists() {
    let (_temp, mut app) = setup();
    let a = skill(&mut app, "Rust");
    let ed = education(&mut app);
    let b = skill(&mut app, "SQLite");
    refresh(&mut app);
    app.go(Page::Picker);
    app.picker.name = "排序验证".into();
    for i in [&a, &ed, &b] {
        app.picker.toggle_item(i, true)
    }
    app.create_resume();
    settle(&mut app);
    let ctx = egui::Context::default();
    configure(&ctx, &app.bundle).unwrap();
    let _ = frame(&mut app, &ctx, vec![], [1120., 780.]);
    let out = frame(&mut app, &ctx, vec![], [1120., 780.]);
    let h = texts(&out)
        .into_iter()
        .filter(|(s, _)| s == "≡")
        .map(|(_, r)| r.center())
        .collect::<Vec<_>>();
    assert_eq!(h.len(), 5);
    let from = h[0];
    let to = h[3];
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(from), pointer(from, true)],
        [1120., 780.],
    );
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(from + egui::vec2(10., 0.))],
        [1120., 780.],
    );
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(to)],
        [1120., 780.],
    );
    let _ = frame(&mut app, &ctx, vec![pointer(to, false)], [1120., 780.]);
    let Some(Editor::Resume(e)) = &app.editor else {
        panic!()
    };
    assert_eq!(e.draft.document.sections[0].kind, "education");
    assert_eq!(e.draft.document.blocks[0].content.title(), "示例大学");
    assert!(app.dirty());
    app.save(false);
    settle(&mut app);
    assert!(app.error.is_empty());
    assert!(!app.dirty());
    let Some(Editor::Resume(e)) = &app.editor else {
        panic!()
    };
    let id = e.id.clone();
    let saved = app.actor.call(false, move |s| s.resume(&id)).unwrap();
    assert_eq!(saved.document.blocks[0].content.title(), "示例大学");
    assert_eq!(
        app.cache
            .items
            .iter()
            .find(|i| i.id == a.id)
            .unwrap()
            .content
            .title(),
        "Rust"
    );
}
#[test]
fn chinese_composition_and_close_guard_work_through_ui_events() {
    let (_temp, mut app) = setup();
    app.go(Page::Profile);
    let ctx = egui::Context::default();
    configure(&ctx, &app.bundle).unwrap();
    let _ = frame(&mut app, &ctx, vec![], [1120., 780.]);
    let out = frame(&mut app, &ctx, vec![], [1120., 780.]);
    let label = texts(&out)
        .into_iter()
        .find(|(s, _)| s == "姓名")
        .unwrap()
        .1;
    let pos = egui::pos2(label.right() + 35., label.center().y);
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(pos), pointer(pos, true)],
        [1120., 780.],
    );
    let _ = frame(&mut app, &ctx, vec![pointer(pos, false)], [1120., 780.]);
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::Ime(egui::ImeEvent::Preedit {
            text: "林".into(),
            active_range_chars: Some(0..1),
        })],
        [1120., 780.],
    );
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::Ime(egui::ImeEvent::Commit("林晓明".into()))],
        [1120., 780.],
    );
    let Some(Editor::Profile { draft, .. }) = &app.editor else {
        panic!()
    };
    assert_eq!(draft.name, "林晓明");
    assert!(app.dirty());
    let mut input = egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(
            egui::Pos2::ZERO,
            egui::vec2(1120., 780.),
        )),
        ..Default::default()
    };
    input
        .viewports
        .get_mut(&egui::ViewportId::ROOT)
        .unwrap()
        .events
        .push(egui::ViewportEvent::Close);
    let mut out = ctx.run_ui(input, |ui| app.draw(ui));
    out.textures_delta.clear();
    assert!(
        out.viewport_output[&egui::ViewportId::ROOT]
            .commands
            .contains(&egui::ViewportCommand::CancelClose)
    );
    assert!(matches!(app.confirm, Some(Confirmation::Close)));
    assert!(!app.allow_close);
}

#[test]
fn ctrl_s_saves_text_committed_in_the_same_native_frame() {
    let (_temp, mut app) = setup();
    app.go(Page::Profile);
    let ctx = egui::Context::default();
    configure(&ctx, &app.bundle).unwrap();
    let _ = frame(&mut app, &ctx, vec![], [1120., 780.]);
    let out = frame(&mut app, &ctx, vec![], [1120., 780.]);
    let label = texts(&out)
        .into_iter()
        .find(|(s, _)| s == "姓名")
        .unwrap()
        .1;
    let pos = egui::pos2(label.right() + 35., label.center().y);
    let _ = frame(
        &mut app,
        &ctx,
        vec![egui::Event::PointerMoved(pos), pointer(pos, true)],
        [1120., 780.],
    );
    let _ = frame(&mut app, &ctx, vec![pointer(pos, false)], [1120., 780.]);
    let _ = frame(
        &mut app,
        &ctx,
        vec![
            egui::Event::Ime(egui::ImeEvent::Commit("最后提交的中文".into())),
            egui::Event::Key {
                key: egui::Key::S,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL,
            },
        ],
        [1120., 780.],
    );
    settle(&mut app);
    assert!(app.error.is_empty());
    assert_eq!(app.cache.profile.content.name, "最后提交的中文");
    assert!(!app.dirty());
}
