use crate::{Result, error::require};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub const DOCUMENT_VERSION: u32 = 2;
pub const LEGACY_DOCUMENT_VERSION: u32 = 1;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomField {
    pub id: String,
    pub label: String,
    pub value: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Link {
    pub label: String,
    pub url: String,
}
impl Link {
    pub fn validate(&self) -> Result<()> {
        text(&self.label, 120)?;
        require(
            self.url.starts_with("https://") || self.url.starts_with("http://"),
            "链接须以 http:// 或 https:// 开头",
        )?;
        text(&self.url, 2048)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProfileDraft {
    pub name: String,
    pub title: String,
    pub phone: String,
    pub email: String,
    pub location: String,
    pub summary: String,
    pub links: Vec<Link>,
    pub photo_asset_id: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_fields: Vec<CustomField>,
}
impl ProfileDraft {
    pub fn validate(&self) -> Result<()> {
        for value in [
            &self.name,
            &self.title,
            &self.phone,
            &self.email,
            &self.location,
        ] {
            text(value, 256)?;
        }
        text(&self.summary, 10000)?;
        require(self.links.len() <= 20, "链接过多")?;
        for link in &self.links {
            link.validate()?;
        }
        require(self.custom_fields.len() <= 50, "自定义信息过多")?;
        unique(
            self.custom_fields.iter().map(|f| f.id.as_str()),
            "自定义信息 ID 重复或为空",
        )?;
        let mut labels = HashSet::new();
        for field in &self.custom_fields {
            require(
                uuid::Uuid::parse_str(&field.id).is_ok(),
                "自定义信息 ID 无效",
            )?;
            require(
                !field.label.trim().is_empty() && labels.insert(field.label.trim().to_lowercase()),
                "自定义信息名称为空或重复",
            )?;
            text(&field.label, 80)?;
            text(&field.value, 2048)?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PartialDate {
    pub year: u16,
    pub month: Option<u8>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DateRange {
    pub start: Option<PartialDate>,
    pub end: Option<PartialDate>,
    pub ongoing: bool,
}
impl DateRange {
    pub fn validate(&self) -> Result<()> {
        for date in [&self.start, &self.end].into_iter().flatten() {
            require(
                (1900..=2200).contains(&date.year)
                    && date.month.is_none_or(|m| (1..=12).contains(&m)),
                "日期无效",
            )?;
        }
        require(
            !self.ongoing || self.end.is_none(),
            "至今与结束日期不能同时填写",
        )?;
        if let (Some(start), Some(end)) = (self.start, self.end) {
            require(
                start.year < end.year
                    || (start.year == end.year
                        && match (start.month, end.month) {
                            (Some(a), Some(b)) => a <= b,
                            _ => true,
                        }),
                "结束日期早于开始日期",
            )?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum ItemContent {
    Education {
        school: String,
        degree: String,
        major: String,
        dates: DateRange,
    },
    Work {
        company: String,
        role: String,
        department: String,
        location: String,
        dates: DateRange,
    },
    Project {
        name: String,
        role: String,
        url: Option<Link>,
        background: String,
        dates: DateRange,
    },
    Skill {
        name: String,
        category: String,
        description: String,
    },
    Custom {
        category: String,
        title: String,
        subtitle: String,
        dates: DateRange,
    },
}
impl ItemContent {
    pub fn title(&self) -> &str {
        match self {
            Self::Education { school, .. } => school,
            Self::Work { company, .. } => company,
            Self::Project { name, .. } | Self::Skill { name, .. } => name,
            Self::Custom { title, .. } => title,
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Education { .. } => "education",
            Self::Work { .. } => "work",
            Self::Project { .. } => "project",
            Self::Skill { .. } => "skill",
            Self::Custom { .. } => "custom",
        }
    }
    pub fn validate(&self) -> Result<()> {
        require(!self.title().trim().is_empty(), "素材名称不能为空")?;
        text(self.title(), 256)?;
        match self {
            Self::Education {
                degree,
                major,
                dates,
                ..
            } => {
                text(degree, 256)?;
                text(major, 256)?;
                dates.validate()?;
            }
            Self::Work {
                role,
                department,
                location,
                dates,
                ..
            } => {
                for s in [role, department, location] {
                    text(s, 256)?;
                }
                dates.validate()?;
            }
            Self::Project {
                role,
                url,
                background,
                dates,
                ..
            } => {
                text(role, 256)?;
                text(background, 10000)?;
                if let Some(u) = url {
                    u.validate()?;
                }
                dates.validate()?;
            }
            Self::Skill {
                category,
                description,
                ..
            } => {
                text(category, 256)?;
                text(description, 10000)?;
            }
            Self::Custom {
                category,
                subtitle,
                dates,
                ..
            } => {
                text(category, 256)?;
                text(subtitle, 256)?;
                dates.validate()?;
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AchievementDraft {
    pub id: Option<String>,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ItemDraft {
    pub content: ItemContent,
    pub tags: Vec<String>,
    pub notes: String,
    pub achievements: Vec<AchievementDraft>,
}
impl ItemDraft {
    pub fn validate(&self) -> Result<()> {
        self.content.validate()?;
        text(&self.notes, 10000)?;
        require(
            self.tags.len() <= 50 && self.achievements.len() <= 200,
            "标签或成果条目过多",
        )?;
        unique(self.tags.iter().map(String::as_str), "标签重复")?;
        unique(
            self.achievements.iter().filter_map(|a| a.id.as_deref()),
            "成果 ID 重复",
        )?;
        for tag in &self.tags {
            require(!tag.trim().is_empty(), "标签不能为空")?;
            text(tag, 80)?;
        }
        for a in &self.achievements {
            require(!a.text.trim().is_empty(), "成果正文不能为空")?;
            text(&a.text, 20000)?;
        }
        require(
            !matches!(self.content, ItemContent::Skill { .. }) || self.achievements.is_empty(),
            "技能不包含经历成果",
        )?;
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum RecordState {
    Active,
    Archived,
    Trashed,
}
impl RecordState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Archived => "archived",
            Self::Trashed => "trashed",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Achievement {
    pub id: String,
    pub text: String,
    pub position: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    pub id: String,
    pub category_id: String,
    pub revision: i64,
    pub state: RecordState,
    pub content: ItemContent,
    pub tags: Vec<String>,
    pub notes: String,
    pub achievements: Vec<Achievement>,
    pub updated_at: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub revision: i64,
    pub content: ProfileDraft,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
pub enum ProfileField {
    Name,
    Title,
    Phone,
    Email,
    Location,
    Summary,
    Links,
    Photo,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectionItem {
    pub item_id: String,
    pub achievement_ids: Vec<String>,
    pub include_background: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selection {
    pub profile_fields: Vec<ProfileField>,
    pub items: Vec<SelectionItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_field_ids: Vec<String>,
    /// None is the legacy flat format; Some (even empty) is grouped selection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sections: Option<Vec<SelectionSection>>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectionSection {
    pub category_id: String,
    pub items: Vec<SelectionItem>,
}
impl Selection {
    pub fn selected_items(&self) -> Vec<&SelectionItem> {
        match &self.sections {
            Some(sections) => sections.iter().flat_map(|s| &s.items).collect(),
            None => self.items.iter().collect(),
        }
    }
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
            && self.sections.as_ref().is_none_or(Vec::is_empty)
            && self.profile_fields.is_empty()
            && self.custom_field_ids.is_empty()
    }
    pub fn validate(&self) -> Result<()> {
        if let Some(sections) = &self.sections {
            require(self.items.is_empty(), "不能混用平铺与分组选材")?;
            require(sections.len() <= 100, "简历类别过多")?;
            unique(
                sections.iter().map(|s| s.category_id.as_str()),
                "选材类别重复或为空",
            )?;
        }
        require(
            self.sections.is_some() || self.custom_field_ids.is_empty(),
            "自定义信息须通过新版分组选材",
        )?;
        let items = self.selected_items();
        require(items.len() <= 300, "所选素材过多")?;
        require(self.custom_field_ids.len() <= 50, "自定义信息选项过多")?;
        unique(
            self.custom_field_ids.iter().map(String::as_str),
            "自定义信息选择重复或为空",
        )?;
        require(
            self.profile_fields.iter().collect::<HashSet<_>>().len() == self.profile_fields.len(),
            "个人字段重复",
        )?;
        unique(items.iter().map(|i| i.item_id.as_str()), "所选素材重复")?;
        for i in items {
            unique(i.achievement_ids.iter().map(String::as_str), "所选成果重复")?;
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Style {
    pub template: String,
    pub template_version: u32,
    pub font_size: f64,
    pub margin_mm: f64,
    pub accent: String,
    #[serde(default = "default_font")]
    pub font_family: String,
    #[serde(default = "default_line_height")]
    pub line_height: f64,
    #[serde(default = "default_section_gap")]
    pub section_gap_mm: f64,
    #[serde(default)]
    pub module_titles: std::collections::BTreeMap<String, String>,
}
fn default_font() -> String {
    "resume-sans-sc".into()
}
fn default_line_height() -> f64 {
    1.5
}
fn default_section_gap() -> f64 {
    5.0
}
impl Default for Style {
    fn default() -> Self {
        Self {
            template: "single-column".into(),
            template_version: 1,
            font_size: 11.0,
            margin_mm: 15.0,
            accent: "#245764".into(),
            font_family: default_font(),
            line_height: default_line_height(),
            section_gap_mm: default_section_gap(),
            module_titles: Default::default(),
        }
    }
}
impl Style {
    pub fn validate(&self) -> Result<()> {
        require(
            matches!(
                self.font_family.as_str(),
                "resume-sans-sc" | "resume-serif-sc"
            ),
            "字体无效",
        )?;
        require(
            (1.2..=2.0).contains(&self.line_height) && (2.0..=12.0).contains(&self.section_gap_mm),
            "行距或模块间距无效",
        )?;
        for (kind, title) in &self.module_titles {
            require(
                matches!(
                    kind.as_str(),
                    "education" | "work" | "project" | "skill" | "custom" | "summary"
                ),
                "模块标题类别无效",
            )?;
            text(title, 80)?;
        }
        require(
            self.template == "single-column" && self.template_version == 1,
            "模板版本无效",
        )?;
        require(
            (9.0..=16.0).contains(&self.font_size) && (10.0..=30.0).contains(&self.margin_mm),
            "字号或页边距无效",
        )?;
        require(
            self.accent.len() == 7
                && self.accent.starts_with('#')
                && self.accent[1..].chars().all(|c| c.is_ascii_hexdigit()),
            "颜色无效",
        )
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Source {
    pub item_id: String,
    pub revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeAchievement {
    pub id: String,
    pub source_id: String,
    pub text: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeBlock {
    pub id: String,
    pub source: Source,
    pub content: ItemContent,
    pub achievements: Vec<ResumeAchievement>,
    pub visible: bool,
    pub page_break_before: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeDocument {
    pub format_version: u32,
    pub profile: ProfileDraft,
    pub profile_source_revision: i64,
    pub blocks: Vec<ResumeBlock>,
    pub style: Style,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sections: Vec<ResumeSection>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeSection {
    pub id: String,
    /// Historical source identifier; does not require a live library category.
    pub category_id: String,
    pub title: String,
    pub kind: String,
    pub block_ids: Vec<String>,
}
impl ResumeDocument {
    pub fn validate(&self) -> Result<()> {
        require(
            matches!(
                self.format_version,
                LEGACY_DOCUMENT_VERSION | DOCUMENT_VERSION
            ),
            "简历内容格式不支持",
        )?;
        self.profile.validate()?;
        self.style.validate()?;
        require(self.blocks.len() <= 300, "简历素材过多")?;
        unique(
            self.blocks.iter().map(|b| b.id.as_str()),
            "简历区块 ID 重复",
        )?;
        if self.format_version == LEGACY_DOCUMENT_VERSION {
            require(
                self.profile.custom_fields.is_empty(),
                "添加自定义信息前须转换为分组简历",
            )?;
            require(self.sections.is_empty(), "旧版简历不能携带分组结构")?;
        } else {
            crate::grouping::validate_sections(self)?;
        }
        for b in &self.blocks {
            b.content.validate()?;
            require(
                (b.source.item_id.is_empty() && b.source.revision == 0)
                    || (!b.source.item_id.is_empty() && b.source.revision > 0),
                "简历来源标识无效",
            )?;
            if b.content.kind() == "skill" {
                require(b.achievements.is_empty(), "技能不包含成果条目")?;
            }
            require(b.achievements.len() <= 200, "简历成果过多")?;
            unique(
                b.achievements.iter().map(|a| a.id.as_str()),
                "简历成果 ID 重复",
            )?;
            for a in &b.achievements {
                text(&a.text, 20000)?;
            }
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Resume {
    pub id: String,
    pub revision: i64,
    pub name: String,
    pub company: String,
    pub role: String,
    pub state: RecordState,
    pub document: ResumeDocument,
    pub updated_at: u64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResumeDraft {
    pub name: String,
    pub company: String,
    pub role: String,
    pub document: ResumeDocument,
}
impl ResumeDraft {
    pub fn validate(&self) -> Result<()> {
        require(!self.name.trim().is_empty(), "简历名称不能为空")?;
        for s in [&self.name, &self.company, &self.role] {
            text(s, 256)?;
        }
        self.document.validate()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub id: String,
    pub resume_id: String,
    pub name: String,
    pub document: ResumeDocument,
    pub pdf_asset_id: Option<String>,
    pub created_at: u64,
    pub company: String,
    pub role: String,
    pub source_revision: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresetDraft {
    pub name: String,
    pub selection: Selection,
    pub style: Style,
}
impl PresetDraft {
    pub fn validate(&self) -> Result<()> {
        require(!self.name.trim().is_empty(), "预设名称不能为空")?;
        text(&self.name, 256)?;
        self.selection.validate()?;
        self.style.validate()
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Preset {
    pub id: String,
    pub revision: i64,
    pub kind: PresetKind,
    pub content: PresetDraft,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum PresetKind {
    #[default]
    Selection,
    Layout,
}
impl PresetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Selection => "selection",
            Self::Layout => "layout",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Counts {
    pub items: usize,
    pub achievements: usize,
    pub resumes: usize,
    pub snapshots: usize,
    pub assets: usize,
    pub presets: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BackupSettings {
    pub directory: Option<String>,
    pub rolling_enabled: bool,
    pub rolling_keep: u8,
    pub daily_keep: u8,
}
impl Default for BackupSettings {
    fn default() -> Self {
        Self {
            directory: None,
            rolling_enabled: true,
            rolling_keep: 10,
            daily_keep: 7,
        }
    }
}
impl BackupSettings {
    pub fn validate(&self) -> Result<()> {
        require(
            (1..=50).contains(&self.rolling_keep) && (1..=50).contains(&self.daily_keep),
            "备份保留数量无效",
        )?;
        if let Some(path) = &self.directory {
            text(path, 2048)?;
            require(
                std::path::Path::new(path).is_absolute(),
                "备份目录必须是完整路径",
            )?;
        }
        Ok(())
    }
}
pub fn text(value: &str, max: usize) -> Result<()> {
    require(
        value.chars().count() <= max && !value.contains('\0'),
        "文本过长或含无效字符",
    )
}
pub fn unique<'a>(values: impl Iterator<Item = &'a str>, message: &str) -> Result<()> {
    let mut seen = HashSet::new();
    for value in values {
        require(!value.is_empty() && seen.insert(value), message)?;
    }
    Ok(())
}
