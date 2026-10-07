use crate::editors::*;
use eframe::egui;
use resume_core::{
    Result, Store,
    actor::Actor,
    catalog::{Category, CategoryRevision},
    model::*,
};
use std::{path::PathBuf, sync::mpsc, time::Instant};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Page {
    Library,
    Profile,
    Item,
    Resumes,
    Resume,
    Picker,
    Presets,
    History,
    Settings,
}
#[derive(Clone)]
pub struct Cache {
    pub profile: Profile,
    pub categories: Vec<Category>,
    pub items: Vec<LibraryItem>,
    pub resumes: Vec<Resume>,
    pub presets: Vec<Preset>,
    pub snapshots: Vec<Snapshot>,
}
impl Cache {
    pub fn load(s: &Store, history: Option<&str>) -> Result<Self> {
        let mut items = vec![];
        let mut resumes = vec![];
        for state in [
            RecordState::Active,
            RecordState::Archived,
            RecordState::Trashed,
        ] {
            items.extend(s.items(state, "")?);
            resumes.extend(s.resumes(state)?)
        }
        let mut presets = s.presets(PresetKind::Selection)?;
        presets.extend(s.presets(PresetKind::Layout)?);
        Ok(Self {
            profile: s.profile()?,
            categories: s.categories()?,
            items,
            resumes,
            presets,
            snapshots: history
                .map(|id| s.snapshots(id))
                .transpose()?
                .unwrap_or_default(),
        })
    }
}
pub struct Event {
    pub cache: Cache,
    pub editor: Option<Editor>,
    pub page: Option<Page>,
    pub notice: String,
    pub rendered: Option<crate::render::Rendered>,
    pub saved_preset: Option<Preset>,
    pub settings: Option<(i64, BackupSettings)>,
    pub confirm: Option<Confirmation>,
}
#[derive(Clone)]
pub enum Confirmation {
    Navigate(Page),
    Close,
    TrashItem(String, i64),
    TrashResume(String, i64),
    DeleteCategory(String, i64),
    DeletePreset(String, i64),
    Convert(String, i64),
    Restore(PathBuf, String),
    ReloadEditor,
}
pub struct App {
    pub actor: Actor,
    pub cache: Cache,
    pub data: PathBuf,
    pub bundle: PathBuf,
    pub page: Page,
    pub editor: Option<Editor>,
    pub baseline: String,
    pub picker: Picker,
    pub picker_baseline: String,
    pub pending: Option<mpsc::Receiver<Result<Event>>>,
    pub error: String,
    pub toast: Option<(String, Instant)>,
    pub query: String,
    pub category_filter: Option<String>,
    pub state_filter: RecordState,
    pub category_name: String,
    pub category_kind: String,
    pub renaming: Option<(String, i64, String)>,
    pub confirm: Option<Confirmation>,
    pub history_id: Option<String>,
    pub snapshot_name: String,
    pub copy_name: String,
    pub preview: bool,
    pub icon: Option<egui::TextureHandle>,
    pub settings: BackupSettings,
    pub settings_revision: i64,
    pub settings_baseline: String,
    pub backup_status: String,
    pub allow_close: bool,
    pub rendered: Option<crate::render::Rendered>,
    pub preview_pages: Vec<egui::TextureHandle>,
    pub preview_page: usize,
    pub loaded_preview_page: Option<usize>,
    pub restore_rx: Option<mpsc::Receiver<Result<()>>>,
    pub save_back_categories: std::collections::HashMap<String, String>,
    pub qa: Option<crate::qa::Qa>,
    pub ui_save: Option<bool>,
    pub ui_navigation: Option<Page>,
}
impl App {
    pub fn open(data: PathBuf, bundle: PathBuf) -> Result<Self> {
        let actor = Actor::open(&data)?;
        // The Store lock prevents removing another live instance's previews.
        let _ = crate::render::clean_abandoned(&data);
        let cache = actor.call(false, |s| Cache::load(s, None))?;
        let (settings_revision, settings) = actor.call(false, |s| s.backup_settings())?;
        Ok(Self {
            actor,
            cache,
            data,
            bundle,
            page: Page::Library,
            editor: None,
            baseline: String::new(),
            picker: Picker::default(),
            picker_baseline: String::new(),
            pending: None,
            error: String::new(),
            toast: None,
            query: String::new(),
            category_filter: None,
            state_filter: RecordState::Active,
            category_name: String::new(),
            category_kind: "custom".into(),
            renaming: None,
            confirm: None,
            history_id: None,
            snapshot_name: String::new(),
            copy_name: String::new(),
            preview: false,
            icon: None,
            settings_baseline: serde_json::to_string(&settings).unwrap(),
            settings,
            settings_revision,
            backup_status: String::new(),
            allow_close: false,
            rendered: None,
            preview_pages: vec![],
            preview_page: 0,
            loaded_preview_page: None,
            restore_rx: None,
            save_back_categories: Default::default(),
            qa: None,
            ui_save: None,
            ui_navigation: None,
        })
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.restore_rx.is_some()
    }
    pub fn dirty(&self) -> bool {
        if self.page == Page::Settings {
            serde_json::to_string(&self.settings).unwrap() != self.settings_baseline
        } else if self.page == Page::Picker {
            serde_json::to_string(&self.picker).unwrap() != self.picker_baseline
        } else {
            self.editor
                .as_ref()
                .is_some_and(|e| e.key() != self.baseline)
        }
    }
    pub fn set_editor(&mut self, e: Editor, page: Page) {
        self.clear_preview();
        self.baseline = e.key();
        self.editor = Some(e);
        self.page = page;
        self.error.clear();
        self.preview = false
    }
    pub fn navigate(&mut self, page: Page) {
        if self.busy() {
            return;
        }
        if self.dirty() {
            self.confirm = Some(Confirmation::Navigate(page))
        } else {
            self.go(page)
        }
    }
    pub fn go(&mut self, page: Page) {
        self.clear_preview();
        if self.page == Page::Settings {
            self.settings = serde_json::from_str(&self.settings_baseline).unwrap();
        }
        self.editor = None;
        self.baseline.clear();
        self.error.clear();
        self.query.clear();
        self.preview = false;
        self.page = page;
        self.state_filter = RecordState::Active;
        if page == Page::Profile {
            self.set_editor(
                Editor::Profile {
                    revision: self.cache.profile.revision,
                    draft: profile_draft(&self.cache.profile.content),
                },
                page,
            )
        }
        if page == Page::Picker {
            self.picker = Picker::default();
            self.picker_baseline = serde_json::to_string(&self.picker).unwrap()
        }
    }
    pub fn submit(
        &mut self,
        write: bool,
        action: impl FnOnce(&mut Store) -> Result<(Option<Editor>, Option<Page>, String)>
        + Send
        + 'static,
    ) {
        if self.busy() {
            return;
        }
        self.toast = None;
        let history = self.history_id.clone();
        match self.actor.submit(write, move |s| {
            let (editor, page, notice) = action(s)?;
            Ok(Event {
                cache: Cache::load(s, history.as_deref())?,
                editor,
                page,
                notice,
                rendered: None,
                saved_preset: None,
                settings: None,
                confirm: None,
            })
        }) {
            Ok(rx) => {
                self.pending = Some(rx);
                self.error.clear()
            }
            Err(e) => self.error = e.to_string(),
        }
    }
    pub fn poll(&mut self) {
        if let Some(result) = self.restore_rx.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(Err(resume_core::Error::Unavailable)),
        }) {
            self.restore_rx = None;
            match result {
                Ok(()) => {
                    self.history_id = None;
                    self.submit(false, |s| {
                        s.flush()?;
                        Ok((None, Some(Page::Library), "恢复完成".into()))
                    });
                    if let Ok((rev, settings)) = self.actor.call(false, |s| s.backup_settings()) {
                        self.settings = settings;
                        self.settings_revision = rev;
                        self.settings_baseline = serde_json::to_string(&self.settings).unwrap();
                    }
                }
                Err(e) => {
                    self.toast = None;
                    self.error = e.to_string()
                }
            }
        }
        let result = self.pending.as_ref().and_then(|rx| match rx.try_recv() {
            Ok(r) => Some(r),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(Err(resume_core::Error::Unavailable)),
        });
        if let Some(result) = result {
            self.pending = None;
            match result {
                Ok(e) => {
                    self.cache = e.cache;
                    if let Some((rev, settings)) = e.settings {
                        self.settings_revision = rev;
                        self.settings_baseline = serde_json::to_string(&settings).unwrap();
                        self.settings = settings;
                    }
                    if let Some(confirm) = e.confirm {
                        self.backup_status = e.notice.clone();
                        self.confirm = Some(confirm);
                    }
                    if let Some(rendered) = e.rendered {
                        self.rendered = Some(rendered);
                        self.preview_pages.clear();
                        self.preview_page = 0;
                        self.loaded_preview_page = None;
                        self.preview = true;
                    }
                    if let Some(preset) = e.saved_preset {
                        self.picker.preset_id = Some((preset.id, preset.revision));
                    }
                    if let Some(page) = e.page {
                        self.go(page)
                    }
                    if let Some(editor) = e.editor {
                        let page = match editor {
                            Editor::Profile { .. } => Page::Profile,
                            Editor::Item(_) => Page::Item,
                            Editor::Resume(_) => Page::Resume,
                        };
                        self.set_editor(editor, page)
                    }
                    self.toast = Some((e.notice, Instant::now()));
                }
                Err(e) => {
                    self.toast = None;
                    self.error = e.to_string()
                }
            }
        }
    }
    pub fn save(&mut self, back: bool) {
        self.toast = None;
        if self.page == Page::Settings {
            self.save_settings(back);
            return;
        }
        let Some(editor) = self.editor.clone() else {
            return;
        };
        match editor {
            Editor::Profile { revision, draft } => self.submit(true, move |s| {
                let p = s.save_profile_editor(draft, revision)?;
                Ok((
                    if back {
                        None
                    } else {
                        Some(Editor::Profile {
                            revision: p.revision,
                            draft: profile_draft(&p.content),
                        })
                    },
                    if back { Some(Page::Library) } else { None },
                    "已保存".into(),
                ))
            }),
            Editor::Item(e) => match e.validated() {
                Err(error) => self.error = error.to_string(),
                Ok(draft) => self.submit(true, move |s| {
                    let item = s.save_item_in_category(
                        e.id.as_deref(),
                        e.revision,
                        &e.category_id,
                        draft,
                    )?;
                    Ok((
                        if back {
                            None
                        } else {
                            Some(Editor::Item(ItemEditor::new(
                                item.category_id.clone(),
                                item_draft(&item),
                                Some(&item),
                            )))
                        },
                        if back { Some(Page::Library) } else { None },
                        "已保存".into(),
                    ))
                }),
            },
            Editor::Resume(e) => match e.validated() {
                Err(error) => self.error = error.to_string(),
                Ok(draft) => self.submit(true, move |s| {
                    let r = s.save_resume(&e.id, e.revision, draft)?;
                    Ok((
                        if back {
                            None
                        } else {
                            Some(Editor::Resume(ResumeEditor::new(r).into()))
                        },
                        if back { Some(Page::Resumes) } else { None },
                        "已保存".into(),
                    ))
                }),
            },
        }
    }
    pub fn open_item(&mut self, item: LibraryItem) {
        self.set_editor(
            Editor::Item(ItemEditor::new(
                item.category_id.clone(),
                item_draft(&item),
                Some(&item),
            )),
            Page::Item,
        )
    }
    pub fn save_selection_preset(&mut self) {
        self.toast = None;
        if self
            .picker
            .selection
            .profile_fields
            .contains(&ProfileField::Summary)
        {
            self.error = "请先修复旧预设中的个人简介选项".into();
            return;
        }
        if self.busy() {
            return;
        }
        let p = self.picker.clone();
        let history = self.history_id.clone();
        match self.actor.submit(true, move |s| {
            let (id, rev) = p
                .preset_id
                .as_ref()
                .map(|(id, rev)| (Some(id.as_str()), Some(*rev)))
                .unwrap_or((None, None));
            let preset = s.save_preset(
                id,
                rev,
                PresetDraft {
                    name: p.preset_name,
                    selection: p.selection,
                    style: p.style,
                },
            )?;
            Ok(Event {
                cache: Cache::load(s, history.as_deref())?,
                editor: None,
                page: None,
                notice: "选材预设已保存".into(),
                rendered: None,
                saved_preset: Some(preset),
                settings: None,
                confirm: None,
            })
        }) {
            Ok(rx) => {
                self.pending = Some(rx);
                self.error.clear()
            }
            Err(e) => self.error = e.to_string(),
        }
    }
    pub fn reload_editor(&mut self) {
        let Some(editor) = self.editor.clone() else {
            return;
        };
        self.submit(false, move |s| {
            let editor = match editor {
                Editor::Profile { .. } => {
                    let p = s.profile()?;
                    Editor::Profile {
                        revision: p.revision,
                        draft: profile_draft(&p.content),
                    }
                }
                Editor::Item(e) => {
                    let i = s
                        .item(e.id.as_deref().ok_or_else(|| {
                            resume_core::Error::Invalid("新素材尚未保存".into())
                        })?)?;
                    Editor::Item(ItemEditor::new(
                        i.category_id.clone(),
                        item_draft(&i),
                        Some(&i),
                    ))
                }
                Editor::Resume(e) => Editor::Resume(ResumeEditor::new(s.resume(&e.id)?).into()),
            };
            Ok((Some(editor), None, "已重新加载".into()))
        })
    }
    pub fn save_settings(&mut self, back: bool) {
        if self.busy() {
            return;
        }
        let settings = self.settings.clone();
        let rev = self.settings_revision;
        let history = self.history_id.clone();
        match self.actor.submit(true, move |s| {
            let settings = s.save_backup_settings(rev, settings)?;
            Ok(Event {
                cache: Cache::load(s, history.as_deref())?,
                editor: None,
                page: if back { Some(Page::Library) } else { None },
                notice: "备份设置已保存".into(),
                rendered: None,
                saved_preset: None,
                settings: Some(settings),
                confirm: None,
            })
        }) {
            Ok(rx) => {
                self.pending = Some(rx);
                self.error.clear()
            }
            Err(e) => self.error = e.to_string(),
        }
    }
    pub fn inspect_restore(&mut self, path: PathBuf) {
        if self.busy() {
            return;
        }
        let history = self.history_id.clone();
        match self.actor.submit(false, move |s| {
            let p = s.inspect_backup(&path)?;
            let notice = format!(
                "备份含 {} 条素材、{} 份简历、{} 个历史版本。",
                p.manifest.counts.items, p.manifest.counts.resumes, p.manifest.counts.snapshots
            );
            Ok(Event {
                cache: Cache::load(s, history.as_deref())?,
                editor: None,
                page: None,
                notice,
                rendered: None,
                saved_preset: None,
                settings: None,
                confirm: Some(Confirmation::Restore(path, p.package_sha256)),
            })
        }) {
            Ok(rx) => {
                self.pending = Some(rx);
                self.error.clear()
            }
            Err(e) => self.error = e.to_string(),
        }
    }
    pub fn render_resume(&mut self, e: &ResumeEditor, target: Option<PathBuf>) {
        if self.busy() {
            return;
        }
        self.toast = None;
        let id = e.id.clone();
        let revision = e.revision;
        let bundle = self.bundle.clone();
        let root = self.data.clone();
        let history = self.history_id.clone();
        // Capture the target state before compilation so a later external modification
        // cannot be silently overwritten by this export.
        let expected = match target
            .as_ref()
            .map(|p| resume_core::files::pdf_target_hash(p))
            .transpose()
        {
            Ok(v) => v.flatten(),
            Err(e) => {
                self.error = e.to_string();
                return;
            }
        };
        match self.actor.submit(target.is_some(), move |s| {
            let frozen = s.capture_export(&id, revision)?;
            let rendered = crate::render::render(s, &bundle, &root, &frozen)?;
            let notice = if let Some(path) = target {
                let pdf = std::fs::read(rendered.directory.join("resume.pdf"))?;
                s.record_export(&frozen, &pdf)?;
                resume_core::files::publish_pdf(&path, &pdf, expected.as_deref()).map_err(|e| {
                    resume_core::Error::Invalid(format!(
                        "PDF 已保存在版本历史中；目标文件写入失败：{e}"
                    ))
                })?;
                format!("PDF 已导出：{}", path.display())
            } else {
                "预览已更新".into()
            };
            Ok(Event {
                cache: Cache::load(s, history.as_deref())?,
                editor: None,
                page: None,
                notice,
                rendered: Some(rendered),
                saved_preset: None,
                settings: None,
                confirm: None,
            })
        }) {
            Ok(rx) => {
                self.pending = Some(rx);
                self.error.clear()
            }
            Err(e) => self.error = e.to_string(),
        }
    }
    pub fn open_resume(&mut self, resume: Resume) {
        self.copy_name = format!("{} 副本", resume.name);
        self.snapshot_name.clear();
        self.set_editor(
            Editor::Resume(ResumeEditor::new(resume).into()),
            Page::Resume,
        )
    }
    pub fn clear_preview(&mut self) {
        self.rendered = None;
        self.preview_pages.clear();
        self.loaded_preview_page = None;
        self.preview_page = 0;
    }
    pub fn preview_history(&mut self, snapshot_id: String) {
        if self.busy() {
            return;
        }
        self.toast = None;
        let root = self.data.clone();
        let history = self.history_id.clone();
        match self.actor.submit(false, move |s| {
            let pdf = s.snapshot_pdf(&snapshot_id)?;
            let rendered = crate::history_pdf::render(&root, &snapshot_id, &pdf)?;
            Ok(Event {
                cache: Cache::load(s, history.as_deref())?,
                editor: None,
                page: None,
                notice: "已打开原始历史 PDF".into(),
                rendered: Some(rendered),
                saved_preset: None,
                settings: None,
                confirm: None,
            })
        }) {
            Ok(rx) => {
                self.pending = Some(rx);
                self.error.clear();
            }
            Err(e) => self.error = e.to_string(),
        }
    }
    pub fn open_history(&mut self, id: String) {
        self.history_id = Some(id.clone());
        self.submit(false, move |_| {
            Ok((None, Some(Page::History), "已加载历史".into()))
        })
    }
    pub fn create_resume(&mut self) {
        if self
            .picker
            .selection
            .profile_fields
            .contains(&ProfileField::Summary)
        {
            self.error = "请先编辑并修复旧预设中的个人简介选项".into();
            return;
        }
        let p = self.picker.clone();
        self.submit(true, move |s| {
            let r = if let Some((id, rev)) = p.append_to {
                s.add_resume_selection(&id, rev, &p.selection)?
            } else {
                s.create_resume_target(&p.name, &p.company, &p.role, &p.selection, p.style)?
            };
            Ok((
                Some(Editor::Resume(ResumeEditor::new(r).into())),
                None,
                "已生成简历".into(),
            ))
        })
    }
    pub fn reorder_categories(&mut self, from: usize, to: usize) {
        let mut order = self
            .cache
            .categories
            .iter()
            .map(|c| CategoryRevision {
                id: c.id.clone(),
                revision: c.revision,
            })
            .collect::<Vec<_>>();
        move_index(&mut order, from, to);
        self.submit(true, move |s| {
            s.reorder_categories(&order)?;
            Ok((None, None, "类别顺序已保存".into()))
        })
    }
    pub fn confirm_action(&mut self, c: Confirmation) {
        match c {
            Confirmation::Navigate(p) => self.go(p),
            Confirmation::Close => self.allow_close = true,
            Confirmation::ReloadEditor => self.reload_editor(),
            Confirmation::TrashItem(id, rev) => self.submit(true, move |s| {
                s.set_item_state(&id, rev, RecordState::Trashed)?;
                Ok((None, None, "已移入回收站".into()))
            }),
            Confirmation::TrashResume(id, rev) => self.submit(true, move |s| {
                s.set_resume_state(&id, rev, RecordState::Trashed)?;
                Ok((None, Some(Page::Resumes), "已移入回收站".into()))
            }),
            Confirmation::DeleteCategory(id, rev) => self.submit(true, move |s| {
                s.delete_category(&id, rev)?;
                Ok((None, None, "类别已删除".into()))
            }),
            Confirmation::DeletePreset(id, rev) => self.submit(true, move |s| {
                s.delete_preset(&id, rev)?;
                Ok((None, None, "预设已删除".into()))
            }),
            Confirmation::Convert(id, rev) => self.submit(true, move |s| {
                let r = s.convert_resume_to_grouped(&id, rev)?;
                Ok((
                    Some(Editor::Resume(ResumeEditor::new(r).into())),
                    None,
                    "已分组，转换前版本已保存".into(),
                ))
            }),
            Confirmation::Restore(path, hash) => {
                match self.actor.acknowledge_saved() {
                    Err(e) => self.error = e.to_string(),
                    Ok(()) => {
                        // Actor freezes subsequent submissions while the restore runs.
                        match self.actor.restore(
                            path,
                            hash,
                            resume_core::backup::RestoreFault::None,
                        ) {
                            Err(e) => self.error = e.to_string(),
                            Ok(rx) => self.restore_rx = Some(rx),
                        }
                    }
                }
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui)
    }
}
