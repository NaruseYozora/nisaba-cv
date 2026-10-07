//! Display the original PDF bytes with Windows' built-in PDF renderer. No browser
//! engine, downloads, or re-typesetting of a historical document are involved.
use crate::render::Rendered;
use resume_core::{Error, Result};
use std::path::Path;

pub fn render(root: &Path, snapshot_id: &str, pdf: &[u8]) -> Result<Rendered> {
    let mut result = Rendered::new(root, format!("snapshot:{snapshot_id}"), 0)?;
    std::fs::write(result.directory.join("resume.pdf"), pdf)?;
    #[cfg(windows)]
    {
        result.pages = windows_render(&result.directory, pdf)
            .map_err(|e| Error::Invalid(format!("历史 PDF 预览失败，请使用另存历史 PDF：{e}")))?;
        Ok(result)
    }
    #[cfg(not(windows))]
    {
        Err(Error::Invalid("当前系统不支持历史 PDF 预览".into()))
    }
}

#[cfg(windows)]
fn windows_render(directory: &Path, pdf: &[u8]) -> windows::core::Result<Vec<std::path::PathBuf>> {
    use windows::{
        Data::Pdf::{PdfDocument, PdfPageRenderOptions},
        Storage::Streams::{DataReader, DataWriter, InMemoryRandomAccessStream},
        Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
    };
    // This function runs on the data worker, never the GUI's STA thread.
    unsafe {
        RoInitialize(RO_INIT_MULTITHREADED)?;
    }
    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                RoUninitialize();
            }
        }
    }
    let _apartment = Apartment;
    let input = InMemoryRandomAccessStream::new()?;
    let writer = DataWriter::CreateDataWriter(&input.GetOutputStreamAt(0)?)?;
    writer.WriteBytes(pdf)?;
    writer.StoreAsync()?.join()?;
    writer.FlushAsync()?.join()?;
    writer.DetachStream()?;
    input.Seek(0)?;
    let document = PdfDocument::LoadFromStreamAsync(&input)?.join()?;
    let count = document.PageCount()?;
    if count == 0 || count > 500 {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80070057u32 as i32),
            "PDF 页数不在 1–500 范围内",
        ));
    }
    let mut pages = vec![];
    for index in 0..count {
        let page = document.GetPage(index)?;
        let output = InMemoryRandomAccessStream::new()?;
        let options = PdfPageRenderOptions::new()?;
        options.SetDestinationWidth(1000)?;
        page.RenderWithOptionsToStreamAsync(&output, &options)?
            .join()?;
        let size = output.Size()?;
        if size > 32 * 1024 * 1024 {
            return Err(windows::core::Error::new(
                windows::core::HRESULT(0x80070057u32 as i32),
                "PDF 预览图过大",
            ));
        }
        let reader = DataReader::CreateDataReader(&output.GetInputStreamAt(0)?)?;
        reader.LoadAsync(size as u32)?.join()?;
        let mut bytes = vec![0; size as usize];
        reader.ReadBytes(&mut bytes)?;
        let path = directory.join(format!("page-{:04}.png", index + 1));
        std::fs::write(&path, bytes)?;
        page.Close()?;
        pages.push(path);
    }
    Ok(pages)
}
