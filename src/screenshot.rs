use std::io::Cursor;

use base64::Engine;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::{ExportMem, Texture};
use smithay::utils::Rectangle;

pub fn save_screenshot_to_file<R>(
    renderer: &mut R,
    target: &R::Framebuffer<'_>,
    size: smithay::utils::Size<i32, smithay::utils::Physical>,
    space: &smithay::desktop::Space<smithay::desktop::Window>,
    filename: &str,
    needs_vflip: bool,
) -> Result<String, String>
where
    R: ExportMem,
    R::TextureMapping: Texture,
{
    let region = Rectangle::from_size((size.w, size.h).into());

    let mapping = renderer
        .copy_framebuffer(target, region, Fourcc::Abgr8888)
        .map_err(|e| format!("Failed to copy framebuffer: {}", e))?;

    let pixels = renderer
        .map_texture(&mapping)
        .map_err(|e| format!("Failed to map texture: {}", e))?;

    let width = mapping.width();
    let height = mapping.height();

    let mut img = image::RgbaImage::from_raw(width, height, pixels.to_vec())
        .ok_or_else(|| "Failed to create image from pixel data".to_string())?;

    if needs_vflip {
        image::imageops::flip_vertical_in_place(&mut img);
    }

    let img: image::DynamicImage = if let Some(window) = space.elements().next() {
        if let Some(geo) = space.element_geometry(window) {
            let x = geo.loc.x.max(0) as u32;
            let y = geo.loc.y.max(0) as u32;
            let w = (geo.size.w as u32).min(width.saturating_sub(x));
            let h = (geo.size.h as u32).min(height.saturating_sub(y));
            image::DynamicImage::ImageRgba8(img).crop_imm(x, y, w, h)
        } else {
            image::DynamicImage::ImageRgba8(img)
        }
    } else {
        image::DynamicImage::ImageRgba8(img)
    };

    img.save(filename)
        .map_err(|e| format!("Failed to save screenshot: {}", e))?;

    Ok(format!(
        "Screenshot saved to {} ({}x{})",
        filename,
        img.width(),
        img.height()
    ))
}

pub fn capture_screenshot<R>(
    renderer: &mut R,
    target: &R::Framebuffer<'_>,
    size: smithay::utils::Size<i32, smithay::utils::Physical>,
    space: &smithay::desktop::Space<smithay::desktop::Window>,
    needs_vflip: bool,
) -> Result<(String, u32, u32), String>
where
    R: ExportMem,
    R::TextureMapping: Texture,
{
    let region = Rectangle::from_size((size.w, size.h).into());

    let mapping = renderer
        .copy_framebuffer(target, region, Fourcc::Abgr8888)
        .map_err(|e| format!("Failed to copy framebuffer: {}", e))?;

    let pixels = renderer
        .map_texture(&mapping)
        .map_err(|e| format!("Failed to map texture: {}", e))?;

    let width = mapping.width();
    let height = mapping.height();

    let mut img = image::RgbaImage::from_raw(width, height, pixels.to_vec())
        .ok_or_else(|| "Failed to create image from pixel data".to_string())?;

    if needs_vflip {
        image::imageops::flip_vertical_in_place(&mut img);
    }

    let img: image::DynamicImage = if let Some(window) = space.elements().next() {
        if let Some(geo) = space.element_geometry(window) {
            let x = geo.loc.x.max(0) as u32;
            let y = geo.loc.y.max(0) as u32;
            let w = (geo.size.w as u32).min(width.saturating_sub(x));
            let h = (geo.size.h as u32).min(height.saturating_sub(y));
            image::DynamicImage::ImageRgba8(img).crop_imm(x, y, w, h)
        } else {
            image::DynamicImage::ImageRgba8(img)
        }
    } else {
        image::DynamicImage::ImageRgba8(img)
    };

    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Png)
        .map_err(|e| format!("Failed to encode PNG: {}", e))?;

    let base64_data = base64::engine::general_purpose::STANDARD.encode(buf.into_inner());

    Ok((base64_data, img.width(), img.height()))
}
