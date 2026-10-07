use crate::{Error, Result};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::Path,
};
use tempfile::NamedTempFile;

pub fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
pub fn lock(root: &Path) -> Result<File> {
    fs::create_dir_all(root)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(root.join("library.lock"))?;
    file.try_lock()
        .map_err(|_| Error::Invalid("此资料库已在另一个应用实例中打开".into()))?;
    Ok(file)
}
pub fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Invalid("文件路径无效".into()))?;
    let mut temp = NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    temp.as_file().sync_all()?;
    replace(temp.path(), path)?;
    sync_directory(parent)?;
    Ok(())
}
pub fn replace(source: &Path, target: &Path) -> Result<()> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows::{
            Win32::Storage::FileSystem::{
                MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
            },
            core::PCWSTR,
        };
        let from: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe {
            MoveFileExW(
                PCWSTR(from.as_ptr()),
                PCWSTR(to.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(|e| Error::Io(std::io::Error::from_raw_os_error(e.code().0 & 0xffff)))?;
    }
    #[cfg(not(windows))]
    {
        fs::rename(source, target)?;
    }
    Ok(())
}
/// Detect a changed destination and keep old bytes intact until replacement succeeds.
pub fn pdf_target_hash(path: &Path) -> Result<Option<String>> {
    match fs::metadata(path) {
        Ok(m) => {
            crate::error::require(
                m.is_file() && m.len() <= 32 * 1024 * 1024,
                "目标文件过大或不是普通文件，请另存新文件名",
            )?;
            Ok(Some(hash(&fs::read(path)?)))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}
pub fn publish_pdf(path: &Path, bytes: &[u8], expected: Option<&str>) -> Result<()> {
    crate::error::require(
        bytes.starts_with(b"%PDF-") && bytes.len() <= 32 * 1024 * 1024,
        "PDF 无效或超过 32MB",
    )?;
    crate::error::require(
        pdf_target_hash(path)?.as_deref() == expected,
        "目标文件在导出期间已改变，请另存新文件名",
    )?;
    let parent = path
        .parent()
        .ok_or_else(|| Error::Invalid("PDF 路径无效".into()))?;
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(bytes)?;
    temporary.as_file().sync_all()?;
    crate::error::require(
        hash(&fs::read(temporary.path())?) == hash(bytes),
        "PDF 临时文件校验失败",
    )?;
    // Check once more after the potentially slow temporary write.
    crate::error::require(
        pdf_target_hash(path)?.as_deref() == expected,
        "目标文件已改变，请另存新文件名",
    )?;
    replace(temporary.path(), path)?;
    sync_directory(parent)
}
pub fn sync_directory(path: &Path) -> Result<()> {
    #[cfg(not(windows))]
    {
        File::open(path)?.sync_all()?;
    }
    #[cfg(windows)]
    {
        let _ = path; /* MoveFileEx uses WRITE_THROUGH; regular files are synced before it. */
    }
    Ok(())
}
