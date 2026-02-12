use std::time::Duration;

use smithay::backend::allocator::Fourcc;
use smithay::{
    backend::{
        renderer::{
            damage::OutputDamageTracker,
            element::surface::WaylandSurfaceRenderElement,
            gles::{GlesRenderer, GlesTarget},
            ExportMem, Texture,
        },
        winit::{self, WinitEvent},
    },
    output::{Mode, Output, PhysicalProperties, Subpixel},
    reexports::calloop::EventLoop,
    utils::{Rectangle, Transform},
};

use crate::{CalloopData, Smallvil};

pub fn init_winit(
    event_loop: &mut EventLoop<CalloopData>,
    data: &mut CalloopData,
) -> Result<(), Box<dyn std::error::Error>> {
    let display_handle = &mut data.display_handle;
    let state = &mut data.state;

    let (mut backend, winit) = winit::init()?;

    let mode = Mode {
        size: backend.window_size(),
        refresh: 60_000,
    };

    let output = Output::new(
        "winit".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".into(),
            model: "Winit".into(),
        },
    );
    let _global = output.create_global::<Smallvil>(display_handle);
    output.change_current_state(
        Some(mode),
        Some(Transform::Flipped180),
        None,
        Some((0, 0).into()),
    );
    output.set_preferred(mode);

    state.space.map_output(&output, (0, 0));

    let mut damage_tracker = OutputDamageTracker::from_output(&output);

    std::env::set_var("WAYLAND_DISPLAY", &state.socket_name);

    event_loop
        .handle()
        .insert_source(winit, move |event, _, data| {
            let display = &mut data.display_handle;
            let state = &mut data.state;

            match event {
                WinitEvent::Resized { size, .. } => {
                    output.change_current_state(
                        Some(Mode {
                            size,
                            refresh: 60_000,
                        }),
                        None,
                        None,
                        None,
                    );
                }
                WinitEvent::Input(event) => state.process_input_event(event),
                WinitEvent::Redraw => {
                    let size = backend.window_size();
                    let damage = Rectangle::from_size(size);

                    {
                        let (renderer, mut framebuffer) = backend.bind().unwrap();
                        smithay::desktop::space::render_output::<
                            _,
                            WaylandSurfaceRenderElement<GlesRenderer>,
                            _,
                            _,
                        >(
                            &output,
                            renderer,
                            &mut framebuffer,
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
                                renderer,
                                &framebuffer,
                                size,
                                &state.space,
                                &filename,
                            );
                            let _ = response_tx.send(screenshot_result);
                        }
                    }
                    backend.submit(Some(&[damage])).unwrap();

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

                    // Ask for redraw to schedule new frame.
                    backend.window().request_redraw();
                }
                WinitEvent::CloseRequested => {
                    state.loop_signal.stop();
                }
                _ => (),
            };
        })?;

    Ok(())
}

fn take_screenshot(
    renderer: &mut GlesRenderer,
    framebuffer: &GlesTarget<'_>,
    size: smithay::utils::Size<i32, smithay::utils::Physical>,
    space: &smithay::desktop::Space<smithay::desktop::Window>,
    filename: &str,
) -> Result<String, String> {
    let region = Rectangle::from_size((size.w, size.h).into());

    let mapping = renderer
        .copy_framebuffer(framebuffer, region, Fourcc::Abgr8888)
        .map_err(|e| format!("Failed to copy framebuffer: {}", e))?;

    let pixels = renderer
        .map_texture(&mapping)
        .map_err(|e| format!("Failed to map texture: {}", e))?;

    let width = mapping.width();
    let height = mapping.height();

    // Create image from raw pixels and flip vertically
    // (OpenGL framebuffer origin is bottom-left)
    let mut img = image::RgbaImage::from_raw(width, height, pixels.to_vec())
        .ok_or_else(|| "Failed to create image from pixel data".to_string())?;
    image::imageops::flip_vertical_in_place(&mut img);

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
