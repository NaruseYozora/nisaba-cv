use eframe::egui::{self, epaint::Shape};
use nisaba_cv::widgets;
use resume_core::{Store, model::*};

fn frame(
    ctx: &egui::Context,
    accent: &mut String,
    events: Vec<egui::Event>,
) -> (egui::FullOutput, egui::Rect) {
    let mut rect = egui::Rect::NOTHING;
    let mut out = ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800., 600.),
            )),
            events,
            focused: true,
            ..Default::default()
        },
        |ui| {
            rect = widgets::accent_picker(ui, accent).rect;
        },
    );
    out.textures_delta.clear();
    (out, rect)
}
fn click(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

#[test]
fn graphical_color_selection_changes_theme_and_round_trips_through_storage() {
    let ctx = egui::Context::default();
    let mut accent = "#245764".to_string();
    let (_, rect) = frame(&ctx, &mut accent, vec![]);
    frame(&ctx, &mut accent, click(rect.center(), true));
    frame(&ctx, &mut accent, click(rect.center(), false));
    let (out, _) = frame(&ctx, &mut accent, vec![]);
    // The saturation/value square is the largest square gradient in the popup.
    let square = out
        .shapes
        .iter()
        .filter_map(|s| {
            if let Shape::Mesh(m) = &s.shape {
                let r = m.calc_bounds();
                (r.width() > 100. && (r.width() - r.height()).abs() < 2.).then_some(r)
            } else {
                None
            }
        })
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .expect("graphical picker opened");
    let pos = square.min + square.size() * egui::vec2(0.8, 0.2);
    frame(&ctx, &mut accent, click(pos, true));
    frame(&ctx, &mut accent, click(pos, false));
    assert_ne!(accent, "#245764");
    let style = Style {
        accent: accent.clone(),
        ..Default::default()
    };
    style.validate().unwrap();
    let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.test-output");
    std::fs::create_dir_all(&root).unwrap();
    let t = tempfile::tempdir_in(root).unwrap();
    let mut s = Store::open(t.path()).unwrap();
    let r = s
        .create_resume(
            "色彩测试",
            &Selection {
                sections: Some(vec![]),
                ..Default::default()
            },
            style.clone(),
        )
        .unwrap();
    let preset = s
        .save_preset_kind(
            None,
            None,
            PresetKind::Layout,
            PresetDraft {
                name: "自选色".into(),
                style,
                selection: Selection::default(),
            },
        )
        .unwrap();
    drop(s);
    let s = Store::open(t.path()).unwrap();
    assert_eq!(s.resume(&r.id).unwrap().document.style.accent, accent);
    assert_eq!(s.preset(&preset.id).unwrap().content.style.accent, accent);
}
