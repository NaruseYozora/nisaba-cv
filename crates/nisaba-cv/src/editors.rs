use resume_core::{Error, Result, catalog::ProfileEditorDraft, model::*, store::id};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub fn profile_draft(p: &ProfileDraft) -> ProfileEditorDraft {
    ProfileEditorDraft {
        name: p.name.clone(),
        title: p.title.clone(),
        phone: p.phone.clone(),
        email: p.email.clone(),
        location: p.location.clone(),
        links: p.links.clone(),
        photo_asset_id: p.photo_asset_id.clone(),
        custom_fields: p.custom_fields.clone(),
    }
}
pub fn empty_item(kind: &str, category: &str) -> ItemDraft {
    let content = match kind {
        "education" => ItemContent::Education {
            school: String::new(),
            degree: String::new(),
            major: String::new(),
            dates: DateRange::default(),
        },
        "work" => ItemContent::Work {
            company: String::new(),
            role: String::new(),
            department: String::new(),
            location: String::new(),
            dates: DateRange::default(),
        },
        "project" => ItemContent::Project {
            name: String::new(),
            role: String::new(),
            url: None,
            background: String::new(),
            dates: DateRange::default(),
        },
        "skill" => ItemContent::Skill {
            name: String::new(),
            category: String::new(),
            description: String::new(),
        },
        _ => ItemContent::Custom {
            category: category.into(),
            title: String::new(),
            subtitle: String::new(),
            dates: DateRange::default(),
        },
    };
    ItemDraft {
        content,
        tags: vec![],
        notes: String::new(),
        achievements: vec![],
    }
}
pub fn item_draft(i: &LibraryItem) -> ItemDraft {
    ItemDraft {
        content: i.content.clone(),
        tags: i.tags.clone(),
        notes: i.notes.clone(),
        achievements: i
            .achievements
            .iter()
            .map(|a| AchievementDraft {
                id: Some(a.id.clone()),
                text: a.text.clone(),
            })
            .collect(),
    }
}
pub fn dates_mut(c: &mut ItemContent) -> Option<&mut DateRange> {
    match c {
        ItemContent::Education { dates, .. }
        | ItemContent::Work { dates, .. }
        | ItemContent::Project { dates, .. }
        | ItemContent::Custom { dates, .. } => Some(dates),
        _ => None,
    }
}
pub fn format_date(d: Option<PartialDate>) -> String {
    d.map(|d| {
        d.month
            .map(|m| format!("{}-{m:02}", d.year))
            .unwrap_or_else(|| d.year.to_string())
    })
    .unwrap_or_default()
}
pub fn parse_date(s: &str) -> Result<Option<PartialDate>> {
    if s.trim().is_empty() {
        return Ok(None);
    }
    let parts: Vec<_> = s.trim().split(['-', '.', '/']).collect();
    if !(1..=2).contains(&parts.len()) {
        return Err(Error::Invalid("日期请填写 YYYY 或 YYYY-MM".into()));
    }
    let year = parts[0]
        .parse::<u16>()
        .map_err(|_| Error::Invalid("年份无效".into()))?;
    let month = parts
        .get(1)
        .map(|m| {
            m.parse::<u8>()
                .map_err(|_| Error::Invalid("月份无效".into()))
        })
        .transpose()?;
    let date = PartialDate { year, month };
    DateRange {
        start: Some(date),
        ..Default::default()
    }
    .validate()?;
    Ok(Some(date))
}
#[derive(Clone, Serialize, Deserialize)]
pub struct DateInput {
    pub start: String,
    pub end: String,
    pub ongoing: bool,
}
impl DateInput {
    pub fn new(c: &ItemContent) -> Option<Self> {
        let mut c = c.clone();
        dates_mut(&mut c).map(|d| Self {
            start: format_date(d.start),
            end: format_date(d.end),
            ongoing: d.ongoing,
        })
    }
    pub fn apply(&self, c: &mut ItemContent) -> Result<()> {
        if let Some(d) = dates_mut(c) {
            *d = DateRange {
                start: parse_date(&self.start)?,
                end: if self.ongoing {
                    None
                } else {
                    parse_date(&self.end)?
                },
                ongoing: self.ongoing,
            };
            d.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ItemEditor {
    pub id: Option<String>,
    pub revision: Option<i64>,
    pub category_id: String,
    pub draft: ItemDraft,
    pub tags: String,
    pub date: Option<DateInput>,
}
impl ItemEditor {
    pub fn new(category_id: String, draft: ItemDraft, source: Option<&LibraryItem>) -> Self {
        Self {
            id: source.map(|i| i.id.clone()),
            revision: source.map(|i| i.revision),
            category_id,
            tags: draft.tags.join("，"),
            date: DateInput::new(&draft.content),
            draft,
        }
    }
    pub fn validated(&self) -> Result<ItemDraft> {
        let mut draft = self.draft.clone();
        draft.tags = self
            .tags
            .split([',', '，'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        if let Some(date) = &self.date {
            date.apply(&mut draft.content)?
        }
        draft.validate()?;
        Ok(draft)
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub struct ResumeEditor {
    pub id: String,
    pub revision: i64,
    pub draft: ResumeDraft,
    pub dates: HashMap<String, DateInput>,
}
impl ResumeEditor {
    pub fn new(r: Resume) -> Self {
        let dates = r
            .document
            .blocks
            .iter()
            .filter_map(|b| DateInput::new(&b.content).map(|d| (b.id.clone(), d)))
            .collect();
        Self {
            id: r.id,
            revision: r.revision,
            draft: ResumeDraft {
                name: r.name,
                company: r.company,
                role: r.role,
                document: r.document,
            },
            dates,
        }
    }
    pub fn validated(&self) -> Result<ResumeDraft> {
        let mut d = self.draft.clone();
        for b in &mut d.document.blocks {
            if let Some(date) = self.dates.get(&b.id) {
                date.apply(&mut b.content)?
            }
        }
        d.validate()?;
        Ok(d)
    }
    pub fn remove_block(&mut self, id: &str) {
        self.draft.document.blocks.retain(|b| b.id != id);
        for s in &mut self.draft.document.sections {
            s.block_ids.retain(|b| b != id)
        }
        self.draft
            .document
            .sections
            .retain(|s| !s.block_ids.is_empty());
        self.dates.remove(id);
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub enum Editor {
    Profile {
        revision: i64,
        draft: ProfileEditorDraft,
    },
    Item(ItemEditor),
    Resume(Box<ResumeEditor>),
}
impl Editor {
    pub fn key(&self) -> String {
        serde_json::to_string(self).expect("serializable editor")
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Picker {
    pub name: String,
    pub company: String,
    pub role: String,
    pub selection: Selection,
    pub style: Style,
    pub append_to: Option<(String, i64)>,
    pub preset_name: String,
    pub preset_id: Option<(String, i64)>,
}
impl Default for Picker {
    fn default() -> Self {
        Self {
            name: String::new(),
            company: String::new(),
            role: String::new(),
            selection: Selection {
                profile_fields: vec![
                    ProfileField::Name,
                    ProfileField::Title,
                    ProfileField::Phone,
                    ProfileField::Email,
                    ProfileField::Location,
                    ProfileField::Links,
                ],
                sections: Some(vec![]),
                ..Default::default()
            },
            style: Style::default(),
            append_to: None,
            preset_name: String::new(),
            preset_id: None,
        }
    }
}
impl Picker {
    pub fn toggle_item(&mut self, item: &LibraryItem, selected: bool) {
        let sections = self.selection.sections.as_mut().expect("grouped picker");
        if selected {
            if !sections.iter().any(|s| s.category_id == item.category_id) {
                sections.push(SelectionSection {
                    category_id: item.category_id.clone(),
                    items: vec![],
                })
            }
            let s = sections
                .iter_mut()
                .find(|s| s.category_id == item.category_id)
                .unwrap();
            if !s.items.iter().any(|i| i.item_id == item.id) {
                s.items.push(SelectionItem {
                    item_id: item.id.clone(),
                    achievement_ids: item.achievements.iter().map(|a| a.id.clone()).collect(),
                    include_background: true,
                })
            }
        } else {
            for s in sections.iter_mut() {
                s.items.retain(|i| i.item_id != item.id)
            }
            sections.retain(|s| !s.items.is_empty())
        }
    }
    pub fn selected_mut(&mut self, id: &str) -> Option<&mut SelectionItem> {
        self.selection
            .sections
            .as_mut()?
            .iter_mut()
            .flat_map(|s| &mut s.items)
            .find(|i| i.item_id == id)
    }
    pub fn selected(&self, id: &str) -> bool {
        self.selection
            .selected_items()
            .iter()
            .any(|i| i.item_id == id)
    }
}
pub fn move_index<T>(v: &mut Vec<T>, from: usize, to: usize) {
    if from < v.len() && to < v.len() && from != to {
        let value = v.remove(from);
        v.insert(to, value)
    }
}
pub fn new_custom_field() -> CustomField {
    CustomField {
        id: id(),
        label: String::new(),
        value: String::new(),
    }
}
