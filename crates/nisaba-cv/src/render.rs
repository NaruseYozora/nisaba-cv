use resume_core::{Error, Result, Store, model::Resume};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub struct Rendered {
    pub directory: PathBuf,
    pub pages: Vec<PathBuf>,
    pub resume_id: String,
    pub revision: i64,
    cache: Option<tempfile::TempDir>,
}
impl Rendered {
    pub fn new(root: &Path, resume_id: String, revision: i64) -> Result<Self> {
        let parent = root.join("previews");
        fs::create_dir_all(&parent)?;
        let cache = tempfile::Builder::new()
            .prefix("nisaba-render-")
            .tempdir_in(parent)?;
        fs::write(
            cache.path().join(".nisaba-render-cache"),
            b"Nisaba CV disposable preview v1",
        )?;
        Ok(Self {
            directory: cache.path().to_owned(),
            pages: vec![],
            resume_id,
            revision,
            cache: Some(cache),
        })
    }
    /// Only QA utilities keep disposable render files as test evidence.
    pub fn preserve(mut self) -> PathBuf {
        self.cache.take().unwrap().keep()
    }
}

/// Called only after the data store's exclusive lock has been acquired.
/// Any directory without our cache marker is deliberately left alone.
pub fn clean_abandoned(root: &Path) -> std::io::Result<()> {
    let parent = root.join("previews");
    if !parent.exists() {
        return Ok(());
    }
    if !plain_directory(&fs::symlink_metadata(&parent)?) {
        return Ok(());
    }
    for entry in fs::read_dir(parent)? {
        let entry = entry?;
        if plain_directory(&fs::symlink_metadata(entry.path())?)
            && entry
                .file_name()
                .to_string_lossy()
                .starts_with("nisaba-render-")
            && fs::read(entry.path().join(".nisaba-render-cache"))
                .ok()
                .as_deref()
                == Some(b"Nisaba CV disposable preview v1")
        {
            fs::remove_dir_all(entry.path())?;
        }
    }
    Ok(())
}

fn plain_directory(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if meta.file_attributes() & 0x400 != 0 {
            return false;
        }
    }
    meta.is_dir() && !meta.file_type().is_symlink()
}

pub fn render(s: &Store, bundle: &Path, root: &Path, frozen: &Resume) -> Result<Rendered> {
    frozen.document.validate()?;
    crate::font_check::validate(bundle, &frozen.document)?;
    let mut rendered = Rendered::new(root, frozen.id.clone(), frozen.revision)?;
    let directory = &rendered.directory;
    let mut json = serde_json::to_value(frozen)?;
    if let Some(id) = &frozen.document.profile.photo_asset_id {
        let (mime, bytes) = s.photo_asset(id)?;
        let name = if mime == "image/png" {
            "photo.png"
        } else {
            "photo.jpg"
        };
        fs::write(directory.join(name), bytes)?;
        json["photoFile"] = name.into();
    } else {
        json["photoFile"] = serde_json::Value::Null;
    }
    fs::write(
        directory.join("resume.json"),
        serde_json::to_vec_pretty(&json)?,
    )?;
    fs::write(directory.join("resume.typ"), include_str!("../resume.typ"))?;
    let cache = root.join("render-cache");
    fs::create_dir_all(&cache)?;
    for output in ["resume.pdf", "page-{0p}.png"] {
        let mut cmd = Command::new(bundle.join("tools/typst.exe"));
        cmd.args([
            "compile",
            "--ignore-system-fonts",
            "--ignore-embedded-fonts",
            "--creation-timestamp",
            "0",
            "--ppi",
            "96",
            "--jobs",
            "2",
        ])
        .arg("--root")
        .arg(directory)
        .arg("--font-path")
        .arg(bundle.join("fonts"))
        .arg("--package-path")
        .arg(&cache)
        .arg("--package-cache-path")
        .arg(&cache)
        .arg(directory.join("resume.typ"))
        .arg(directory.join(output))
        .env("HTTP_PROXY", "http://127.0.0.1:9")
        .env("HTTPS_PROXY", "http://127.0.0.1:9")
        .env("ALL_PROXY", "http://127.0.0.1:9")
        .env("NO_PROXY", "")
        .env("TEMP", &cache)
        .env("TMP", &cache);
        #[cfg(windows)]
        cmd.creation_flags(0x08000000);
        let result = cmd.output()?;
        fs::write(
            directory.join(if output.ends_with("pdf") {
                "pdf.log"
            } else {
                "preview.log"
            }),
            &result.stderr,
        )?;
        if !result.status.success() || !result.stderr.is_empty() {
            return Err(Error::Invalid(format!(
                "排版失败：{}",
                String::from_utf8_lossy(&result.stderr)
            )));
        }
    }
    let mut pages = fs::read_dir(directory)?
        .filter_map(|p| p.ok().map(|p| p.path()))
        .filter(|p| {
            p.extension().is_some_and(|e| e == "png")
                && p.file_stem().and_then(|s| s.to_str()).is_some_and(|s| {
                    s.strip_prefix("page-")
                        .is_some_and(|n| !n.is_empty() && n.bytes().all(|c| c.is_ascii_digit()))
                })
        })
        .collect::<Vec<_>>();
    pages.sort_by_key(|p| {
        p.file_stem()
            .unwrap()
            .to_string_lossy()
            .trim_start_matches("page-")
            .parse::<u32>()
            .unwrap_or(u32::MAX)
    });
    if pages.is_empty() {
        return Err(Error::Invalid("没有生成预览页面".into()));
    }
    rendered.pages = pages;
    Ok(rendered)
}
