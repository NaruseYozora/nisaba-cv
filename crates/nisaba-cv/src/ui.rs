use crate::{app::*, editors::*, widgets::*};
use eframe::egui;
use resume_core::model::*;
use std::{fs, time::Duration};

impl App {
    pub fn draw(&mut self, root: &mut egui::Ui) {
        let ctx = root.ctx().clone();
        self.poll();
        self.qa_tick(&ctx);
        if self.allow_close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close)
        }
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::CTRL, egui::Key::S))
            && !self.busy()
            && self.confirm.is_none()
        {
            self.ui_save = Some(false);
        }
        egui::Panel::left("navigation")
            .resizable(false)
            .default_size(160.)
            .show(root, |ui| {
                ui.add_space(12.);
                ui.horizontal(|ui| {
                    if let Some(icon) = &self.icon {
                        ui.add(egui::Image::new(icon).fit_to_exact_size(egui::vec2(28., 28.)));
                    }
                    ui.strong("Nisaba CV");
                });
                ui.add_space(18.);
                ui.add_enabled_ui(!self.busy(), |ui| {
                    for (p, label) in [
                        (Page::Library, "资料库"),
                        (Page::Profile, "个人信息"),
                        (Page::Resumes, "简历"),
                        (Page::Presets, "预设"),
                        (Page::Settings, "备份与设置"),
                    ] {
                        let selected = self.page == p
                            || (p == Page::Library && self.page == Page::Item)
                            || (p == Page::Resumes
                                && matches!(
                                    self.page,
                                    Page::Resume | Page::Picker | Page::History
                                ));
                        if ui
                            .add_sized([138., 36.], egui::Button::selectable(selected, label))
                            .clicked()
                        {
                            self.ui_navigation = Some(p)
                        }
                    }
                });
                ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
                    ui.small("资料保存在此电脑");
                });
            });
        egui::Panel::top("actions").show(root, |ui| {
            ui.add_space(6.);
            ui.horizontal_wrapped(|ui| {
                ui.heading(match self.page {
                    Page::Library => "资料库",
                    Page::Profile => "个人信息",
                    Page::Item => "编辑素材",
                    Page::Resumes => "简历",
                    Page::Resume => "编辑简历",
                    Page::Picker => {
                        if self.picker.append_to.is_some() {
                            "追加素材"
                        } else {
                            "新建简历"
                        }
                    }
                    Page::Presets => "预设",
                    Page::History => "版本历史",
                    Page::Settings => "备份与设置",
                });
                if self.dirty() {
                    ui.weak("未保存");
                }
                if self.busy() {
                    ui.spinner();
                    ui.label("正在处理…");
                }
                ui.add_enabled_ui(!self.busy(), |ui| {
                    if self.editor.is_some() || self.page == Page::Settings {
                        if ui.button("保存").clicked() {
                            self.ui_save = Some(false)
                        }
                        if ui.button("保存并返回").clicked() {
                            self.ui_save = Some(true)
                        }
                        if self.editor.is_some()
                            && ui
                                .button("重新加载")
                                .on_hover_text("从资料库读取已保存内容，放弃当前输入")
                                .clicked()
                        {
                            self.confirm = Some(Confirmation::ReloadEditor)
                        }
                    }
                    if matches!(
                        self.page,
                        Page::Item | Page::Profile | Page::Resume | Page::Picker | Page::History
                    ) && ui.button("返回").clicked()
                    {
                        let p = if matches!(self.page, Page::Item | Page::Profile) {
                            Page::Library
                        } else {
                            Page::Resumes
                        };
                        self.ui_navigation = Some(p)
                    }
                });
            });
            ui.add_space(6.);
            if !self.error.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(164, 42, 42), &self.error);
            }
        });
        egui::CentralPanel::default().show(root, |ui| {
            ui.add_enabled_ui(!self.busy(), |ui| {
                egui::ScrollArea::vertical()
                    .id_salt(format!("{:?}", self.page))
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.add_space(10.);
                        match self.page {
                            Page::Library => self.library_ui(ui),
                            Page::Resumes => self.resumes_ui(ui),
                            Page::Profile | Page::Item | Page::Resume => self.editor_ui(ui),
                            Page::Picker => self.picker_ui(ui),
                            Page::Presets => self.presets_ui(ui),
                            Page::History => self.history_ui(ui),
                            Page::Settings => self.settings_ui(ui),
                        }
                    });
            });
        });
        self.confirm_ui(&ctx);
        // Text/IME events must be applied by the editor before its save or navigation
        // command runs; both may be delivered during the same native frame.
        if let Some(back) = self.ui_save.take() {
            self.save(back)
        }
        if let Some(page) = self.ui_navigation.take() {
            self.navigate(page)
        }
        if ctx.input(|i| i.viewport().close_requested())
            && !self.allow_close
            && (self.busy() || self.dirty())
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if !self.busy() {
                self.confirm = Some(Confirmation::Close)
            }
        }
        if let Some((message, at)) = &self.toast {
            if at.elapsed() < Duration::from_secs(2) {
                // Measure each message afresh: reusing the short "saved" area's
                // width otherwise squeezes longer export paths into a tall column.
                let wrap_width = (ctx.content_rect().width() - 72.).clamp(120., 520.);
                let font_id = egui::TextStyle::Body.resolve(root.style());
                let text = ctx.fonts_mut(|fonts| {
                    fonts.layout(message.clone(), font_id, egui::Color32::WHITE, wrap_width)
                });
                egui::Area::new(egui::Id::new("saved-toast"))
                    .anchor(egui::Align2::CENTER_BOTTOM, [0., -24.])
                    .order(egui::Order::Foreground)
                    .fade_in(false)
                    .show(&ctx, |ui| {
                        ui.set_width(text.size().x + 24.);
                        egui::Frame::new()
                            .fill(egui::Color32::from_gray(28))
                            .corner_radius(6.)
                            .inner_margin(12.)
                            .show(ui, |ui| {
                                ui.add(egui::Label::new(text));
                            });
                    });
                ctx.request_repaint_after(Duration::from_millis(80));
            } else {
                self.toast = None
            }
        }
        if self.busy() {
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }
    fn library_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            field(ui, "搜索", &mut self.query, 260.);
        });
        state_picker(ui, &mut self.state_filter);
        ui.add_space(8.);
        ui.horizontal_wrapped(|ui| {
            ui.label("新建自定义类别");
            ui.add(
                egui::TextEdit::singleline(&mut self.category_name)
                    .desired_width(160.)
                    .hint_text("类别名称，如：爱好"),
            );
            if ui
                .add_enabled(
                    !self.category_name.trim().is_empty(),
                    egui::Button::new("创建类别"),
                )
                .clicked()
            {
                let name = self.category_name.clone();
                self.submit(true, move |s| {
                    s.create_category(&name, "custom")?;
                    Ok((None, None, "类别已创建".into()))
                });
            }
        });
        ui.small("自定义类别与教育、项目、技能并列，类别名称直接用作简历大标题。");
        egui::CollapsingHeader::new("管理类别")
            .id_salt("category-manager")
            .show(ui, |ui| {
                ui.small("可改名、排序；在类别旁添加素材或删除自建类别。");
                let categories = self.cache.categories.clone();
                let mut movement = None;
                for (index, c) in categories.iter().enumerate() {
                    let row = ui.horizontal_wrapped(|ui| {
                        if let Some(m) =
                            order_controls(ui, "library-categories", index, categories.len())
                        {
                            movement = Some(m)
                        }
                        ui.label(&c.name);
                        if ui.small_button("改名").clicked() {
                            self.renaming = Some((c.id.clone(), c.revision, c.name.clone()))
                        }
                    });
                    if let Some(m) = row_drop(&row.response, "library-categories", index) {
                        movement = Some(m)
                    }
                }
                if let Some((a, b)) = movement {
                    self.reorder_categories(a, b)
                }
                if let Some((id, rev, mut name)) = self.renaming.clone() {
                    ui.horizontal(|ui| {
                        field(ui, "新名称", &mut name, 200.);
                        if ui.button("确认改名").clicked() {
                            self.submit(true, move |s| {
                                s.rename_category(&id, rev, &name)?;
                                Ok((None, None, "类别已改名".into()))
                            });
                            self.renaming = None;
                        } else {
                            self.renaming = Some((id, rev, name));
                        }
                        if ui.button("取消").clicked() {
                            self.renaming = None;
                        }
                    });
                }
            });
        ui.add_space(8.);
        ui.horizontal_wrapped(|ui| {
            if ui
                .selectable_label(self.category_filter.is_none(), "全部类别")
                .clicked()
            {
                self.category_filter = None
            }
            for c in &self.cache.categories {
                if ui
                    .selectable_label(self.category_filter.as_deref() == Some(&c.id), &c.name)
                    .clicked()
                {
                    self.category_filter = Some(c.id.clone())
                }
            }
        });
        let categories = self.cache.categories.clone();
        let query = self.query.to_lowercase();
        for c in categories {
            if self.category_filter.as_ref().is_some_and(|id| id != &c.id) {
                continue;
            }
            let items: Vec<_> = self
                .cache
                .items
                .iter()
                .filter(|i| {
                    i.category_id == c.id && i.state == self.state_filter && search_item(i, &query)
                })
                .cloned()
                .collect();
            ui.add_space(12.);
            ui.horizontal_wrapped(|ui| {
                ui.strong(&c.name);
                ui.weak(format!("{} 项", items.len()));
                if self.state_filter == RecordState::Active && ui.button("添加素材").clicked() {
                    self.set_editor(
                        Editor::Item(ItemEditor::new(
                            c.id.clone(),
                            empty_item(&c.kind, &c.name),
                            None,
                        )),
                        Page::Item,
                    )
                }
                if !c.builtin && ui.small_button("删除类别").clicked() {
                    self.confirm = Some(Confirmation::DeleteCategory(c.id.clone(), c.revision));
                }
            });
            if items.is_empty() {
                ui.weak("暂无素材");
            }
            for item in items {
                ui.push_id(&item.id, |ui| {
                    egui::Frame::group(ui.style())
                        .inner_margin(10.)
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(item.content.title());
                                if item.state != RecordState::Trashed && ui.button("编辑").clicked()
                                {
                                    self.open_item(item.clone())
                                }
                                if ui.small_button("复制").clicked() {
                                    let id = item.id.clone();
                                    let rev = item.revision;
                                    self.submit(true, move |s| {
                                        s.copy_item(&id, rev)?;
                                        Ok((None, None, "素材已复制".into()))
                                    })
                                }
                                if item.state != RecordState::Active
                                    && ui.small_button("恢复").clicked()
                                {
                                    let id = item.id.clone();
                                    let rev = item.revision;
                                    self.submit(true, move |s| {
                                        s.set_item_state(&id, rev, RecordState::Active)?;
                                        Ok((None, None, "素材已恢复".into()))
                                    })
                                }
                                if item.state == RecordState::Active
                                    && ui.small_button("归档").clicked()
                                {
                                    let id = item.id.clone();
                                    let rev = item.revision;
                                    self.submit(true, move |s| {
                                        s.set_item_state(&id, rev, RecordState::Archived)?;
                                        Ok((None, None, "已归档".into()))
                                    })
                                }
                                if item.state != RecordState::Trashed
                                    && ui.small_button("移入回收站").clicked()
                                {
                                    self.confirm = Some(Confirmation::TrashItem(
                                        item.id.clone(),
                                        item.revision,
                                    ))
                                }
                            });
                            if !item.tags.is_empty() {
                                ui.weak(item.tags.join(" · "));
                            }
                            ui.small(format!("{} 条成果", item.achievements.len()));
                        });
                });
            }
        }
    }
    fn editor_ui(&mut self, ui: &mut egui::Ui) {
        let Some(mut editor) = self.editor.take() else {
            return;
        };
        match &mut editor {
            Editor::Profile { draft, .. } => {
                profile(ui, draft);
                ui.add_space(12.);
                ui.strong("照片");
                ui.horizontal_wrapped(|ui| {
                    ui.label(if draft.photo_asset_id.is_some() {
                        "已保存照片"
                    } else {
                        "未添加照片"
                    });
                    if ui
                        .add_enabled(
                            editor_key_profile(draft, &self.cache.profile.content),
                            egui::Button::new("选择照片"),
                        )
                        .on_hover_text("请先保存其他修改，再导入照片")
                        .clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("照片", &["png", "jpg", "jpeg"])
                            .pick_file()
                    {
                        let rev = self.cache.profile.revision;
                        self.submit(true, move |s| {
                            let bytes = fs::read(path)?;
                            let p = s.import_profile_photo(rev, &bytes)?;
                            Ok((
                                Some(Editor::Profile {
                                    revision: p.revision,
                                    draft: profile_draft(&p.content),
                                }),
                                None,
                                "照片已保存".into(),
                            ))
                        })
                    }
                    if draft.photo_asset_id.is_some() && ui.button("移除照片").clicked() {
                        draft.photo_asset_id = None;
                    }
                });
            }
            Editor::Item(e) => {
                ui.horizontal_wrapped(|ui| {
                    ui.label("所属类别");
                    egui::ComboBox::from_id_salt("item-category")
                        .selected_text(
                            self.cache
                                .categories
                                .iter()
                                .find(|c| c.id == e.category_id)
                                .map(|c| c.name.as_str())
                                .unwrap_or("类别不存在"),
                        )
                        .show_ui(ui, |ui| {
                            for c in self
                                .cache
                                .categories
                                .iter()
                                .filter(|c| c.kind == e.draft.content.kind())
                            {
                                ui.selectable_value(&mut e.category_id, c.id.clone(), &c.name);
                            }
                        });
                });
                ui.add_space(8.);
                content(ui, &mut e.draft.content, e.date.as_mut());
                if e.draft.content.kind() != "skill" {
                    ui.add_space(10.);
                    ui.strong("成果 / 正文");
                    let mut remove = None;
                    let mut movement = None;
                    let count = e.draft.achievements.len();
                    for (i, a) in e.draft.achievements.iter_mut().enumerate() {
                        ui.push_id(("achievement", i), |ui| {
                            let row = ui.horizontal(|ui| {
                                if let Some(m) = order_controls(ui, "item-achievements", i, count) {
                                    movement = Some(m)
                                }
                                ui.add(
                                    egui::TextEdit::multiline(&mut a.text)
                                        .desired_rows(2)
                                        .desired_width((ui.available_width() - 65.).max(120.)),
                                );
                                if ui.button("移除").clicked() {
                                    remove = Some(i)
                                }
                            });
                            if let Some(m) = row_drop(&row.response, "item-achievements", i) {
                                movement = Some(m)
                            }
                        });
                    }
                    if let Some(i) = remove {
                        e.draft.achievements.remove(i);
                    }
                    if let Some((a, b)) = movement {
                        move_index(&mut e.draft.achievements, a, b)
                    }
                    if ui.button("添加成果").clicked() {
                        e.draft.achievements.push(AchievementDraft {
                            id: None,
                            text: String::new(),
                        })
                    }
                }
                ui.add_space(12.);
                ui.horizontal_wrapped(|ui| {
                    field(ui, "标签", &mut e.tags, 350.);
                });
                ui.small("多个标签用逗号分隔。");
                multiline(ui, "内部备注（不进入简历）", &mut e.draft.notes, 3);
            }
            Editor::Resume(e) => self.resume_editor_ui(ui, e),
        }
        self.editor = Some(editor);
    }
    fn resumes_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("新建简历").clicked() {
                self.go(Page::Picker)
            }
            field(ui, "搜索", &mut self.query, 250.);
        });
        state_picker(ui, &mut self.state_filter);
        let query = self.query.to_lowercase();
        let resumes = self
            .cache
            .resumes
            .iter()
            .filter(|r| {
                r.state == self.state_filter
                    && format!("{} {} {}", r.name, r.company, r.role)
                        .to_lowercase()
                        .contains(&query)
            })
            .cloned()
            .collect::<Vec<_>>();
        if resumes.is_empty() {
            ui.add_space(15.);
            ui.label("暂无简历");
        }
        for r in resumes {
            ui.push_id(&r.id, |ui| {
                egui::Frame::group(ui.style())
                    .inner_margin(12.)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(&r.name);
                            ui.weak(format!("{} · {}", r.company, r.role));
                            if r.state != RecordState::Trashed && ui.button("编辑").clicked() {
                                self.open_resume(r.clone())
                            }
                            if ui.button("版本历史").clicked() {
                                self.open_history(r.id.clone())
                            }
                            if r.state != RecordState::Trashed && ui.button("复制").clicked() {
                                let id = r.id.clone();
                                let rev = r.revision;
                                let name = format!("{} 副本", r.name);
                                self.submit(true, move |s| {
                                    let r = s.copy_resume_checked(&id, Some(rev), &name)?;
                                    Ok((
                                        Some(Editor::Resume(ResumeEditor::new(r).into())),
                                        None,
                                        "已复制简历".into(),
                                    ))
                                })
                            }
                            if r.state != RecordState::Active && ui.button("恢复").clicked() {
                                let id = r.id.clone();
                                let rev = r.revision;
                                self.submit(true, move |s| {
                                    s.set_resume_state(&id, rev, RecordState::Active)?;
                                    Ok((None, None, "简历已恢复".into()))
                                })
                            }
                            if r.state == RecordState::Active && ui.button("归档").clicked() {
                                let id = r.id.clone();
                                let rev = r.revision;
                                self.submit(true, move |s| {
                                    s.set_resume_state(&id, rev, RecordState::Archived)?;
                                    Ok((None, None, "简历已归档".into()))
                                })
                            }
                            if r.state != RecordState::Trashed
                                && ui.small_button("移入回收站").clicked()
                            {
                                self.confirm =
                                    Some(Confirmation::TrashResume(r.id.clone(), r.revision))
                            }
                        });
                    });
            });
        }
    }
    fn resume_editor_ui(&mut self, ui: &mut egui::Ui, e: &mut ResumeEditor) {
        let saved_before_frame = Editor::Resume(e.clone().into()).key() == self.baseline;
        ui.horizontal_wrapped(|ui| {
            field(ui, "名称", &mut e.draft.name, 210.);
            field(ui, "目标公司", &mut e.draft.company, 170.);
            field(ui, "岗位", &mut e.draft.role, 170.);
        });
        let grouped = e.draft.document.format_version == 2;
        if !grouped {
            ui.label(
                "这是旧版简历。转换后可按类别管理；转换前内容会自动保存为历史版本，已有 PDF 保留。",
            );
            if ui
                .add_enabled(
                    Editor::Resume(e.clone().into()).key() == self.baseline,
                    egui::Button::new("转换为分组简历"),
                )
                .on_hover_text("先保存修改再转换")
                .clicked()
            {
                self.confirm = Some(Confirmation::Convert(e.id.clone(), e.revision))
            }
            text_preview(ui, &e.draft.document);
            return;
        }
        ui.horizontal_wrapped(|ui| {
            let clean = Editor::Resume(e.clone().into()).key() == self.baseline;
            if ui
                .add_enabled(clean, egui::Button::new("PDF 预览"))
                .on_hover_text("先保存修改，再更新 PDF 预览")
                .clicked()
            {
                self.render_resume(e, None)
            }
            if ui
                .add_enabled(clean, egui::Button::new("导出 PDF"))
                .on_hover_text("先保存修改，再导出 PDF")
                .clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("PDF", &["pdf"])
                    .set_file_name(format!("{}.pdf", safe_filename(&e.draft.name)))
                    .save_file()
            {
                self.render_resume(e, Some(path))
            }
            if ui
                .add_enabled(
                    Editor::Resume(e.clone().into()).key() == self.baseline,
                    egui::Button::new("追加资料库素材"),
                )
                .on_hover_text("先保存简历修改，再追加素材")
                .clicked()
            {
                self.picker = Picker {
                    append_to: Some((e.id.clone(), e.revision)),
                    ..Default::default()
                };
                self.picker.selection.profile_fields.clear();
                self.picker_baseline = serde_json::to_string(&self.picker).unwrap();
                self.page = Page::Picker;
            }
            if ui.selectable_label(self.preview, "内容预览").clicked() {
                self.preview = !self.preview
            }
            if ui
                .add_enabled(
                    Editor::Resume(e.clone().into()).key() == self.baseline,
                    egui::Button::new("版本历史"),
                )
                .clicked()
            {
                self.open_history(e.id.clone())
            }
        });
        ui.small("这里的改写只影响当前简历，资料库和其他简历保持独立。");
        if self.preview {
            if self
                .rendered
                .as_ref()
                .is_some_and(|r| r.resume_id == e.id && r.revision == e.revision)
            {
                self.pdf_preview_ui(ui);
            } else {
                text_preview(ui, &e.draft.document);
            }
            return;
        }
        egui::CollapsingHeader::new("个人信息")
            .id_salt("resume-profile")
            .show(ui, |ui| {
                let mut p = profile_draft(&e.draft.document.profile);
                profile(ui, &mut p);
                let old_summary = e.draft.document.profile.summary.clone();
                e.draft.document.profile = ProfileDraft {
                    name: p.name,
                    title: p.title,
                    phone: p.phone,
                    email: p.email,
                    location: p.location,
                    links: p.links,
                    photo_asset_id: p.photo_asset_id,
                    custom_fields: p.custom_fields,
                    summary: old_summary,
                };
            });
        egui::CollapsingHeader::new("版式").show(ui, |ui| {
            style(ui, &mut e.draft.document.style);
            ui.horizontal_wrapped(|ui| {
                field(ui, "预设名称", &mut self.picker.preset_name, 180.);
                if ui.button("保存版式预设").clicked() {
                    let name = self.picker.preset_name.clone();
                    let style = e.draft.document.style.clone();
                    self.submit(true, move |s| {
                        s.save_preset_kind(
                            None,
                            None,
                            PresetKind::Layout,
                            PresetDraft {
                                name,
                                style,
                                selection: Selection::default(),
                            },
                        )?;
                        Ok((None, None, "版式预设已保存".into()))
                    })
                }
                egui::ComboBox::from_id_salt("resume-layout-preset")
                    .selected_text("应用版式预设")
                    .show_ui(ui, |ui| {
                        for p in self
                            .cache
                            .presets
                            .iter()
                            .filter(|p| p.kind == PresetKind::Layout)
                        {
                            if ui.selectable_label(false, &p.content.name).clicked() {
                                e.draft.document.style = p.content.style.clone()
                            }
                        }
                    });
            });
        });
        let sections = e.draft.document.sections.clone();
        let mut section_move = None;
        let mut block_move = None;
        let mut remove = None;
        for (index, section) in sections.iter().enumerate() {
            ui.add_space(10.);
            let row = ui.horizontal_wrapped(|ui| {
                if let Some(m) = order_controls(ui, "resume-sections", index, sections.len()) {
                    section_move = Some(m)
                }
                field(
                    ui,
                    "类别标题",
                    &mut e.draft.document.sections[index].title,
                    220.,
                );
                if let Some(category) = self
                    .cache
                    .categories
                    .iter()
                    .find(|c| c.id == section.category_id)
                    && category.name != e.draft.document.sections[index].title
                    && ui
                        .small_button("使用资料库名称")
                        .on_hover_text("将本组标题更新为资料库中的类别名；保存后生效")
                        .clicked()
                {
                    e.draft.document.sections[index].title = category.name.clone();
                }
                if ui.button("添加临时素材").clicked() {
                    let id = resume_core::store::id();
                    let mut content = empty_item(&section.kind, &section.title).content;
                    match &mut content {
                        ItemContent::Education { school, .. } => *school = "新教育经历".into(),
                        ItemContent::Work { company, .. } => *company = "新工作经历".into(),
                        ItemContent::Project { name, .. } => *name = "新项目".into(),
                        ItemContent::Skill { name, .. } => *name = "新技能".into(),
                        ItemContent::Custom { title, .. } => *title = "新内容".into(),
                    }
                    if let Some(date) = DateInput::new(&content) {
                        e.dates.insert(id.clone(), date);
                    }
                    e.draft.document.sections[index].block_ids.push(id.clone());
                    let position = section
                        .block_ids
                        .last()
                        .and_then(|id| e.draft.document.blocks.iter().position(|b| &b.id == id))
                        .map(|p| p + 1)
                        .unwrap_or(e.draft.document.blocks.len());
                    e.draft.document.blocks.insert(
                        position,
                        ResumeBlock {
                            id,
                            source: Source {
                                item_id: String::new(),
                                revision: 0,
                            },
                            content,
                            achievements: vec![],
                            visible: true,
                            page_break_before: false,
                        },
                    );
                }
            });
            if let Some(m) = row_drop(&row.response, "resume-sections", index) {
                section_move = Some(m)
            }
            for (bi, block_id) in section.block_ids.iter().enumerate() {
                let Some(block) = e
                    .draft
                    .document
                    .blocks
                    .iter_mut()
                    .find(|b| &b.id == block_id)
                else {
                    continue;
                };
                let scope = format!("resume-blocks-{}", section.id);
                let header = ui.horizontal_wrapped(|ui| {
                    if let Some(m) = order_controls(ui, &scope, bi, section.block_ids.len()) {
                        block_move = Some((section.id.clone(), m))
                    }
                    ui.checkbox(&mut block.visible, block.content.title());
                    ui.checkbox(&mut block.page_break_before, "前置分页");
                    if ui.small_button("移除素材").clicked() {
                        remove = Some(block.id.clone())
                    }
                });
                if let Some(m) = row_drop(&header.response, &scope, bi) {
                    block_move = Some((section.id.clone(), m))
                }
                ui.push_id(&block.id, |ui| {
                    egui::CollapsingHeader::new("改写内容").show(ui, |ui| {
                        content(ui, &mut block.content, e.dates.get_mut(&block.id));
                        let mut category = self
                            .save_back_categories
                            .entry(block.id.clone())
                            .or_insert_with(|| section.category_id.clone())
                            .clone();
                        ui.horizontal_wrapped(|ui| {
                            egui::ComboBox::from_id_salt("save-back-category")
                                .selected_text(
                                    self.cache
                                        .categories
                                        .iter()
                                        .find(|c| c.id == category)
                                        .map(|c| c.name.as_str())
                                        .unwrap_or("选择保存类别"),
                                )
                                .show_ui(ui, |ui| {
                                    for c in self
                                        .cache
                                        .categories
                                        .iter()
                                        .filter(|c| c.kind == block.content.kind())
                                    {
                                        ui.selectable_value(&mut category, c.id.clone(), &c.name);
                                    }
                                });
                            if ui
                                .add_enabled(
                                    saved_before_frame,
                                    egui::Button::new("另存为资料库素材"),
                                )
                                .on_hover_text("先保存简历修改；会创建新素材")
                                .clicked()
                            {
                                let target = category.clone();
                                let id = e.id.clone();
                                let rev = e.revision;
                                let block_id = block.id.clone();
                                self.submit(true, move |s| {
                                    s.resume_block_as_item_in_category(
                                        &id, rev, &block_id, &target,
                                    )?;
                                    Ok((None, None, "已新增资料库素材".into()))
                                })
                            }
                        });
                        self.save_back_categories.insert(block.id.clone(), category);
                        let mut movement = None;
                        let mut deletion = None;
                        let count = block.achievements.len();
                        for (ai, a) in block.achievements.iter_mut().enumerate() {
                            ui.push_id(&a.id, |ui| {
                                ui.horizontal(|ui| {
                                    if let Some(m) = order_controls(
                                        ui,
                                        &format!("resume-achievements-{}", block.id),
                                        ai,
                                        count,
                                    ) {
                                        movement = Some(m)
                                    }
                                    ui.add(
                                        egui::TextEdit::multiline(&mut a.text)
                                            .desired_rows(2)
                                            .desired_width((ui.available_width() - 65.).max(150.)),
                                    );
                                    if ui.small_button("移除").clicked() {
                                        deletion = Some(ai)
                                    }
                                });
                            });
                        }
                        if let Some(ai) = deletion {
                            block.achievements.remove(ai);
                        }
                        if let Some((a, b)) = movement {
                            move_index(&mut block.achievements, a, b)
                        }
                        if block.content.kind() != "skill" && ui.button("添加正文").clicked() {
                            block.achievements.push(ResumeAchievement {
                                id: resume_core::store::id(),
                                source_id: String::new(),
                                text: String::new(),
                            })
                        }
                    });
                });
            }
        }
        if let Some((a, b)) = section_move {
            let mut order = sections.iter().map(|s| s.id.clone()).collect::<Vec<_>>();
            move_index(&mut order, a, b);
            if let Err(error) =
                resume_core::grouping::reorder_sections(&mut e.draft.document, &order)
            {
                self.error = error.to_string()
            }
        }
        if let Some((section, (a, b))) = block_move {
            let mut order = sections
                .iter()
                .find(|s| s.id == section)
                .unwrap()
                .block_ids
                .clone();
            move_index(&mut order, a, b);
            if let Err(error) = resume_core::grouping::reorder_section_items(
                &mut e.draft.document,
                &section,
                &order,
            ) {
                self.error = error.to_string()
            }
        }
        if let Some(id) = remove {
            e.remove_block(&id)
        }
        ui.add_space(18.);
        ui.horizontal_wrapped(|ui| {
            field(ui, "版本名称", &mut self.snapshot_name, 180.);
            if ui
                .add_enabled(
                    Editor::Resume(e.clone().into()).key() == self.baseline,
                    egui::Button::new("保存历史版本"),
                )
                .on_hover_text("先保存简历修改")
                .clicked()
            {
                let id = e.id.clone();
                let rev = e.revision;
                let name = self.snapshot_name.clone();
                self.submit(true, move |s| {
                    s.create_snapshot(&id, rev, &name, None)?;
                    Ok((None, None, "历史版本已保存".into()))
                })
            }
        });
    }
    pub(crate) fn pdf_preview_ui(&mut self, ui: &mut egui::Ui) {
        let count = self.rendered.as_ref().map_or(0, |r| r.pages.len());
        ui.horizontal(|ui| {
            if ui
                .add_enabled(self.preview_page > 0, egui::Button::new("上一页"))
                .clicked()
            {
                self.preview_page -= 1;
            }
            ui.label(format!(
                "{} / {}",
                if count == 0 { 0 } else { self.preview_page + 1 },
                count
            ));
            if ui
                .add_enabled(self.preview_page + 1 < count, egui::Button::new("下一页"))
                .clicked()
            {
                self.preview_page += 1;
            }
        });
        if self.loaded_preview_page != Some(self.preview_page) {
            self.preview_pages.clear();
            self.loaded_preview_page = Some(self.preview_page);
            if let Some(path) = self
                .rendered
                .as_ref()
                .and_then(|r| r.pages.get(self.preview_page))
            {
                match image::open(path) {
                    Ok(img) => {
                        let rgba = img.into_rgba8();
                        self.preview_pages.push(ui.ctx().load_texture(
                            path.display().to_string(),
                            egui::ColorImage::from_rgba_unmultiplied(
                                [rgba.width() as usize, rgba.height() as usize],
                                rgba.as_raw(),
                            ),
                            Default::default(),
                        ))
                    }
                    Err(e) => self.error = format!("预览图读取失败：{e}"),
                }
            }
        }
        if let Some(texture) = self.preview_pages.first() {
            let width = ui.available_width().min(794.);
            let size = texture.size_vec2();
            ui.add(
                egui::Image::new(texture)
                    .fit_to_exact_size(egui::vec2(width, width * size.y / size.x)),
            );
        }
    }
}
fn safe_filename(s: &str) -> String {
    s.chars()
        .map(|c| {
            if "<>:\"/\\|?*".contains(c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .take(120)
        .collect()
}
fn search_item(i: &LibraryItem, q: &str) -> bool {
    format!(
        "{} {} {} {}",
        i.content.title(),
        serde_json::to_string(&i.content).unwrap(),
        i.tags.join(" "),
        i.achievements
            .iter()
            .map(|a| a.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    )
    .to_lowercase()
    .contains(q)
}
fn editor_key_profile(a: &resume_core::catalog::ProfileEditorDraft, b: &ProfileDraft) -> bool {
    serde_json::to_string(a).unwrap() == serde_json::to_string(&profile_draft(b)).unwrap()
}
