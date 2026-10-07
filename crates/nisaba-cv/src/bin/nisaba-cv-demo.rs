use nisaba_cv::{editors::*, render};
use resume_core::{Store, model::*, store::id};
use std::{fs, path::PathBuf};
fn run() -> Result<(), Box<dyn std::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.len() != 2 {
        return Err("用法：nisaba-cv-demo <空数据目录> <程序包目录>".into());
    }
    let root = PathBuf::from(&args[0]);
    if root.exists() && fs::read_dir(&root)?.next().is_some() {
        return Err("演示只允许使用空目录".into());
    }
    let mut store = Store::open(&root)?;
    let p = store.profile()?;
    let mut profile = profile_draft(&p.content);
    profile.name = "林晓明".into();
    profile.title = "软件开发工程师".into();
    profile.email = "lin@example.com".into();
    profile.phone = "138 0000 0000".into();
    profile.location = "上海".into();
    profile.custom_fields = vec![CustomField {
        id: id(),
        label: "工作许可".into(),
        value: "可全职".into(),
    }];
    store.save_profile_editor(profile, p.revision)?;
    let certificate = store.create_category("资格与证书", "custom")?;
    let mut items = vec![];
    for (kind, category, title) in [
        ("skill", "builtin:skill", "Rust"),
        ("education", "builtin:education", "示例大学"),
        ("skill", "builtin:skill", "SQLite"),
        ("project", "builtin:project", "本地资料管理"),
        ("custom", certificate.id.as_str(), "示例资格证书"),
    ] {
        let mut draft = empty_item(kind, "资格与证书");
        match &mut draft.content {
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
            ItemContent::Skill {
                name, description, ..
            } => {
                *name = title.into();
                *description = "熟悉本地数据管理与应用开发。".into();
            }
            ItemContent::Project {
                name,
                role,
                background,
                ..
            } => {
                *name = title.into();
                *role = "开发者".into();
                *background = "离线维护个人资料，根据岗位挑选素材。".into();
            }
            ItemContent::Custom {
                title: t, subtitle, ..
            } => {
                *t = title.into();
                *subtitle = "匿名演示证书".into();
            }
            _ => {}
        }
        if kind != "skill" {
            draft.achievements.push(AchievementDraft {
                id: None,
                text: "匿名演示资料，用于验证填写、选材与排序。".into(),
            })
        }
        draft.notes = "内部备注，不进入简历或 PDF。".into();
        items.push(store.save_item_in_category(None, None, category, draft)?);
    }
    let mut picker = Picker {
        name: "软件工程师简历".into(),
        ..Default::default()
    };
    for i in &items {
        picker.toggle_item(i, true)
    }
    picker.selection.custom_field_ids = store
        .profile()?
        .content
        .custom_fields
        .iter()
        .map(|f| f.id.clone())
        .collect();
    let r = store.create_resume(&picker.name, &picker.selection, picker.style.clone())?;
    store.save_preset(
        None,
        None,
        PresetDraft {
            name: "技术岗位".into(),
            selection: picker.selection,
            style: picker.style,
        },
    )?;
    store.create_snapshot(&r.id, r.revision, "初稿", None)?;
    let rendered = render::render(&store, &PathBuf::from(&args[1]), &root, &r)?;
    let pdf = fs::read(rendered.directory.join("resume.pdf"))?;
    store.record_export(&r, &pdf)?;
    fs::write(root.join("示例简历.pdf"), pdf)?;
    store.backup(&root.join("示例完整备份.rslbackup"))?;
    fs::write(
        root.join("演示结果.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"resumeId":r.id,"sections":r.document.sections.iter().map(|s|&s.title).collect::<Vec<_>>(),"pages":rendered.pages.len(),"renderDirectory":rendered.directory,"anonymous":true}),
        )?,
    )?;
    rendered.preserve();
    println!("{}", root.display());
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1)
    }
}
