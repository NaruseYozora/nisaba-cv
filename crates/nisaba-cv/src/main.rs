#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use eframe::egui;
use nisaba_cv::{app::App, configure, icon};
use std::path::PathBuf;

fn run() -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let args = std::env::args().collect::<Vec<_>>();
    let option = |key: &str| {
        args.windows(2)
            .find(|a| a[0] == key)
            .map(|a| PathBuf::from(&a[1]))
    };
    let exe = std::env::current_exe()?;
    let root = exe.parent().ok_or("无法确定程序目录")?;
    let bundle = option("--bundle").unwrap_or_else(|| root.to_owned());
    let data = option("--data").unwrap_or_else(|| root.join("nisaba-data"));
    if args.iter().any(|a| a == "--self-test") {
        if option("--data").is_none() {
            return Err("自测必须显式指定空的 --data 目录".into());
        }
        nisaba_cv::qa::self_test(&data, &bundle)?;
        return Ok(());
    }
    // Portable data stays independent. Upgrade is explicit backup import in Settings.
    let native_check = option("--native-check");
    let mut app = App::open(data, bundle.clone())?;
    if let Some(directory) = option("--native-check") {
        app.qa = Some(nisaba_cv::qa::Qa::new(directory)?);
    }
    let image = icon()?;
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1120., 780.])
            .with_min_inner_size([800., 560.])
            .with_icon(egui::IconData {
                rgba: image.clone().into_raw(),
                width: image.width(),
                height: image.height(),
            }),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Nisaba CV",
        options,
        Box::new(move |cc| {
            configure(&cc.egui_ctx, &bundle)?;
            if app.qa.is_some()
                && let Some(scale) = args
                    .windows(2)
                    .find(|a| a[0] == "--native-scale")
                    .and_then(|a| a[1].parse::<f32>().ok())
            {
                cc.egui_ctx.set_zoom_factor(scale.clamp(0.75, 2.5));
            }
            let mut app = app;
            app.icon = Some(cc.egui_ctx.load_texture(
                "nisaba",
                egui::ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    &image.into_raw(),
                ),
                Default::default(),
            ));
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| e.to_string())?;
    if let Some(directory) = native_check {
        std::fs::write(directory.join("native-exited.txt"), "event loop returned")?;
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        if !std::env::args().any(|a| a == "--self-test") {
            rfd::MessageDialog::new()
                .set_title("Nisaba CV")
                .set_description(format!("无法打开资料库：{error}"))
                .set_level(rfd::MessageLevel::Error)
                .show();
        }
        std::process::exit(1)
    }
}
