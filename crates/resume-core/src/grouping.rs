use crate::{
    Error, Result, Store,
    catalog::{BUILTINS, valid_kind},
    error::require,
    files,
    model::*,
    store::{id, now, require_changed, resume_on, sync_refs},
};
use rusqlite::params;
use std::collections::{HashMap, HashSet};

pub fn validate_sections(document: &ResumeDocument) -> Result<()> {
    require(document.sections.len() <= 100, "简历类别过多")?;
    unique(
        document.sections.iter().map(|s| s.id.as_str()),
        "简历类别 ID 重复或为空",
    )?;
    unique(
        document.sections.iter().map(|s| s.category_id.as_str()),
        "同一类别不能拆分为多个分组",
    )?;
    let blocks: HashMap<_, _> = document.blocks.iter().map(|b| (b.id.as_str(), b)).collect();
    let mut ordered = Vec::new();
    for section in &document.sections {
        require(
            valid_kind(&section.kind) && !section.title.trim().is_empty(),
            "简历类别名称或结构类型无效",
        )?;
        text(&section.title, 256)?;
        text(&section.id, 256)?;
        text(&section.category_id, 256)?;
        for block_id in &section.block_ids {
            let block = blocks
                .get(block_id.as_str())
                .ok_or_else(|| Error::Missing(block_id.clone()))?;
            require(
                block.content.kind() == section.kind,
                "简历素材与所属类别不一致",
            )?;
            ordered.push(block_id.as_str());
        }
    }
    unique(ordered.iter().copied(), "简历素材重复分组")?;
    require(
        ordered
            == document
                .blocks
                .iter()
                .map(|b| b.id.as_str())
                .collect::<Vec<_>>(),
        "简历分组与素材顺序不一致或有遗漏",
    )
}
fn canonicalize(document: &mut ResumeDocument) -> Result<()> {
    let mut blocks: HashMap<_, _> = document
        .blocks
        .iter()
        .cloned()
        .map(|b| (b.id.clone(), b))
        .collect();
    let mut ordered = Vec::new();
    for section in &document.sections {
        for block_id in &section.block_ids {
            ordered.push(
                blocks
                    .remove(block_id)
                    .ok_or_else(|| Error::Missing(block_id.clone()))?,
            );
        }
    }
    require(blocks.is_empty(), "分组不能遗漏简历素材")?;
    document.blocks = ordered;
    document.validate()
}
/// Pure preview: frozen content defines grouping, never the current library text.
pub fn grouped_copy(source: &ResumeDocument) -> Result<ResumeDocument> {
    source.validate()?;
    if source.format_version == DOCUMENT_VERSION {
        return Ok(source.clone());
    }
    let mut document = source.clone();
    document.format_version = DOCUMENT_VERSION;
    for block in &source.blocks {
        let kind = block.content.kind();
        let (category_id, title) = match &block.content {
            ItemContent::Custom { category, .. }
                if !category.trim().is_empty() && category.trim() != "自定义" =>
            {
                (
                    format!(
                        "legacy-custom:{}",
                        files::hash(category.trim().to_lowercase().as_bytes())
                    ),
                    category.trim().to_owned(),
                )
            }
            _ => (
                format!("builtin:{kind}"),
                source
                    .style
                    .module_titles
                    .get(kind)
                    .filter(|s| !s.trim().is_empty())
                    .cloned()
                    .unwrap_or_else(|| BUILTINS.iter().find(|(k, _)| *k == kind).unwrap().1.into()),
            ),
        };
        if let Some(section) = document
            .sections
            .iter_mut()
            .find(|s| s.category_id == category_id)
        {
            section.block_ids.push(block.id.clone());
        } else {
            // Stable across preview/retry so UI selections don't jump.
            document.sections.push(ResumeSection {
                id: format!("section:{}", files::hash(category_id.as_bytes())),
                category_id,
                title,
                kind: kind.into(),
                block_ids: vec![block.id.clone()],
            });
        }
    }
    canonicalize(&mut document)?;
    Ok(document)
}
pub fn reorder_sections(document: &mut ResumeDocument, order: &[String]) -> Result<()> {
    document.validate()?;
    require(
        document.format_version == DOCUMENT_VERSION,
        "请先转换为分组简历",
    )?;
    unique(order.iter().map(String::as_str), "类别排序重复")?;
    require(
        order.len() == document.sections.len(),
        "类别排序必须包含全部分组",
    )?;
    let mut candidate = document.clone();
    candidate.sections = order
        .iter()
        .map(|id| {
            document
                .sections
                .iter()
                .find(|s| &s.id == id)
                .cloned()
                .ok_or_else(|| Error::Missing(id.clone()))
        })
        .collect::<Result<_>>()?;
    canonicalize(&mut candidate)?;
    *document = candidate;
    Ok(())
}
pub fn reorder_section_items(
    document: &mut ResumeDocument,
    section_id: &str,
    order: &[String],
) -> Result<()> {
    document.validate()?;
    let mut candidate = document.clone();
    let section = candidate
        .sections
        .iter_mut()
        .find(|s| s.id == section_id)
        .ok_or_else(|| Error::Missing(section_id.into()))?;
    unique(order.iter().map(String::as_str), "素材排序重复")?;
    require(
        order.iter().collect::<HashSet<_>>() == section.block_ids.iter().collect::<HashSet<_>>(),
        "只能调整当前类别内的素材顺序",
    )?;
    section.block_ids = order.to_vec();
    canonicalize(&mut candidate)?;
    *document = candidate;
    Ok(())
}
pub(crate) fn append_document(
    document: &mut ResumeDocument,
    addition: ResumeDocument,
) -> Result<()> {
    for incoming in addition.sections {
        if let Some(section) = document
            .sections
            .iter_mut()
            .find(|s| s.category_id == incoming.category_id)
        {
            require(section.kind == incoming.kind, "简历类别结构已改变")?;
            section.block_ids.extend(incoming.block_ids);
        } else {
            document.sections.push(incoming);
        }
    }
    document.blocks.extend(addition.blocks);
    canonicalize(document)
}
impl Store {
    /// Converts a legacy preset in memory using current library membership.
    /// Missing/archived sources must be resolved instead of silently discarded.
    pub fn grouped_selection(&self, selection: &Selection) -> Result<Selection> {
        let missing = self.inspect_selection(selection)?;
        require(
            missing.is_empty(),
            &format!("所选内容不可用：{}", missing.join("、")),
        )?;
        if selection.sections.is_some() {
            return Ok(selection.clone());
        }
        let mut grouped = selection.clone();
        let mut sections: Vec<SelectionSection> = vec![];
        for selected in &selection.items {
            let category_id = self.item(&selected.item_id)?.category_id;
            if let Some(section) = sections.iter_mut().find(|s| s.category_id == category_id) {
                section.items.push(selected.clone());
            } else {
                sections.push(SelectionSection {
                    category_id,
                    items: vec![selected.clone()],
                });
            }
        }
        grouped.items.clear();
        grouped.sections = Some(sections);
        grouped.validate()?;
        Ok(grouped)
    }
    /// Keeps a frozen pre-conversion snapshot in the same transaction as the rewrite.
    pub fn convert_resume_to_grouped(&mut self, resume_id: &str, revision: i64) -> Result<Resume> {
        let tx = self.conn_mut()?.transaction()?;
        let source = resume_on(&tx, resume_id)?;
        if source.revision != revision {
            return Err(Error::Conflict);
        }
        require(
            source.state != RecordState::Trashed,
            "请先恢复回收站中的简历",
        )?;
        if source.document.format_version == DOCUMENT_VERSION {
            return Ok(source);
        }
        let grouped = grouped_copy(&source.document)?;
        let snapshot_id = id();
        tx.execute("INSERT INTO snapshots(id,resume_id,name,document,pdf_asset_id,created_at,company,role,source_revision) VALUES (?,?,?,(SELECT document FROM resumes WHERE id=?),NULL,?,?,?,?)",params![snapshot_id,resume_id,"分组转换前",resume_id,now() as i64,source.company,source.role,source.revision])?;
        sync_refs(
            &tx,
            "snapshot",
            &snapshot_id,
            source.document.profile.photo_asset_id.iter().cloned(),
        )?;
        require_changed(tx.execute("UPDATE resumes SET revision=revision+1,document=?,updated_at=? WHERE id=? AND revision=?",params![serde_json::to_string(&grouped)?,now() as i64,resume_id,revision])?)?;
        tx.commit()?;
        self.resume(resume_id)
    }
}
