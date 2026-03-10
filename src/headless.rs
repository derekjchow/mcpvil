use std::io::Cursor;
use std::time::Duration;

use base64::Engine;
use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::pixman::{PixmanRenderer, PixmanTarget};
use smithay::{
    backend::renderer::{
        damage::OutputDamageTracker, element::surface::WaylandSurfaceRenderElement, Bind,
        ExportMem, Offscreen, Texture,
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::{
        timer::{TimeoutAction, Timer},
        EventLoop,
    },
    utils::{Buffer as BufferCoords, Rectangle, Size, Transform},
};

use crate::{CalloopData, Smallvil};

pub fn init_headless(
    event_loop: &mut EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn std::error::Error>> {
    let display_handle = &mut data.display_handle;
    let state = &mut data.state;

    let mut renderer = PixmanRenderer::new()?;

    let buffer_size: Size<i32, BufferCoords> = (1280, 720).into();
    let mut buffer = renderer.create_buffer(Fourcc::Argb8888, buffer_size)?;

    let output_size: Size<i32, smithay::utils::Physical> = (1280, 720).into();
    let mode = Mode {
        size: output_size,
        refresh: 60_000,
    };

    let output = Output::new(
        "headless".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".into(),
            model: "Headless".into(),
        },
    );
    let _global = output.create_global::<Smallvil>(display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Normal),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    state.space.map_output(&output, (0, 0));

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    let timer = Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, data| {
            let display = &mut data.display_handle;
            let state = &mut data.state;

            let _damage = Rectangle::from_size(output_size);

            {
                let mut target = renderer.bind(&mut buffer).unwrap();
                smithay::desktop::space::render_output::<
                    _,
                    WaylandSurfaceRenderElement<PixmanRenderer>,
                    _,
                    _,
                >(
                    &output,
                    &mut renderer,
                    &mut target,
                    1.0,
                    0,
                    [&state.space],
                    &[],
                    &mut damage_tracker,
                    [0.1, 0.1, 0.1, 1.0],
                )
                .unwrap();

                // Handle pending screenshot
                if let Some((filename, response_tx)) = state.pending_screenshot.take() {
                    let screenshot_result = take_screenshot(
                        &mut renderer,
                        &target,
                        output_size,
                        &state.space,
                        &filename,
                    );
                    let _ = response_tx.send(screenshot_result);
                }

                // Handle pending capture_screenshot
                if let Some(response_tx) = state.pending_capture_screenshot.take() {
                    let capture_result =
                        capture_screenshot(&mut renderer, &target, output_size, &state.space);
                    let _ = response_tx.send(capture_result);
                }
            }

            state.space.elements().for_each(|window| {
                window.send_frame(
                    &output,
                    state.start_time.elapsed(),
                    Some(Duration::ZERO),
                    |_, _| Some(output.clone()),
                )
            });

            state.space.refresh();
            state.popups.cleanup();
            let _ = display.flush_clients();

            TimeoutAction::ToDuration(Duration::from_millis(16))
        })?;

    Ok(())
}

fn take_screenshot(
    renderer: &mut PixmanRenderer,
    target: &PixmanTarget<'_>,
    size: smithay::utils::Size<i32, smithay::utils::Physical>,
    space: &smithay::desktop::Space<smithay::desktop::Window>,
    filename: &str,
) -> Result<String, String> {
    let region = Rectangle::from_size((size.w, size.h).into());

    let mapping = renderer
        .copy_framebuffer(target, region, Fourcc::Abgr8888)
        .map_err(|e| format!("Failed to copy framebuffer: {}", e))?;

    let pixels = renderer
        .map_texture(&mapping)
        .map_err(|e| format!("Failed to map texture: {}", e))?;

    let width = mapping.width();
    let height = mapping.height();

    // Pixman renders top-down (no vertical flip needed unlike OpenGL)
    let img = image::RgbaImage::from_raw(width, height, pixels.to_vec())
        .ok_or_else(|| "Failed to create image from pixel data".to_string())?;

    // Crop to the first window's bounds if one exists
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

fn capture_screenshot(
    renderer: &mut PixmanRenderer,
    target: &PixmanTarget<'_>,
    size: smithay::utils::Size<i32, smithay::utils::Physical>,
    space: &smithay::desktop::Space<smithay::desktop::Window>,
) -> Result<(String, u32, u32), String> {
    let region = Rectangle::from_size((size.w, size.h).into());

    let mapping = renderer
        .copy_framebuffer(target, region, Fourcc::Abgr8888)
        .map_err(|e| format!("Failed to copy framebuffer: {}", e))?;

    let pixels = renderer
        .map_texture(&mapping)
        .map_err(|e| format!("Failed to map texture: {}", e))?;

    let width = mapping.width();
    let height = mapping.height();

    // Pixman renders top-down (no vertical flip needed unlike OpenGL)
    let img = image::RgbaImage::from_raw(width, height, pixels.to_vec())
        .ok_or_else(|| "Failed to create image from pixel data".to_string())?;

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
