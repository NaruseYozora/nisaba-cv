use crate::{app::*, editors::*, widgets::*};
use eframe::egui;
use resume_core::model::*;
use std::time::Instant;

impl App {
    pub fn picker_ui(&mut self, ui: &mut egui::Ui) {
        if self.picker.preset_id.is_some() {
            if self
                .picker
                .selection
                .profile_fields
                .contains(&ProfileField::Summary)
            {
                ui.label("旧预设包含个人简介，可用“修复选项”移除这一旧选项。");
            }
            if ui.button("修复选项：移除不可用选择").clicked() {
                self.repair_picker();
            }
        }
        if self.picker.append_to.is_none() {
            ui.horizontal_wrapped(|ui| {
                field(ui, "简历名称", &mut self.picker.name, 200.);
                field(ui, "公司", &mut self.picker.company, 150.);
                field(ui, "岗位", &mut self.picker.role, 150.);
            });
            egui::CollapsingHeader::new("个人信息选项")
                .default_open(true)
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (f, label) in [
                            (ProfileField::Name, "姓名"),
                            (ProfileField::Title, "求职方向"),
                            (ProfileField::Phone, "电话"),
                            (ProfileField::Email, "邮箱"),
                            (ProfileField::Location, "所在地"),
                            (ProfileField::Links, "链接"),
                            (ProfileField::Photo, "照片"),
                        ] {
                            let mut selected = self.picker.selection.profile_fields.contains(&f);
                            if ui.checkbox(&mut selected, label).changed() {
                                if selected {
                                    self.picker.selection.profile_fields.push(f)
                                } else {
                                    self.picker.selection.profile_fields.retain(|v| v != &f)
                                }
                            }
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        for f in &self.cache.profile.content.custom_fields {
                            let mut selected =
                                self.picker.selection.custom_field_ids.contains(&f.id);
                            if ui.checkbox(&mut selected, &f.label).changed() {
                                if selected {
                                    self.picker.selection.custom_field_ids.push(f.id.clone())
                                } else {
                                    self.picker
                                        .selection
                                        .custom_field_ids
                                        .retain(|v| v != &f.id)
                                }
                            }
                        }
                    });
                });
        }
        ui.horizontal_wrapped(|ui| {
            field(ui, "预设名称", &mut self.picker.preset_name, 170.);
            if ui.button("保存选材预设").clicked() {
                self.save_selection_preset()
            }
            egui::ComboBox::from_id_salt("use-preset")
                .selected_text("应用预设")
                .show_ui(ui, |ui| {
                    let presets = self.cache.presets.clone();
                    for p in presets {
                        if ui.selectable_label(false, &p.content.name).clicked() {
                            self.apply_preset(p)
                        }
                    }
                });
        });
        if self.picker.append_to.is_none() {
            egui::CollapsingHeader::new("版式").show(ui, |ui| style(ui, &mut self.picker.style));
        }
        ui.add_space(10.);
        ui.strong("已选类别与顺序");
        ui.small("拖动 ≡ 调整大类顺序；每个大类中的素材单独排序。");
        let groups = self.picker.selection.sections.clone().unwrap_or_default();
        let mut group_move = None;
        let mut item_move = None;
        if groups.is_empty() {
            ui.weak("请在下方选择素材。");
        }
        for (index, group) in groups.iter().enumerate() {
            let name = self
                .cache
                .categories
                .iter()
                .find(|c| c.id == group.category_id)
                .map(|c| c.name.as_str())
                .unwrap_or("类别不可用");
            let row = ui.horizontal_wrapped(|ui| {
                if let Some(m) = order_controls(ui, "picker-sections", index, groups.len()) {
                    group_move = Some(m)
                }
                ui.strong(name);
            });
            if let Some(m) = row_drop(&row.response, "picker-sections", index) {
                group_move = Some(m)
            }
            let scope = format!("picker-items-{}", group.category_id);
            for (i, item) in group.items.iter().enumerate() {
                let name = self
                    .cache
                    .items
                    .iter()
                    .find(|v| v.id == item.item_id)
                    .map(|v| v.content.title())
                    .unwrap_or("素材不可用");
                let row = ui.horizontal_wrapped(|ui| {
                    ui.add_space(16.);
                    if let Some(m) = order_controls(ui, &scope, i, group.items.len()) {
                        item_move = Some((index, m))
                    }
                    ui.label(name);
                });
                if let Some(m) = row_drop(&row.response, &scope, i) {
                    item_move = Some((index, m))
                }
            }
        }
        if let Some((a, b)) = group_move {
            move_index(self.picker.selection.sections.as_mut().unwrap(), a, b)
        }
        if let Some((index, (a, b))) = item_move {
            move_index(
                &mut self.picker.selection.sections.as_mut().unwrap()[index].items,
                a,
                b,
            )
        }
        ui.add_space(12.);
        ui.separator();
        ui.horizontal(|ui| {
            field(ui, "搜索素材", &mut self.query, 260.);
        });
        let query = self.query.to_lowercase();
        let categories = self.cache.categories.clone();
        for c in categories {
            let items = self
                .cache
                .items
                .iter()
                .filter(|i| {
                    i.category_id == c.id
                        && i.state == RecordState::Active
                        && format!("{} {}", i.content.title(), i.tags.join(" "))
                            .to_lowercase()
                            .contains(&query)
                })
                .cloned()
                .collect::<Vec<_>>();
            if items.is_empty() {
                continue;
            }
            ui.add_space(8.);
            ui.strong(&c.name);
            for item in items {
                ui.push_id(&item.id, |ui| {
                    let mut selected = self.picker.selected(&item.id);
                    if ui.checkbox(&mut selected, item.content.title()).changed() {
                        self.picker.toggle_item(&item, selected)
                    }
                    if selected {
                        ui.indent("selected-details", |ui| {
                            if let Some(s) = self.picker.selected_mut(&item.id) {
                                if let ItemContent::Project { background, .. } = &item.content
                                    && !background.is_empty()
                                {
                                    ui.checkbox(&mut s.include_background, "包括项目背景");
                                }
                                for a in &item.achievements {
                                    let mut checked = s.achievement_ids.contains(&a.id);
                                    if ui.checkbox(&mut checked, &a.text).changed() {
                                        if checked {
                                            s.achievement_ids.push(a.id.clone())
                                        } else {
                                            s.achievement_ids.retain(|id| id != &a.id)
                                        }
                                    }
                                }
                                let mut movement = None;
                                let count = s.achievement_ids.len();
                                for (i, id) in s.achievement_ids.iter().enumerate() {
                                    let title = item
                                        .achievements
                                        .iter()
                                        .find(|a| &a.id == id)
                                        .map(|a| a.text.as_str())
                                        .unwrap_or("成果不可用");
                                    ui.horizontal(|ui| {
                                        if let Some(m) = order_controls(
                                            ui,
                                            &format!("picker-achievements-{}", item.id),
                                            i,
                                            count,
                                        ) {
                                            movement = Some(m)
                                        }
                                        ui.label(title.chars().take(45).collect::<String>());
                                    });
                                }
                                if let Some((a, b)) = movement {
                                    move_index(&mut s.achievement_ids, a, b)
                                }
                            }
                        });
                    }
                });
            }
        }
        ui.add_space(16.);
        if ui
            .button(if self.picker.append_to.is_some() {
                "追加到简历"
            } else {
                "生成简历"
            })
            .clicked()
        {
            self.create_resume()
        }
    }
    pub fn apply_preset(&mut self, p: Preset) {
        if p.kind == PresetKind::Layout {
            self.picker.style = p.content.style;
            self.toast = Some(("版式已应用".into(), Instant::now()));
            return;
        }
        let selection = p.content.selection.clone();
        match self.actor.call(false, move |s| {
            let selection = s.grouped_selection(&selection)?;
            let missing = s.inspect_selection(&selection)?;
            if !missing.is_empty() {
                return Err(resume_core::Error::Invalid(format!(
                    "预设含不可用选项：{}。请先修复资料或编辑预设。",
                    missing.join("、")
                )));
            }
            Ok(selection)
        }) {
            Err(e) => self.error = e.to_string(),
            Ok(mut selection) => {
                if self.picker.append_to.is_some() {
                    selection.profile_fields.clear();
                    selection.custom_field_ids.clear()
                }
                self.picker.selection = selection;
                self.picker.style = p.content.style;
                self.picker.preset_name = p.content.name;
                self.picker.preset_id = Some((p.id, p.revision));
                self.toast = Some(("选材预设已应用".into(), Instant::now()))
            }
        }
    }
    pub fn presets_ui(&mut self, ui: &mut egui::Ui) {
        let presets = self.cache.presets.clone();
        if presets.is_empty() {
            ui.label("在新建简历中保存选材预设，在编辑简历中保存版式预设。");
        }
        for p in presets {
            ui.push_id(&p.id, |ui| {
                egui::Frame::group(ui.style())
                    .inner_margin(10.)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(&p.content.name);
                            ui.weak(if p.kind == PresetKind::Layout {
                                "版式"
                            } else {
                                "选材"
                            });
                            if ui.button("使用").clicked() {
                                self.go(Page::Picker);
                                self.apply_preset(p.clone())
                            }
                            if p.kind == PresetKind::Selection && ui.button("编辑").clicked() {
                                self.edit_preset(p.clone());
                            }
                            if ui.small_button("删除").clicked() {
                                self.confirm =
                                    Some(Confirmation::DeletePreset(p.id.clone(), p.revision))
                            }
                        });
                    });
            });
        }
    }
    pub fn edit_preset(&mut self, p: Preset) {
        self.go(Page::Picker);
        let mut selection = p.content.selection;
        if selection.sections.is_none() {
            let mut sections: Vec<SelectionSection> = vec![];
            for i in selection.items.drain(..) {
                let category = self
                    .cache
                    .items
                    .iter()
                    .find(|v| v.id == i.item_id)
                    .map(|v| v.category_id.clone())
                    .unwrap_or_else(|| format!("missing:{}", i.item_id));
                if !sections.iter().any(|s| s.category_id == category) {
                    sections.push(SelectionSection {
                        category_id: category.clone(),
                        items: vec![],
                    })
                }
                sections
                    .iter_mut()
                    .find(|s| s.category_id == category)
                    .unwrap()
                    .items
                    .push(i);
            }
            selection.sections = Some(sections);
        }
        self.picker.selection = selection;
        self.picker.style = p.content.style;
        self.picker.preset_name = p.content.name;
        self.picker.preset_id = Some((p.id, p.revision));
        self.picker_baseline = serde_json::to_string(&self.picker).unwrap();
    }
    pub fn repair_picker(&mut self) {
        self.picker
            .selection
            .profile_fields
            .retain(|f| f != &ProfileField::Summary);
        self.picker.selection.custom_field_ids.retain(|id| {
            self.cache
                .profile
                .content
                .custom_fields
                .iter()
                .any(|f| &f.id == id)
        });
        for group in self.picker.selection.sections.as_mut().unwrap() {
            group.items.retain(|i| {
                self.cache.items.iter().any(|v| {
                    v.id == i.item_id
                        && v.category_id == group.category_id
                        && v.state == RecordState::Active
                })
            });
            for i in &mut group.items {
                if let Some(v) = self.cache.items.iter().find(|v| v.id == i.item_id) {
                    i.achievement_ids
                        .retain(|id| v.achievements.iter().any(|a| &a.id == id));
                }
            }
        }
        self.picker
            .selection
            .sections
            .as_mut()
            .unwrap()
            .retain(|s| !s.items.is_empty());
        self.error.clear();
        self.toast = Some(("不可用选项已移除，请确认后保存预设".into(), Instant::now()));
    }
    pub fn history_ui(&mut self, ui: &mut egui::Ui) {
        if self.preview
            && self
                .rendered
                .as_ref()
                .is_some_and(|r| r.resume_id.starts_with("snapshot:"))
        {
            if ui.button("返回历史列表").clicked() {
                self.preview = false;
                self.clear_preview();
                return;
            }
            ui.small("原始历史 PDF，内容与导出时一致。");
            self.pdf_preview_ui(ui);
            return;
        }
        let snapshots = self.cache.snapshots.clone();
        if snapshots.is_empty() {
            ui.label("暂无历史版本");
        }
        for snapshot in snapshots {
            ui.push_id(&snapshot.id, |ui| {
                egui::Frame::group(ui.style())
                    .inner_margin(10.)
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.strong(&snapshot.name);
                            ui.weak(format!("来源修订 {}", snapshot.source_revision));
                            if ui.button("恢复为新草稿").clicked() {
                                let id = snapshot.id.clone();
                                let name = format!("{} 恢复", snapshot.name);
                                self.submit(true, move |s| {
                                    let r = s.restore_snapshot_as_resume(&id, &name)?;
                                    Ok((
                                        Some(Editor::Resume(ResumeEditor::new(r).into())),
                                        None,
                                        "已恢复为新草稿".into(),
                                    ))
                                })
                            }
                            if snapshot.pdf_asset_id.is_some()
                                && ui.button("查看历史 PDF").clicked()
                            {
                                self.preview_history(snapshot.id.clone());
                            }
                            if snapshot.pdf_asset_id.is_some()
                                && ui.button("另存历史 PDF").clicked()
                                && let Some(path) = rfd::FileDialog::new()
                                    .add_filter("PDF", &["pdf"])
                                    .set_file_name("历史简历.pdf")
                                    .save_file()
                            {
                                let id = snapshot.id.clone();
                                self.submit(false, move |s| {
                                    let pdf = s.snapshot_pdf(&id)?;
                                    let hash = resume_core::files::pdf_target_hash(&path)?;
                                    resume_core::files::publish_pdf(&path, &pdf, hash.as_deref())?;
                                    Ok((None, None, "历史 PDF 已保存".into()))
                                })
                            }
                        });
                        egui::CollapsingHeader::new("查看内容")
                            .show(ui, |ui| text_preview(ui, &snapshot.document));
                    });
            });
        }
    }
    pub fn settings_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("资料保存在此电脑");
        ui.small(self.data.display().to_string());
        ui.add_space(12.);
        ui.horizontal_wrapped(|ui| {
            if ui.button("导出完整备份").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("完整备份", &["rslbackup"])
                    .set_file_name("Nisaba-CV.rslbackup")
                    .save_file()
            {
                self.submit(false, move |s| {
                    s.backup(&path)?;
                    Ok((None, None, "完整备份已保存".into()))
                })
            }
            if ui.button("恢复完整备份").clicked()
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("完整备份", &["rslbackup"])
                    .pick_file()
            {
                self.inspect_restore(path)
            }
        });
        ui.add_space(14.);
        ui.strong("自动备份");
        ui.checkbox(&mut self.settings.rolling_enabled, "启用滚动备份");
        ui.horizontal_wrapped(|ui| {
            ui.label("滚动保留");
            ui.add(egui::DragValue::new(&mut self.settings.rolling_keep).range(1..=50));
            ui.label("份；有变更日期保留");
            ui.add(egui::DragValue::new(&mut self.settings.daily_keep).range(1..=50));
            ui.label("天");
        });
        ui.label(
            self.settings
                .directory
                .as_deref()
                .unwrap_or("默认：资料目录下的 automatic-backups"),
        );
        ui.horizontal(|ui| {
            if ui.button("选择备份目录").clicked()
                && let Some(path) = rfd::FileDialog::new().pick_folder()
            {
                self.settings.directory = Some(path.to_string_lossy().into())
            }
            if ui.button("使用默认目录").clicked() {
                self.settings.directory = None
            }
            if ui.button("保存备份设置").clicked() {
                self.save_settings(false)
            }
        });
        if ui.button("检查自动备份状态").clicked() {
            match self.actor.call(false, |s| s.automatic_backup_status()) {
                Ok(status) => {
                    self.backup_status = format!(
                        "自动备份：{}；现有 {} 份。\n目录：{}{}",
                        if status.last_success_at.is_some() {
                            "已完成备份"
                        } else {
                            "等待首次备份"
                        },
                        status.managed_count,
                        status.directory,
                        status
                            .last_error
                            .map(|e| format!("\n备份失败：{e}"))
                            .unwrap_or_default()
                    )
                }
                Err(e) => self.error = e.to_string(),
            }
        }
        if !self.backup_status.is_empty() {
            ui.label(&self.backup_status);
        }
    }
    pub fn confirm_ui(&mut self, ctx: &egui::Context) {
        let Some(confirm) = self.confirm.clone() else {
            return;
        };
        egui::Modal::new(egui::Id::new("confirmation")).show(ctx, |ui| {
            let (title, body, action) = match &confirm {
                Confirmation::Navigate(_) | Confirmation::Close => {
                    ("有未保存的修改", "离开后会丢弃当前输入。", "丢弃修改")
                }
                Confirmation::TrashItem(_, _) | Confirmation::TrashResume(_, _) => (
                    "移入回收站",
                    "可以从回收站恢复；已生成的简历和历史内容保留。",
                    "移入回收站",
                ),
                Confirmation::DeleteCategory(_, _) => (
                    "删除类别",
                    "删除类别及其中所有素材（包括归档和回收站），无法撤销。已生成的简历保留。",
                    "删除",
                ),
                Confirmation::DeletePreset(_, _) => ("删除预设", "已生成的简历保留。", "删除"),
                Confirmation::Convert(_, _) => (
                    "转换为分组简历",
                    "同类素材集中成组。转换前内容保存为历史版本，已有 PDF 保留。",
                    "转换并保存版本",
                ),
                Confirmation::Restore(_, _) => (
                    "恢复完整备份",
                    "当前资料库会被备份中的资料替换。恢复前自动保留当前库的备份。",
                    "确认恢复",
                ),
                Confirmation::ReloadEditor => (
                    "重新加载",
                    "当前输入将被已保存内容替换。",
                    "放弃修改并重新加载",
                ),
            };
            ui.heading(title);
            if let Confirmation::DeleteCategory(id, _) = &confirm
                && let Some(category) = self.cache.categories.iter().find(|c| &c.id == id)
            {
                let count = self
                    .cache
                    .items
                    .iter()
                    .filter(|i| &i.category_id == id)
                    .count();
                ui.strong(format!("{} · {} 项素材", category.name, count));
            }
            ui.label(body);
            if matches!(confirm, Confirmation::Restore(_, _)) {
                ui.label(&self.backup_status);
            }
            ui.horizontal(|ui| {
                if matches!(confirm, Confirmation::Navigate(_) | Confirmation::Close)
                    && (self.editor.is_some() || self.page == Page::Settings)
                    && ui.button("返回并保存").clicked()
                {
                    self.confirm = None;
                    self.ui_save = Some(false)
                }
                if ui.button("取消").clicked() {
                    self.confirm = None
                }
                if ui.button(action).clicked() {
                    self.confirm = None;
                    self.confirm_action(confirm.clone())
                }
            });
        });
    }
}
