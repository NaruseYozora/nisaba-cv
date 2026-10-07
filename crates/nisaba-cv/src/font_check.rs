use resume_core::{
    Error, Result,
    model::{ItemContent, ResumeDocument},
};
use skrifa::{FontRef, MetadataProvider};
use std::{collections::BTreeSet, path::Path};

/// Check visible text before Typst can silently emit a missing-glyph box.
pub fn validate(bundle: &Path, d: &ResumeDocument) -> Result<()> {
    let p = &d.profile;
    let mut text = vec![
        p.name.as_str(),
        &p.title,
        &p.phone,
        &p.email,
        &p.location,
        &p.summary,
    ];
    for f in &p.custom_fields {
        text.extend([f.label.as_str(), &f.value]);
    }
    for l in &p.links {
        text.extend([l.label.as_str(), &l.url]);
    }
    for section in &d.sections {
        if d.blocks
            .iter()
            .any(|b| b.visible && section.block_ids.contains(&b.id))
        {
            text.push(&section.title);
        }
    }
    if d.format_version == 1 {
        text.extend(d.style.module_titles.values().map(String::as_str));
    }
    for b in d.blocks.iter().filter(|b| b.visible) {
        match &b.content {
            ItemContent::Education {
                school,
                degree,
                major,
                ..
            } => text.extend([school.as_str(), degree, major]),
            ItemContent::Work {
                company,
                role,
                department,
                location,
                ..
            } => text.extend([company.as_str(), role, department, location]),
            ItemContent::Project {
                name,
                role,
                background,
                url,
                ..
            } => {
                text.extend([name.as_str(), role, background]);
                if let Some(l) = url {
                    text.push(&l.label);
                }
            }
            ItemContent::Skill {
                name, description, ..
            } => text.extend([name.as_str(), description]),
            ItemContent::Custom {
                title, subtitle, ..
            } => text.extend([title.as_str(), subtitle]),
        }
        text.extend(b.achievements.iter().map(|a| a.text.as_str()));
    }
    let family = if d.style.font_family == "resume-serif-sc" {
        "ResumeSerifSC"
    } else {
        "ResumeSansSC"
    };
    let files = [
        format!("{family}-Regular.ttf"),
        format!("{family}-Semibold.ttf"),
        "NisabaCJKFallback-Regular.ttf".into(),
    ];
    let bytes = files
        .iter()
        .map(|f| std::fs::read(bundle.join("fonts").join(f)))
        .collect::<std::io::Result<Vec<_>>>()?;
    let fonts = bytes
        .iter()
        .map(|b| FontRef::new(b).map_err(|e| Error::Invalid(format!("字体无法读取：{e}"))))
        .collect::<Result<Vec<_>>>()?;
    let maps = fonts.iter().map(|f| f.charmap()).collect::<Vec<_>>();
    let missing = text
        .iter()
        .flat_map(|s| s.chars())
        .filter(|c| {
            !c.is_whitespace()
                && !matches!(
                    *c,
                    '\u{200b}' | '\u{200c}' | '\u{200d}' | '\u{feff}' | '\u{fe0e}' | '\u{fe0f}'
                )
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .filter(|c| {
            maps.iter()
                .all(|m| m.map(*c).is_none_or(|id| id.to_u32() == 0))
        })
        .take(12)
        .map(|c| format!("{c} (U+{:04X})", c as u32))
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(Error::Invalid(format!(
            "随包字体无法显示以下字符，已停止导出以避免缺字：{}。请替换字符后重试。",
            missing.join("、")
        )))
    }
}
