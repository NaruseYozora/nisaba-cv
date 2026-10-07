use crate::{Error, Result, error::require};
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use std::io::Cursor;

/// Decode before storing; flatten orientation and remove source metadata.
pub fn normalize(data: &[u8]) -> Result<Vec<u8>> {
    require(
        !data.is_empty() && data.len() <= 32 * 1024 * 1024,
        "照片必须在 32MB 以内",
    )?;
    let invalid = |e: image::ImageError| Error::Invalid(format!("照片无法解码：{e}"));
    let mut reader = ImageReader::new(Cursor::new(data)).with_guessed_format()?;
    require(
        matches!(reader.format(), Some(ImageFormat::Png | ImageFormat::Jpeg)),
        "仅支持 PNG 或 JPEG 照片",
    )?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(10_000);
    limits.max_image_height = Some(10_000);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(invalid)?;
    let (width, height) = decoder.dimensions();
    require(
        width > 0 && height > 0 && u64::from(width) * u64::from(height) <= 24_000_000,
        "照片不能超过 2400 万像素",
    )?;
    let orientation = decoder.orientation().map_err(invalid)?;
    let mut photo = DynamicImage::from_decoder(decoder).map_err(invalid)?;
    photo.apply_orientation(orientation);
    if photo.width() > 1600 || photo.height() > 1600 {
        photo = photo.resize(1600, 1600, image::imageops::FilterType::Lanczos3);
    }
    let mut output = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(photo.to_rgba8())
        .write_to(&mut output, ImageFormat::Png)
        .map_err(invalid)?;
    let output = output.into_inner();
    require(output.len() <= 32 * 1024 * 1024, "处理后的照片过大")?;
    Ok(output)
}
