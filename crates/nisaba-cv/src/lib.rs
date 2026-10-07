pub mod app;
pub mod editors;
pub mod font_check;
pub mod history_pdf;
pub mod qa;
pub mod render;
mod selection_ui;
mod ui;
pub mod widgets;

use eframe::egui;
use std::path::Path;

pub fn configure(ctx: &egui::Context, bundle: &Path) -> std::io::Result<()> {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "nisaba-cn".into(),
        egui::FontData::from_owned(std::fs::read(
            bundle.join("fonts/ResumeSansSC-Regular.ttf"),
        )?)
        .into(),
    );
    fonts
        .families
        .get_mut(&egui::FontFamily::Proportional)
        .unwrap()
        .insert(0, "nisaba-cn".into());
    fonts
        .families
        .get_mut(&egui::FontFamily::Monospace)
        .unwrap()
        .push("nisaba-cn".into());
    let fallback = bundle.join("fonts/NisabaCJKFallback-Regular.ttf");
    fonts.font_data.insert(
        "nisaba-fallback".into(),
        egui::FontData::from_owned(std::fs::read(fallback)?).into(),
    );
    for family in [egui::FontFamily::Proportional, egui::FontFamily::Monospace] {
        fonts
            .families
            .get_mut(&family)
            .unwrap()
            .push("nisaba-fallback".into());
    }
    ctx.set_fonts(fonts);
    ctx.set_theme(egui::Theme::Light);
    ctx.set_visuals(egui::Visuals::light());
    let mut style = (*ctx.style_of(egui::Theme::Light)).clone();
    style.spacing.item_spacing = egui::vec2(8., 8.);
    style.spacing.button_padding = egui::vec2(10., 6.);
    style
        .text_styles
        .insert(egui::TextStyle::Body, egui::FontId::proportional(14.));
    style
        .text_styles
        .insert(egui::TextStyle::Button, egui::FontId::proportional(14.));
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(20.));
    style.visuals.panel_fill = egui::Color32::from_rgb(248, 249, 248);
    style.visuals.selection.bg_fill = egui::Color32::from_rgb(198, 222, 221);
    style.visuals.selection.stroke.color = egui::Color32::from_rgb(25, 62, 69);
    ctx.set_style_of(egui::Theme::Light, style);
    Ok(())
}
pub fn icon() -> image::ImageResult<image::RgbaImage> {
    Ok(image::load_from_memory(include_bytes!("../../../assets/nisaba-icon.png"))?.into_rgba8())
}
