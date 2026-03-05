#![allow(irrefutable_let_patterns)]

mod handlers;

mod grabs;
mod input;
mod state;
mod winit;

use rmcp::{
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, Content, ServerCapabilities, ServerInfo},
    schemars,
    tool,
    tool_handler,
    tool_router,
    transport::stdio,
    ErrorData as McpError,
    ServerHandler,
    ServiceExt,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};


use smithay::reexports::{
    calloop::EventLoop,
    wayland_server::{Display, DisplayHandle},
};
pub use state::Smallvil;

pub struct CalloopData {
    state: Smallvil,
    display_handle: DisplayHandle,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct LaunchAppRequest {
    command: String,
    args: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ScreenshotRequest {
    /// File path to save the screenshot to (PNG format)
    filename: String,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CaptureScreenshotRequest {}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct CloseAppRequest {
    /// The process ID (PID) of the application to close, as returned by launch_app
    pid: u32,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct MouseMoveRequest {
    /// X coordinate in the compositor space
    x: f64,
    /// Y coordinate in the compositor space
    y: f64,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct MouseClickRequest {
    /// X coordinate to click at
    x: f64,
    /// Y coordinate to click at
    y: f64,
    /// Mouse button: "left", "right", or "middle" (default: "left")
    button: Option<String>,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct KeyPressRequest {
    /// Linux evdev keycode (e.g. 28 for Enter, 1 for Escape)
    key: u32,
}

#[derive(Serialize, Deserialize, JsonSchema)]
pub struct ScrollRequest {
    /// X coordinate to scroll at
    x: f64,
    /// Y coordinate to scroll at
    y: f64,
    /// Scroll direction: "up", "down", "left", or "right"
    direction: String,
    /// Scroll amount in pixels (default: 15.0)
    amount: Option<f64>,
}

pub enum McpCommand {
    LaunchApp {
        command: String,
        args: Vec<String>,
        response_tx: tokio::sync::oneshot::Sender<Result<u32, String>>,
    },
    Screenshot {
        filename: String,
        response_tx: tokio::sync::oneshot::Sender<Result<String, String>>,
    },
    CaptureScreenshot {
        response_tx: tokio::sync::oneshot::Sender<Result<(String, u32, u32), String>>,
    },
    CloseApp {
        pid: u32,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    MouseMove {
        x: f64,
        y: f64,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    MouseClick {
        x: f64,
        y: f64,
        button: u32,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    KeyPress {
        key: u32,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
    Scroll {
        x: f64,
        y: f64,
        axis: smithay::backend::input::Axis,
        amount: f64,
        response_tx: tokio::sync::oneshot::Sender<Result<(), String>>,
    },
}

impl std::fmt::Debug for McpCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpCommand::LaunchApp { command, args, .. } => f
                .debug_struct("LaunchApp")
                .field("command", command)
                .field("args", args)
                .finish(),
            McpCommand::Screenshot { filename, .. } => f
                .debug_struct("Screenshot")
                .field("filename", filename)
                .finish(),
            McpCommand::CaptureScreenshot { .. } => f
                .debug_struct("CaptureScreenshot")
                .finish(),
            McpCommand::CloseApp { pid, .. } => f
                .debug_struct("CloseApp")
                .field("pid", pid)
                .finish(),
            McpCommand::MouseMove { x, y, .. } => f
                .debug_struct("MouseMove")
                .field("x", x)
                .field("y", y)
                .finish(),
            McpCommand::MouseClick { x, y, button, .. } => f
                .debug_struct("MouseClick")
                .field("x", x)
                .field("y", y)
                .field("button", button)
                .finish(),
            McpCommand::KeyPress { key, .. } => f
                .debug_struct("KeyPress")
                .field("key", key)
                .finish(),
            McpCommand::Scroll { x, y, axis, amount, .. } => f
                .debug_struct("Scroll")
                .field("x", x)
                .field("y", y)
                .field("axis", axis)
                .field("amount", amount)
                .finish(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct MCPvilServer {
    tool_router: ToolRouter<Self>,
    command_tx: smithay::reexports::calloop::channel::Sender<McpCommand>,
}

#[tool_router]
impl MCPvilServer {
    fn new(command_tx: smithay::reexports::calloop::channel::Sender<McpCommand>) -> Self {
        Self {
            tool_router: Self::tool_router(),
            command_tx,
        }
    }

    #[tool(description = "Launches an application in the compositor")]
    async fn launch_app(
        &self,
        params: Parameters<LaunchAppRequest>,
    ) -> Result<CallToolResult, McpError> {
        let command = params.0.command.clone();
        let args = params.0.args.clone();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(McpCommand::LaunchApp {
                command: command.clone(),
                args: args.clone(),
                response_tx,
            })
            .map_err(|e| {
                McpError::internal_error(format!("Failed to send command: {}", e), None)
            })?;

        let result = response_rx.await.map_err(|_| {
            McpError::internal_error("Event loop dropped response channel".to_string(), None)
        })?;

        match result {
            Ok(pid) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Launched {} (pid {}) with args {:?}",
                command, pid, args
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to launch {}: {}",
                command, e
            ))])),
        }
    }

    #[tool(description = "Takes a screenshot of the compositor output and saves it as a PNG file")]
    async fn screenshot(
        &self,
        params: Parameters<ScreenshotRequest>,
    ) -> Result<CallToolResult, McpError> {
        let filename = params.0.filename.clone();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(McpCommand::Screenshot {
                filename: filename.clone(),
                response_tx,
            })
            .map_err(|e| {
                McpError::internal_error(format!("Failed to send command: {}", e), None)
            })?;

        let result = response_rx.await.map_err(|_| {
            McpError::internal_error("Event loop dropped response channel".to_string(), None)
        })?;

        match result {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to take screenshot: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Closes/kills an application by its PID (as returned by launch_app)")]
    async fn close_app(
        &self,
        params: Parameters<CloseAppRequest>,
    ) -> Result<CallToolResult, McpError> {
        let pid = params.0.pid;
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(McpCommand::CloseApp { pid, response_tx })
            .map_err(|e| {
                McpError::internal_error(format!("Failed to send command: {}", e), None)
            })?;

        let result = response_rx.await.map_err(|_| {
            McpError::internal_error("Event loop dropped response channel".to_string(), None)
        })?;

        match result {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Sent SIGTERM to process {}",
                pid
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to close process {}: {}",
                pid, e
            ))])),
        }
    }

    #[tool(description = "Moves the mouse pointer to the specified coordinates")]
    async fn mouse_move(
        &self,
        params: Parameters<MouseMoveRequest>,
    ) -> Result<CallToolResult, McpError> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(McpCommand::MouseMove {
                x: params.0.x,
                y: params.0.y,
                response_tx,
            })
            .map_err(|e| {
                McpError::internal_error(format!("Failed to send command: {}", e), None)
            })?;

        let result = response_rx.await.map_err(|_| {
            McpError::internal_error("Event loop dropped response channel".to_string(), None)
        })?;

        match result {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Mouse moved to ({}, {})",
                params.0.x, params.0.y
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to move mouse: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Clicks a mouse button at the specified coordinates")]
    async fn mouse_click(
        &self,
        params: Parameters<MouseClickRequest>,
    ) -> Result<CallToolResult, McpError> {
        let button_name = params.0.button.as_deref().unwrap_or("left");
        let button_code: u32 = match button_name {
            "left" => 0x110,   // BTN_LEFT
            "right" => 0x111,  // BTN_RIGHT
            "middle" => 0x112, // BTN_MIDDLE
            other => {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Unknown button '{}'. Use 'left', 'right', or 'middle'.",
                    other
                ))]));
            }
        };

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(McpCommand::MouseClick {
                x: params.0.x,
                y: params.0.y,
                button: button_code,
                response_tx,
            })
            .map_err(|e| {
                McpError::internal_error(format!("Failed to send command: {}", e), None)
            })?;

