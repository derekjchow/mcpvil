use std::time::Duration;

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::pixman::PixmanRenderer;
use smithay::backend::renderer::{
    damage::OutputDamageTracker, Bind, Color32F, ExportMem, Frame, ImportMem, Offscreen, Renderer,
};
use smithay::backend::winit::{self, WinitEvent};
use smithay::output::{Mode, Output, PhysicalProperties, Subpixel};
use smithay::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay::reexports::calloop::EventLoop;
use smithay::utils::{Buffer as BufferCoords, Rectangle, Size, Transform};

use crate::{CalloopData, Smallvil};

pub struct WinitState {
    pub backend: smithay::backend::winit::WinitGraphicsBackend<GlesRenderer>,
    pub damage_tracker: OutputDamageTracker,
}

pub fn init(
    event_loop: &mut EventLoop<CalloopData>,
    data: &mut CalloopData,
    gui_mode: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let display_handle = &mut data.display_handle;
    let state = &mut data.state;

    // Always create the pixman renderer for compositing and screenshots
    let mut renderer = PixmanRenderer::new()?;
    let buffer_size: Size<i32, BufferCoords> = (1280, 720).into();
    let mut buffer = renderer.create_buffer(Fourcc::Argb8888, buffer_size)?;

    let output_size: Size<i32, smithay::utils::Physical> = (1280, 720).into();
    let mode = Mode {
        size: output_size,
        refresh: 60_000,
    };

    let output = Output::new(
        "mcpvil".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".into(),
            model: "MCPvil".into(),
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

    // Optionally open the GUI window
    if gui_mode {
        open_window(data)?;
    }

    // Timer drives the render loop (always headless pixman, optionally blits to window)
    let timer = Timer::immediate();
    event_loop
        .handle()
        .insert_source(timer, move |_, _, data| {
            let display = &mut data.display_handle;
            let state = &mut data.state;

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

                // Handle pending save_screenshot_to_file (pixman is always top-down, no vflip needed)
                if let Some((filename, response_tx)) = state.pending_save_screenshot_to_file.take() {
                    let result = crate::screenshot::save_screenshot_to_file(
                        &mut renderer,
                        &target,
                        output_size,
                        &state.space,
                        &filename,
                        false,
                    );
                    let _ = response_tx.send(result);
                }

                if let Some(response_tx) = state.pending_capture_screenshot.take() {
                    let result = crate::screenshot::capture_screenshot(
                        &mut renderer,
                        &target,
                        output_size,
                        &state.space,
                        false,
                    );
                    let _ = response_tx.send(result);
                }

                // Blit to GUI window if open
                if let Some(ref mut winit_state) = state.winit_state {
                    blit_to_window(&mut renderer, &target, output_size, winit_state);
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

pub fn open_window(data: &mut CalloopData) -> Result<(), Box<dyn std::error::Error>> {
    let handle = data.state.loop_handle.clone();

    // Temporarily unset WAYLAND_DISPLAY so winit connects to the
    // host display server instead of our own compositor socket.
    let saved_wayland = std::env::var("WAYLAND_DISPLAY").ok();
    std::env::remove_var("WAYLAND_DISPLAY");

    let (backend, winit_event_source) = winit::init::<GlesRenderer>()?;

    // Restore WAYLAND_DISPLAY for child processes
    if let Some(val) = saved_wayland {
        std::env::set_var("WAYLAND_DISPLAY", val);
    }

    // Create a separate damage tracker for the winit window output
    // We use a dummy output just to initialize the tracker with the right size
    let winit_output = Output::new(
        "winit-display".to_string(),
        PhysicalProperties {
            size: (0, 0).into(),
            subpixel: Subpixel::Unknown,
            make: "Smithay".into(),
            model: "Winit".into(),
        },
    );
    let window_size = backend.window_size();
    winit_output.change_current_state(
        Some(Mode {
            size: window_size,
            refresh: 60_000,
        }),
        Some(Transform::Normal),
        None,
        Some((0, 0).into()),
    );
    let damage_tracker = OutputDamageTracker::from_output(&winit_output);

    data.state.winit_state = Some(WinitState {
        backend,
        damage_tracker,
    });

    handle.insert_source(winit_event_source, move |event, _, data| {
        let state = &mut data.state;
        match event {
            WinitEvent::Input(event) => state.process_input_event(event),
            WinitEvent::CloseRequested => {
                state.winit_state = None;
            }
            _ => {} // Timer handles rendering; ignore Redraw/Resized
        }
    })?;

    Ok(())
}

fn blit_to_window(
    pixman: &mut PixmanRenderer,
    pixman_target: &<PixmanRenderer as smithay::backend::renderer::RendererSuper>::Framebuffer<'_>,
    output_size: Size<i32, smithay::utils::Physical>,
    winit_state: &mut WinitState,
) {
    let region = Rectangle::from_size((output_size.w, output_size.h).into());

    let Ok(mapping) = pixman.copy_framebuffer(pixman_target, region, Fourcc::Abgr8888) else {
        return;
    };
    let Ok(pixels) = pixman.map_texture(&mapping) else {
        return;
    };

    let buf_size: Size<i32, BufferCoords> = (output_size.w, output_size.h).into();
    let window_size = winit_state.backend.window_size();

    {
        let Ok((gles, mut fb)) = winit_state.backend.bind() else {
            return;
        };

        let Ok(tex) = gles.import_memory(pixels, Fourcc::Abgr8888, buf_size, false) else {
            return;
        };

        let Ok(mut frame) = gles.render(&mut fb, window_size, Transform::Normal) else {
            return;
        };

        let _ = frame.clear(
            Color32F::BLACK,
            &[Rectangle::from_size(window_size)],
        );

        let src = Rectangle::from_size((buf_size.w as f64, buf_size.h as f64).into());
        let dst = Rectangle::from_size(window_size);
        let _ = frame.render_texture_from_to(
            &tex,
            src,
            dst,
            &[dst],
            &[],
            Transform::Normal,
            1.0,
            None,
            &[],
        );

        let _ = frame.finish();
    }

    let _ = winit_state.backend.submit(None);
}
