//! Opt-in native GPU capture harness; never enabled in normal use.
use crate::{
    app::{App, Page},
    editors::Picker,
};
use eframe::egui;
use std::{
    fs,
    path::PathBuf,
    time::{Duration, Instant},
};
pub struct Qa {
    pub directory: PathBuf,
    index: usize,
    requested: bool,
    at: Instant,
    pub results: Vec<serde_json::Value>,
}
const PHASES: [(&str, Page, [f32; 2]); 8] = [
    ("library", Page::Library, [1120., 780.]),
    ("profile", Page::Profile, [1120., 780.]),
    ("education", Page::Item, [800., 560.]),
    ("picker", Page::Picker, [1120., 780.]),
    ("resume", Page::Resume, [1120., 780.]),
    ("saved", Page::Profile, [1120., 780.]),
    ("pdf", Page::Resume, [1120., 780.]),
    ("history-pdf", Page::History, [1120., 780.]),
];
impl Qa {
    pub fn new(directory: PathBuf) -> std::io::Result<Self> {
        fs::create_dir_all(&directory)?;
        Ok(Self {
            directory,
            index: 0,
            requested: false,
            at: Instant::now(),
            results: vec![],
        })
    }
}
impl App {
    pub fn qa_tick(&mut self, ctx: &egui::Context) {
        let Some(mut qa) = self.qa.take() else { return };
        if qa.at.elapsed() > Duration::from_secs(30) {
            let _ = fs::write(qa.directory.join("error.txt"), "原生渲染截图超时");
            self.allow_close = true;
            return;
        }
        let screenshots = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| {
                    if let egui::Event::Screenshot { image, .. } = e {
                        Some(image.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
        });
        for image in screenshots {
            let path = qa.directory.join(format!("{}.png", PHASES[qa.index].0));
            let bytes = image
                .pixels
                .iter()
                .flat_map(|p| p.to_array())
                .collect::<Vec<_>>();
            let result =
                image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, bytes)
                    .ok_or("截图尺寸无效")
                    .and_then(|img| img.save(&path).map_err(|_| "截图写入失败"));
            if let Err(e) = result {
                self.error = e.into();
                self.qa = Some(qa);
                return;
            }
            qa.results.push(serde_json::json!({"page":PHASES[qa.index].0,"path":path,"size":image.size,"error":self.error,"busy":self.busy(),"zoomFactor":ctx.zoom_factor(),"previewPages":self.rendered.as_ref().map(|r|r.pages.len()),"loadedTextures":self.preview_pages.len()}));
            qa.index += 1;
            if qa.index == PHASES.len() {
                let _ = fs::write(
                    qa.directory.join("result.json"),
                    serde_json::to_vec_pretty(&qa.results).unwrap(),
                );
                self.allow_close = true;
                return;
            }
            let (_, page, size) = PHASES[qa.index];
            self.go(page);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(
                size[0], size[1],
            )));
            match page {
                Page::Item => {
                    if let Some(item) = self
                        .cache
                        .items
                        .iter()
                        .find(|i| i.content.kind() == "education")
                        .cloned()
                    {
                        self.open_item(item)
                    }
                }
                Page::Picker => {
                    self.picker = Picker {
                        name: "软件工程师简历".into(),
                        ..Default::default()
                    };
                    for item in &self.cache.items {
                        self.picker.toggle_item(item, true)
                    }
                    self.picker_baseline = serde_json::to_string(&self.picker).unwrap()
                }
                Page::Resume => {
                    if let Some(r) = self.cache.resumes.first().cloned() {
                        self.open_resume(r.clone());
                        if qa.index == 6 {
                            self.render_resume(
                                &crate::editors::ResumeEditor::new(r),
                                Some(self.data.join("native-check.pdf")),
                            );
                        }
                    }
                }
                Page::History => {
                    if let Some(r) = self.cache.resumes.first() {
                        self.open_history(r.id.clone());
                    }
                }
                Page::Profile if qa.index == 5 => self.save(false),
                _ => {}
            }
            qa.at = Instant::now();
            qa.requested = false;
        }
        if qa.index == 7
            && !self.busy()
            && !self.preview
            && self.error.is_empty()
            && let Some(snapshot) = self
                .cache
                .snapshots
                .iter()
                .find(|s| s.pdf_asset_id.is_some())
        {
            self.preview_history(snapshot.id.clone());
        }
        let saving_animation = qa.index == 5
            && self
                .toast
                .as_ref()
                .is_none_or(|(_, at)| at.elapsed() < Duration::from_millis(350));
        if !self.busy()
            && !qa.requested
            && !saving_animation
            && qa.at.elapsed() > Duration::from_millis(350)
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            qa.requested = true;
        }
        ctx.request_repaint_after(Duration::from_millis(60));
        self.qa = Some(qa);
    }
}

/// Explicit, anonymous release-binary smoke check for extracted offline packages.
/// Refuses any nonempty directory and never discovers or migrates personal data.
pub fn self_test(root: &std::path::Path, bundle: &std::path::Path) -> resume_core::Result<()> {
    use resume_core::{Store, model::*, store::id};
    if root.exists() && fs::read_dir(root)?.next().is_some() {
        return Err(resume_core::Error::Invalid("自测只允许使用空目录".into()));
    }
    let mut s = Store::open(root)?;
    let p = s.profile()?;
    s.save_profile(
        ProfileDraft {
            name: "匿名𠮷测试".into(),
            title: "软件开发工程师".into(),
            custom_fields: vec![CustomField {
                id: id(),
                label: "工作许可".into(),
                value: "可全职".into(),
            }],
            ..Default::default()
        },
        p.revision,
    )?;
    let item = s.save_item_in_category(
        None,
        None,
        "builtin:skill",
        ItemDraft {
            content: ItemContent::Skill {
                name: "Rust".into(),
                category: String::new(),
                description: "离线资料与 PDF 验收".into(),
            },
            tags: vec![],
            notes: "内部备注".into(),
            achievements: vec![],
        },
    )?;
    let mut picker = Picker::default();
    picker.toggle_item(&item, true);
    picker.selection.custom_field_ids = s
        .profile()?
        .content
        .custom_fields
        .iter()
        .map(|f| f.id.clone())
        .collect();
    let r = s.create_resume("离线验收", &picker.selection, Style::default())?;
    let rendered = crate::render::render(&s, bundle, root, &r)?;
    let pdf = fs::read(rendered.directory.join("resume.pdf"))?;
    let snap = s.record_export(&r, &pdf)?;
    let old = crate::history_pdf::render(root, &snap.id, &s.snapshot_pdf(&snap.id)?)?;
    if fs::read(old.directory.join("resume.pdf"))? != pdf {
        return Err(resume_core::Error::Invalid("历史 PDF 字节不一致".into()));
    }
    let backup = root.join("anonymous.rslbackup");
    s.backup(&backup)?;
    let inspected = s.inspect_backup(&backup)?;
    let pages = rendered.pages.len();
    let history_pages = old.pages.len();
    drop(old);
    drop(rendered);
    drop(s);
    let s = Store::open(root)?;
    let reopened = s.resume(&r.id)?;
    if reopened.document != r.document {
        return Err(resume_core::Error::Invalid("重开内容不一致".into()));
    }
    fs::write(
        root.join("self-test.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"passed":true,"anonymous":true,"pages":pages,"historyPages":history_pages,"historyOriginalBytes":true,"savedAndReopened":true,"backupSchema":inspected.manifest.schema_version,"managedPreviewDirectories":fs::read_dir(root.join("previews"))?.count(),"physicalNetworkDisconnect":false}),
        )?,
    )?;
    Ok(())
}
