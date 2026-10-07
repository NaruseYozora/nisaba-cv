use crate::editors::{DateInput, move_index, new_custom_field};
use eframe::egui::{self, Ui};
use resume_core::{catalog::ProfileEditorDraft, model::*};

pub fn field(ui: &mut Ui, label: &str, value: &mut String, width: f32) -> egui::Response {
    let label = ui.label(label);
    ui.add(egui::TextEdit::singleline(value).desired_width(width))
        .labelled_by(label.id)
}
pub fn multiline(ui: &mut Ui, label: &str, value: &mut String, rows: usize) {
    let l = ui.label(label);
    ui.add(
        egui::TextEdit::multiline(value)
            .desired_width(f32::INFINITY)
            .desired_rows(rows),
    )
    .labelled_by(l.id);
}
pub fn dates(ui: &mut Ui, d: &mut DateInput) {
    ui.horizontal_wrapped(|ui| {
        field(ui, "时间", &mut d.start, 88.);
        ui.label("—");
        ui.add_enabled_ui(!d.ongoing, |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut d.end)
                    .desired_width(88.)
                    .hint_text("YYYY-MM"),
            );
        });
        ui.checkbox(&mut d.ongoing, "至今");
    });
}
pub fn profile(ui: &mut Ui, d: &mut ProfileEditorDraft) {
    let width = ((ui.available_width() - 150.) / 2.).clamp(100., 240.);
    ui.horizontal_wrapped(|ui| {
        field(ui, "姓名", &mut d.name, width);
        field(ui, "求职方向", &mut d.title, width);
    });
    ui.horizontal_wrapped(|ui| {
        field(ui, "电话", &mut d.phone, width);
        field(ui, "邮箱", &mut d.email, width);
    });
    ui.horizontal_wrapped(|ui| {
        field(ui, "所在地", &mut d.location, width);
    });
    ui.add_space(12.);
    ui.strong("自定义信息");
    let mut remove = None;
    let mut movement = None;
    let count = d.custom_fields.len();
    for (i, f) in d.custom_fields.iter_mut().enumerate() {
        ui.push_id(&f.id, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut f.label)
                        .desired_width(110.)
                        .hint_text("名称"),
                );
                ui.add(
                    egui::TextEdit::singleline(&mut f.value)
                        .desired_width(width * 1.5)
                        .hint_text("内容"),
                );
                if ui
                    .add_enabled(i > 0, egui::Button::new("↑"))
                    .on_hover_text("上移")
                    .clicked()
                {
                    movement = Some((i, i - 1))
                }
                if ui
                    .add_enabled(i + 1 < count, egui::Button::new("↓"))
                    .on_hover_text("下移")
                    .clicked()
                {
                    movement = Some((i, i + 1))
                }
                if ui.small_button("移除").clicked() {
                    remove = Some(i)
                }
            });
        });
    }
    if let Some(i) = remove {
        d.custom_fields.remove(i);
    }
    if let Some((a, b)) = movement {
        move_index(&mut d.custom_fields, a, b)
    }
    if ui
        .add_enabled(count < 50, egui::Button::new("添加自定义信息"))
        .clicked()
    {
        d.custom_fields.push(new_custom_field())
    }
    ui.add_space(12.);
    ui.strong("链接");
    let mut remove = None;
    for (i, link) in d.links.iter_mut().enumerate() {
        ui.push_id(("link", i), |ui| {
            ui.horizontal_wrapped(|ui| {
                field(ui, "名称", &mut link.label, 90.);
                field(ui, "网址", &mut link.url, width * 1.4);
                if ui.small_button("移除").clicked() {
                    remove = Some(i)
                }
            });
        });
    }
    if let Some(i) = remove {
        d.links.remove(i);
    }
    if ui
        .add_enabled(d.links.len() < 20, egui::Button::new("添加链接"))
        .clicked()
    {
        d.links.push(Link::default())
    }
}
pub fn content(ui: &mut Ui, c: &mut ItemContent, date: Option<&mut DateInput>) {
    let wide = (ui.available_width() - 100.).clamp(160., 480.);
    match c {
        ItemContent::Education {
            school,
            degree,
            major,
            ..
        } => {
            let column = ((ui.available_width() - 220.) / 2.).max(100.);
            ui.horizontal_wrapped(|ui| {
                field(ui, "学校", school, column);
                field(ui, "学位", degree, 70.);
                field(ui, "专业", major, column);
            });
        }
        ItemContent::Work {
            company,
            role,
            department,
            location,
            ..
        } => {
            ui.horizontal_wrapped(|ui| {
                field(ui, "公司", company, wide * 0.55);
                field(ui, "职位", role, wide * 0.55);
            });
            ui.horizontal_wrapped(|ui| {
                field(ui, "部门", department, wide * 0.55);
                field(ui, "地点", location, wide * 0.55);
            });
        }
        ItemContent::Project {
            name,
            role,
            url,
            background,
            ..
        } => {
            ui.horizontal_wrapped(|ui| {
                field(ui, "项目", name, wide * 0.55);
                field(ui, "角色", role, wide * 0.55);
            });
            let mut enabled = url.is_some();
            if ui.checkbox(&mut enabled, "项目链接").changed() {
                *url = if enabled {
                    Some(Link {
                        label: "项目链接".into(),
                        url: String::new(),
                    })
                } else {
                    None
                }
            }
            if let Some(link) = url {
                ui.horizontal_wrapped(|ui| {
                    field(ui, "名称", &mut link.label, 100.);
                    field(ui, "网址", &mut link.url, wide);
                });
            }
            multiline(ui, "项目背景", background, 3);
        }
        ItemContent::Skill {
            name,
            category,
            description,
        } => {
            ui.horizontal_wrapped(|ui| {
                field(ui, "技能", name, wide * 0.6);
                field(ui, "细分标签", category, wide * 0.4);
            });
            multiline(ui, "描述", description, 4);
        }
        ItemContent::Custom {
            title, subtitle, ..
        } => {
            ui.horizontal_wrapped(|ui| {
                field(ui, "标题", title, wide * 0.55);
                field(ui, "副标题", subtitle, wide * 0.55);
            });
        }
    }
    if let Some(d) = date {
        dates(ui, d)
    }
}
pub fn style(ui: &mut Ui, s: &mut Style) {
    ui.horizontal_wrapped(|ui| {
        ui.label("字号");
        ui.add(
            egui::DragValue::new(&mut s.font_size)
                .range(9.0..=16.0)
                .speed(0.25),
        );
        ui.label("页边距 mm");
        ui.add(
            egui::DragValue::new(&mut s.margin_mm)
                .range(10.0..=30.0)
                .speed(0.5),
        );
        ui.label("行距");
        ui.add(
            egui::DragValue::new(&mut s.line_height)
                .range(1.2..=2.0)
                .speed(0.05),
        );
        ui.label("组间距 mm");
        ui.add(
            egui::DragValue::new(&mut s.section_gap_mm)
                .range(2.0..=12.0)
                .speed(0.5),
        );
    });
    ui.horizontal_wrapped(|ui| {
        egui::ComboBox::from_id_salt("font")
            .selected_text(if s.font_family == "resume-serif-sc" {
                "宋体风格"
            } else {
                "黑体风格"
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut s.font_family, "resume-sans-sc".into(), "黑体风格");
                ui.selectable_value(&mut s.font_family, "resume-serif-sc".into(), "宋体风格");
            });
        field(ui, "主题色", &mut s.accent, 85.);
    });
}
#[derive(Clone)]
pub struct Drag {
    pub scope: String,
    pub index: usize,
}
pub fn order_controls(
    ui: &mut Ui,
    scope: &str,
    index: usize,
    count: usize,
) -> Option<(usize, usize)> {
    let mut movement = None;
    ui.dnd_drag_source(
        egui::Id::new((scope, index)),
        Drag {
            scope: scope.into(),
            index,
        },
        |ui| {
            ui.label("≡").on_hover_text("拖动排序");
        },
    );
    if ui.add_enabled(index > 0, egui::Button::new("↑")).clicked() {
        movement = Some((index, index - 1))
    }
    if ui
        .add_enabled(index + 1 < count, egui::Button::new("↓"))
        .clicked()
    {
        movement = Some((index, index + 1))
    }
    movement
}
pub fn row_drop(response: &egui::Response, scope: &str, index: usize) -> Option<(usize, usize)> {
    response
        .dnd_release_payload::<Drag>()
        .filter(|d| d.scope == scope)
        .map(|d| (d.index, index))
}
pub fn state_picker(ui: &mut Ui, state: &mut RecordState) {
    ui.horizontal(|ui| {
        ui.selectable_value(state, RecordState::Active, "使用中");
        ui.selectable_value(state, RecordState::Archived, "归档");
        ui.selectable_value(state, RecordState::Trashed, "回收站");
    });
}
pub fn text_preview(ui: &mut Ui, d: &ResumeDocument) {
    ui.heading(&d.profile.name);
    ui.label(&d.profile.title);
    ui.label(
        [&d.profile.phone, &d.profile.email, &d.profile.location]
            .into_iter()
            .filter(|s| !s.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join(" · "),
    );
    for f in &d.profile.custom_fields {
        ui.label(format!("{}：{}", f.label, f.value));
    }
    for l in &d.profile.links {
        ui.hyperlink_to(&l.label, &l.url);
    }
    if !d.profile.summary.is_empty() {
        ui.label(&d.profile.summary);
    }
    if d.format_version == 2 {
        for s in &d.sections {
            let blocks: Vec<_> = s
                .block_ids
                .iter()
                .filter_map(|id| d.blocks.iter().find(|b| &b.id == id && b.visible))
                .collect();
            if blocks.is_empty() {
                continue;
            }
            ui.add_space(12.);
            ui.strong(&s.title);
            ui.separator();
            for b in blocks {
                block_preview(ui, b)
            }
        }
    } else {
        for b in &d.blocks {
            if b.visible {
                block_preview(ui, b)
            }
        }
    }
}
fn block_preview(ui: &mut Ui, b: &ResumeBlock) {
    ui.add_space(6.);
    ui.strong(b.content.title());
    match &b.content {
        ItemContent::Education {
            degree,
            major,
            dates,
            ..
        } => {
            ui.label(format!("{} · {} · {}", degree, major, date_label(dates)));
        }
        ItemContent::Work {
            role,
            department,
            location,
            dates,
            ..
        } => {
            ui.label(format!(
                "{} · {} · {} · {}",
                role,
                department,
                location,
                date_label(dates)
            ));
        }
        ItemContent::Project {
            role,
            dates,
            background,
            url,
            ..
        } => {
            ui.label(format!("{} · {}", role, date_label(dates)));
            ui.label(background);
            if let Some(l) = url {
                ui.hyperlink_to(&l.label, &l.url);
            }
        }
        ItemContent::Skill { description, .. } => {
            ui.label(description);
        }
        ItemContent::Custom {
            subtitle, dates, ..
        } => {
            ui.label(format!("{} · {}", subtitle, date_label(dates)));
        }
    }
    for a in &b.achievements {
        ui.label(format!("• {}", a.text));
    }
}
pub fn date_label(d: &DateRange) -> String {
    let a = crate::editors::format_date(d.start);
    let b = if d.ongoing {
        "至今".into()
    } else {
        crate::editors::format_date(d.end)
    };
    if a.is_empty() && b.is_empty() {
        String::new()
    } else {
        format!("{a} — {b}")
    }
}