        let result = response_rx.await.map_err(|_| {
            McpError::internal_error("Event loop dropped response channel".to_string(), None)
        })?;

        match result {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Clicked {} at ({}, {})",
                button_name, params.0.x, params.0.y
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to click: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Simulates a key press and release using a Linux evdev keycode")]
    async fn key_press(
        &self,
        params: Parameters<KeyPressRequest>,
    ) -> Result<CallToolResult, McpError> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(McpCommand::KeyPress {
                key: params.0.key,
                response_tx,
            })
            .map_err(|e| {
                McpError::internal_error(format!("Failed to send command: {}", e), None)
            })?;

        let result = response_rx.await.map_err(|_| {
            McpError::internal_error("Event loop dropped response channel".to_string(), None)
        })?;

        match result {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Key {} pressed and released",
                params.0.key
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to press key: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Scrolls at the specified coordinates in the given direction")]
    async fn scroll(
        &self,
        params: Parameters<ScrollRequest>,
    ) -> Result<CallToolResult, McpError> {
        use smithay::backend::input::Axis;

        let (axis, amount) = match params.0.direction.as_str() {
            "up" => (Axis::Vertical, -(params.0.amount.unwrap_or(15.0))),
            "down" => (Axis::Vertical, params.0.amount.unwrap_or(15.0)),
            "left" => (Axis::Horizontal, -(params.0.amount.unwrap_or(15.0))),
            "right" => (Axis::Horizontal, params.0.amount.unwrap_or(15.0)),
            other => {
                return Ok(CallToolResult::success(vec![Content::text(format!(
                    "Unknown direction '{}'. Use 'up', 'down', 'left', or 'right'.",
                    other
                ))]));
            }
        };

        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(McpCommand::Scroll {
                x: params.0.x,
                y: params.0.y,
                axis,
                amount,
                response_tx,
            })
            .map_err(|e| {
                McpError::internal_error(format!("Failed to send command: {}", e), None)
            })?;

        let result = response_rx.await.map_err(|_| {
            McpError::internal_error("Event loop dropped response channel".to_string(), None)
        })?;

        match result {
            Ok(()) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Scrolled {} at ({}, {})",
                params.0.direction, params.0.x, params.0.y
            ))])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to scroll: {}",
                e
            ))])),
        }
    }

    #[tool(description = "Captures a screenshot of the compositor output and returns it as a base64-encoded PNG image")]
    async fn capture_screenshot(
        &self,
        #[allow(unused_variables)] params: Parameters<CaptureScreenshotRequest>,
    ) -> Result<CallToolResult, McpError> {
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx
            .send(McpCommand::CaptureScreenshot { response_tx })
            .map_err(|e| {
                McpError::internal_error(format!("Failed to send command: {}", e), None)
            })?;

        let result = response_rx.await.map_err(|_| {
            McpError::internal_error("Event loop dropped response channel".to_string(), None)
        })?;

        match result {
            Ok((base64_data, width, height)) => Ok(CallToolResult::success(vec![
                Content::image(base64_data, "image/png"),
                Content::text(format!("Screenshot captured ({}x{})", width, height)),
            ])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(format!(
                "Failed to capture screenshot: {}",
                e
            ))])),
        }
    }
}

