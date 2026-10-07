//! Isolated developer acceptance runner. Never shipped in the user bundle.
use nisaba_cv::{
    app::{App, Cache},
    editors::*,
    history_pdf, render,
};
use resume_core::{Store, model::*, store::id};
use std::{
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

type R<T> = Result<T, Box<dyn std::error::Error>>;
fn base(store: &mut Store) -> R<Resume> {
    let p = store.profile()?;
    store.save_profile(
        ProfileDraft {
            name: "林晓明".into(),
            title: "软件开发工程师".into(),
            phone: "138 0000 0000".into(),
            email: "lin@example.com".into(),
            location: "上海".into(),
            links: vec![Link {
                label: "作品集".into(),
                url: "https://example.com/portfolio".into(),
            }],
            custom_fields: vec![CustomField {
                id: id(),
                label: "工作许可".into(),
                value: "可全职".into(),
            }],
            ..Default::default()
        },
        p.revision,
    )?;
    let mut picker = Picker {
        name: "软件工程师简历".into(),
        ..Default::default()
    };
    for (kind, title) in [
        ("skill", "Rust"),
        ("education", "示例大学"),
        ("skill", "SQLite"),
        ("project", "离线资料管理"),
        ("work", "示例公司"),
        ("custom", "示例证书"),
    ] {
        let mut d = empty_item(kind, "资格证书");
        match &mut d.content {
            ItemContent::Skill {
                name, description, ..
            } => {
                *name = title.into();
                *description = "熟悉本地数据管理与图形应用开发。".into();
            }
            ItemContent::Education {
                school,
                degree,
                major,
                dates,
            } => {
                *school = title.into();
                *degree = "学士".into();
                *major = "计算机科学与技术".into();
                dates.start = Some(PartialDate {
                    year: 2020,
                    month: Some(9),
                });
                dates.end = Some(PartialDate {
                    year: 2024,
                    month: Some(6),
                });
            }
            ItemContent::Project {
                name,
                role,
                background,
                url,
                ..
            } => {
                *name = title.into();
                *role = "开发者".into();
                *background = "独立维护可复用素材，按岗位生成简历。".into();
                *url = Some(Link {
                    label: "源代码".into(),
                    url: "https://example.com/source".into(),
                });
            }
            ItemContent::Work {
                company,
                role,
                dates,
                ..
            } => {
                *company = title.into();
                *role = "软件工程师".into();
                dates.start = Some(PartialDate {
                    year: 2024,
                    month: None,
                });
                dates.ongoing = true;
            }
            ItemContent::Custom {
                title: t, subtitle, ..
            } => {
                *t = title.into();
                *subtitle = "资格与证书".into();
            }
        }
        if kind != "skill" {
            d.achievements.push(AchievementDraft {
                id: None,
                text: "完成匿名验收样例，提升资料选择与复用效率。".into(),
            });
        }
        d.notes = "PRIVATE_NOTE_MUST_NOT_EXPORT".into();
        let item = store.save_item_in_category(None, None, &format!("builtin:{kind}"), d)?;
        picker.toggle_item(&item, true);
    }
    picker.selection.custom_field_ids = store
        .profile()?
        .content
        .custom_fields
        .iter()
        .map(|f| f.id.clone())
        .collect();
    Ok(store.create_resume(&picker.name, &picker.selection, Style::default())?)
}
fn matrix(root: &Path, bundle: &Path) -> R<()> {
    let mut s = Store::open(root.join("data"))?;
    let base = base(&mut s)?;
    let mut cases = vec![("basic", base.clone())];
    let mut photo = base.clone();
    let mut png = std::io::Cursor::new(Vec::new());
    image::RgbaImage::from_pixel(120, 160, image::Rgba([24, 100, 130, 255]))
        .write_to(&mut png, image::ImageFormat::Png)?;
    let p = s.import_profile_photo(s.profile()?.revision, &png.into_inner())?;
    photo.document.profile.photo_asset_id = p.content.photo_asset_id;
    cases.push(("photo", photo.clone()));
    let mut long = photo;
    long.document.profile.title = "软件开发工程师：桌面应用、本地数据与排版工具".repeat(4);
    long.document.profile.location = "中国上海市，支持跨地区远程协作与本地工作".repeat(4);
    long.document.profile.custom_fields[0].value =
        "自定义字段内容需要跨行，全部保留不能裁切。".repeat(85);
    long.document
        .blocks
        .iter_mut()
        .find(|b| b.content.kind() == "project")
        .unwrap()
        .achievements[0]
        .text = "完整长段落：验证连续中文在页面边界自动换页，不应遗漏文字或挤出页面。".repeat(160);
    cases.push(("long-photo", long));
    let mut ten = base.clone();
    let block = ten
        .document
        .blocks
        .iter_mut()
        .find(|b| b.content.kind() == "project")
        .unwrap();
    block.achievements = (1..=26)
        .map(|i| ResumeAchievement {
            id: id(),
            source_id: String::new(),
            text: format!(
                "段落{i:02}开始。{}段落{i:02}结束。",
                "验证跨页内容完整，保持各条成果顺序与可搜索性。".repeat(45)
            ),
        })
        .collect();
    cases.push(("many-pages", ten));
    let mut breaks = base.clone();
    for b in &mut breaks.document.blocks {
        b.page_break_before = true;
    }
    breaks.document.blocks[1].visible = false; // Hidden SQLite is absent, category remains once.
    cases.push(("page-breaks", breaks));
    let mut minimum = base.clone();
    minimum.document.style.font_size = 9.;
    minimum.document.style.margin_mm = 10.;
    minimum.document.style.line_height = 1.2;
    minimum.document.style.section_gap_mm = 2.;
    cases.push(("minimum", minimum));
    let mut maximum = base.clone();
    maximum.document.style.font_family = "resume-serif-sc".into();
    maximum.document.style.font_size = 16.;
    maximum.document.style.margin_mm = 30.;
    maximum.document.style.line_height = 2.;
    maximum.document.style.section_gap_mm = 12.;
    cases.push(("maximum-serif", maximum));
    let mut url = base.clone();
    url.document.profile.links[0].url = format!("https://example.com/{}", "longpath".repeat(90));
    if let ItemContent::Skill { description, .. } = &mut url.document.blocks[0].content {
        *description =
            "C++ C# .NET <tag> & [ ] #import(\"@preview/evil:1.0.0\") 𠮷 喆 龘 café naïve".into();
    }
    cases.push(("url-unicode", url));
    let mut legacy = base.clone();
    legacy.document.format_version = 1;
    legacy.document.sections.clear();
    legacy.document.profile.custom_fields.clear();
    legacy.document.profile.summary = "旧版简介原文应保留。".into();
    legacy
        .document
        .style
        .module_titles
        .insert("education".into(), "学习经历".into());
    cases.push(("legacy", legacy));
    let mut report = vec![];
    for (name, mut r) in cases {
        r.name = format!("验收：{name}");
        let start = Instant::now();
        match render::render(&s, bundle, root, &r) {
            Ok(rendered) => {
                let pages = rendered.pages.len();
                let paths = rendered.pages.clone();
                let directory = rendered.preserve();
                report.push(serde_json::json!({"case":name,"ok":true,"directory":directory,"pages":pages,"pagePaths":paths,"ms":start.elapsed().as_millis()}));
                println!("{name}: {pages} pages");
            }
            Err(e) => {
                println!("{name}: FAILED: {e}");
                report.push(serde_json::json!({"case":name,"ok":false,"error":e.to_string()}));
            }
        }
    }
    let passed = report.iter().all(|r| r["ok"] == true);
    fs::write(
        root.join("matrix.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    if !passed {
        return Err("PDF matrix failed".into());
    }
    let first = PathBuf::from(report[0]["directory"].as_str().unwrap());
    let pdf = fs::read(first.join("resume.pdf"))?;
    let old = history_pdf::render(root, "anonymous-original", &pdf)?;
    assert_eq!(fs::read(old.directory.join("resume.pdf"))?, pdf);
    let original_pages = old.pages.len();
    let original_directory = old.preserve();
    fs::write(
        root.join("history.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"originalBytesUnchanged":true,"pages":original_pages,"directory":original_directory}),
        )?,
    )?;
    Ok(())
}
fn performance(root: &Path, bundle: &Path) -> R<()> {
    let data = root.join("data");
    let mut s = Store::open(&data)?;
    let first = base(&mut s)?;
    for i in 0..1000 {
        s.save_item_in_category(
            None,
            None,
            "builtin:skill",
            ItemDraft {
                content: ItemContent::Skill {
                    name: format!("技能{i:04}"),
                    category: String::new(),
                    description: "本地搜索与资料复用。".repeat(20),
                },
                tags: vec!["性能验收".into()],
                notes: "匿名数据".into(),
                achievements: vec![],
            },
        )?;
    }
    for i in 0..100 {
        s.copy_resume(&first.id, &format!("岗位{i:03}"))?;
    }
    drop(s);
    let start = Instant::now();
    let mut app = App::open(data, bundle.to_owned())?;
    let open_ms = start.elapsed().as_millis();
    let mut loads = vec![];
    for _ in 0..10 {
        let at = Instant::now();
        app.cache = app.actor.call(false, |s| Cache::load(s, None))?;
        loads.push(at.elapsed().as_millis());
    }
    let mut saves = vec![];
    for i in 0..10 {
        app.go(nisaba_cv::app::Page::Profile);
        if let Some(Editor::Profile { draft, .. }) = &mut app.editor {
            draft.name = format!("匿名{i}");
        }
        let at = Instant::now();
        app.save(false);
        while app.busy() {
            app.poll();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(app.error.is_empty(), "{}", app.error);
        saves.push(at.elapsed().as_millis());
    }
    let ctx = eframe::egui::Context::default();
    nisaba_cv::configure(&ctx, bundle)?;
    app.go(nisaba_cv::app::Page::Library);
    let mut frames = vec![];
    for _ in 0..20 {
        let at = Instant::now();
        let input = eframe::egui::RawInput {
            screen_rect: Some(eframe::egui::Rect::from_min_size(
                eframe::egui::Pos2::ZERO,
                eframe::egui::vec2(1120., 780.),
            )),
            ..Default::default()
        };
        let _ = ctx.run_ui(input, |ui| app.draw(ui));
        frames.push(at.elapsed().as_millis());
    }
    fs::write(
        root.join("performance.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"items":app.cache.items.len(),"resumes":app.cache.resumes.len(),"openMs":open_ms,"cacheLoadMs":loads,"saveAndRefreshMs":saves,"syntheticUiFrameMs":frames,"release":!cfg!(debug_assertions),"physicalLongRunningTest":false}),
        )?,
    )?;
    Ok(())
}
fn run() -> R<()> {
    let project = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_owned();
    let mode = std::env::args().nth(1).unwrap_or_else(|| "matrix".into());
    let root = project
        .join(".test-output")
        .join(format!("{mode}-{}", id()));
    fs::create_dir_all(&root)?;
    let bundle = project.join("runtime");
    let result = match mode.as_str() {
        "matrix" => matrix(&root, &bundle),
        "performance" => performance(&root, &bundle),
        _ => Err("unknown mode".into()),
    };
    fs::write(
        project.join(format!(".test-output/{mode}-location.json")),
        serde_json::to_vec_pretty(&serde_json::json!({"directory":root,"ok":result.is_ok()}))?,
    )?;
    result
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1)
    }
}