#[tool_handler]
impl ServerHandler for MCPvilServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            instructions: Some("A Wayland Compositor to launch and control applications in".into()),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            ..Default::default()
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if let Ok(env_filter) = tracing_subscriber::EnvFilter::try_from_default_env() {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(env_filter)
            .init();
    } else {
        tracing_subscriber::fmt()
            .with_writer(std::io::stderr)
            .init();
    }

    let mut event_loop: EventLoop<CalloopData> = EventLoop::try_new()?;

    let display: Display<Smallvil> = Display::new()?;
    let display_handle = display.handle();
    let state = Smallvil::new(&mut event_loop, display);

    let mut data = CalloopData {
        state,
        display_handle,
    };

    crate::winit::init_winit(&mut event_loop, &mut data)?;

    let mut args = std::env::args().skip(1);
    match args.next().as_deref() {
        Some("-c") | Some("--command") => {
            if let Some(command) = args.next() {
                std::process::Command::new(command).args(args).spawn().ok();
            }
        }
        _ => {}
    }

    // let transport = (tokio::io::stdin(), tokio::io::stdout());
    let (command_tx, command_rx) = smithay::reexports::calloop::channel::channel::<McpCommand>();

    event_loop
        .handle()
        .insert_source(command_rx, |event, _, _data| match event {
            smithay::reexports::calloop::channel::Event::Msg(msg) => match msg {
                McpCommand::LaunchApp {
                    command,
                    args,
                    response_tx,
                } => {
                    let mut cmd = std::process::Command::new(&command);
                    cmd.args(&args);
                    let result = match cmd.spawn() {
                        Ok(child) => Ok(child.id()),
                        Err(e) => {
                            tracing::error!("Failed to launch app: {}", e);
                            Err(e.to_string())
                        }
                    };
                    let _ = response_tx.send(result);
                }
                McpCommand::Screenshot {
                    filename,
                    response_tx,
                } => {
                    _data.state.pending_screenshot = Some((filename, response_tx));
                }
                McpCommand::CaptureScreenshot { response_tx } => {
                    _data.state.pending_capture_screenshot = Some(response_tx);
                }
                McpCommand::CloseApp { pid, response_tx } => {
                    let result = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
                    if result == 0 {
                        let _ = response_tx.send(Ok(()));
                    } else {
                        let err = std::io::Error::last_os_error();
                        tracing::error!("Failed to kill process {}: {}", pid, err);
                        let _ = response_tx.send(Err(err.to_string()));
                    }
                }
                McpCommand::MouseMove { x, y, response_tx } => {
                    use smithay::input::pointer::MotionEvent;
                    use smithay::utils::SERIAL_COUNTER;

                    let pos = (x, y).into();
                    let serial = SERIAL_COUNTER.next_serial();
                    let pointer = _data.state.seat.get_pointer().unwrap();
                    let under = _data.state.surface_under(pos);
                    let time = _data.state.start_time.elapsed().as_millis() as u32;

                    pointer.motion(
                        &mut _data.state,
                        under,
                        &MotionEvent {
                            location: pos,
                            serial,
                            time,
                        },
                    );
                    pointer.frame(&mut _data.state);
                    let _ = response_tx.send(Ok(()));
                }
                McpCommand::MouseClick { x, y, button, response_tx } => {
                    use smithay::backend::input::ButtonState;
                    use smithay::input::pointer::{ButtonEvent, MotionEvent};
                    use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
                    use smithay::utils::SERIAL_COUNTER;

                    let pos = (x, y).into();
                    let time = _data.state.start_time.elapsed().as_millis() as u32;

                    // Move pointer to position first
                    let serial = SERIAL_COUNTER.next_serial();
                    let pointer = _data.state.seat.get_pointer().unwrap();
                    let keyboard = _data.state.seat.get_keyboard().unwrap();
                    let under = _data.state.surface_under(pos);

                    pointer.motion(
                        &mut _data.state,
                        under,
                        &MotionEvent {
                            location: pos,
                            serial,
                            time,
                        },
                    );
                    pointer.frame(&mut _data.state);

                    // Focus/raise logic (same as input.rs)
                    let serial = SERIAL_COUNTER.next_serial();
                    if !pointer.is_grabbed() {
                        if let Some((window, _loc)) = _data
                            .state
                            .space
                            .element_under(pointer.current_location())
                            .map(|(w, l)| (w.clone(), l))
                        {
                            _data.state.space.raise_element(&window, true);
                            keyboard.set_focus(
                                &mut _data.state,
                                Some(window.toplevel().unwrap().wl_surface().clone()),
                                serial,
                            );
                            _data.state.space.elements().for_each(|window| {
                                window.toplevel().unwrap().send_pending_configure();
                            });
                        } else {
                            _data.state.space.elements().for_each(|window| {
                                window.set_activated(false);
                                window.toplevel().unwrap().send_pending_configure();
                            });
                            keyboard.set_focus(
                                &mut _data.state,
                                Option::<WlSurface>::None,
                                serial,
                            );
                        }
                    }

                    // Press
                    pointer.button(
                        &mut _data.state,
                        &ButtonEvent {
                            button,
                            state: ButtonState::Pressed,
                            serial,
                            time,
                        },
                    );
                    pointer.frame(&mut _data.state);

                    // Release
                    let serial = SERIAL_COUNTER.next_serial();
                    pointer.button(
                        &mut _data.state,
                        &ButtonEvent {
                            button,
                            state: ButtonState::Released,
                            serial,
                            time,
                        },
                    );
                    pointer.frame(&mut _data.state);
                    let _ = response_tx.send(Ok(()));
                }
                McpCommand::KeyPress { key, response_tx } => {
                    use smithay::backend::input::KeyState;
                    use smithay::input::keyboard::FilterResult;
                    use smithay::utils::SERIAL_COUNTER;

                    let serial = SERIAL_COUNTER.next_serial();
                    let time = _data.state.start_time.elapsed().as_millis() as u32;
                    let keyboard = _data.state.seat.get_keyboard().unwrap();

                    keyboard.input::<(), _>(
                        &mut _data.state,
                        key.into(),
                        KeyState::Pressed,
                        serial,
                        time,
                        |_, _, _| FilterResult::Forward,
                    );

                    let serial = SERIAL_COUNTER.next_serial();
                    keyboard.input::<(), _>(
                        &mut _data.state,
                        key.into(),
                        KeyState::Released,
                        serial,
                        time,
                        |_, _, _| FilterResult::Forward,
                    );
                    let _ = response_tx.send(Ok(()));
                }
                McpCommand::Scroll { x, y, axis, amount, response_tx } => {
                    use smithay::backend::input::AxisSource;
                    use smithay::input::pointer::{AxisFrame, MotionEvent};
                    use smithay::utils::SERIAL_COUNTER;

                    let pos = (x, y).into();
                    let time = _data.state.start_time.elapsed().as_millis() as u32;

                    // Move pointer to position first
                    let serial = SERIAL_COUNTER.next_serial();
                    let pointer = _data.state.seat.get_pointer().unwrap();
                    let under = _data.state.surface_under(pos);

                    pointer.motion(
                        &mut _data.state,
                        under,
                        &MotionEvent {
                            location: pos,
                            serial,
                            time,
                        },
                    );
                    pointer.frame(&mut _data.state);

                    // Scroll
                    let frame = AxisFrame::new(time)
                        .source(AxisSource::Wheel)
                        .value(axis, amount);
                    pointer.axis(&mut _data.state, frame);
                    pointer.frame(&mut _data.state);
                    let _ = response_tx.send(Ok(()));
                }
            },
            smithay::reexports::calloop::channel::Event::Closed => {
                tracing::info!("MCP command channel closed");
            }
        })
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)?;

    let service = MCPvilServer::new(command_tx);
    let server = MCPvilServer::serve(service, stdio());

    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?;

    let _guard = rt.enter();
    rt.spawn(async move {
        match server.await {
            Ok(running_service) => {
                if let Err(e) = running_service.waiting().await {
                    tracing::error!("MCP server task error: {:?}", e);
                }
            }
            Err(e) => {
                tracing::error!("MCP server initialization error: {:?}", e);
            }
        }
    });

    event_loop.run(None, &mut data, move |_| {
        // Smallvil is running
    })?;

    Ok(())
}
