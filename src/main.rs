#![allow(irrefutable_let_patterns)]

mod handlers;

mod grabs;
mod input;
mod state;
mod winit;

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    // model::*,
    schemars, tool, tool_handler, tool_router,
    // tool, tool_handler, tool_router,
    model::{Content, CallToolResult, ServerInfo, ServerCapabilities},
    ErrorData as McpError,
    transport::stdio,
};
// use tokio::io::{stdin, stdout};
use schemars::JsonSchema;
use serde::{Serialize, Deserialize};
use tracing;

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
}

impl std::fmt::Debug for McpCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            McpCommand::LaunchApp { command, args, .. } => {
                f.debug_struct("LaunchApp")
                    .field("command", command)
                    .field("args", args)
                    .finish()
            }
            McpCommand::Screenshot { filename, .. } => {
                f.debug_struct("Screenshot")
                    .field("filename", filename)
                    .finish()
            }
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
        params: Parameters<LaunchAppRequest>
    ) -> Result<CallToolResult, McpError> {
        let command = params.0.command.clone();
        let args = params.0.args.clone();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx.send(McpCommand::LaunchApp {
            command: command.clone(),
            args: args.clone(),
            response_tx,
        }).map_err(|e| McpError::internal_error(format!("Failed to send command: {}", e), None))?;

        let result = response_rx.await
            .map_err(|_| McpError::internal_error("Event loop dropped response channel".to_string(), None))?;

        match result {
            Ok(pid) => Ok(CallToolResult::success(vec![Content::text(
                format!("Launched {} (pid {}) with args {:?}", command, pid, args),
            )])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(
                format!("Failed to launch {}: {}", command, e),
            )])),
        }
    }

    #[tool(description = "Takes a screenshot of the compositor output and saves it as a PNG file")]
    async fn screenshot(
        &self,
        params: Parameters<ScreenshotRequest>
    ) -> Result<CallToolResult, McpError> {
        let filename = params.0.filename.clone();
        let (response_tx, response_rx) = tokio::sync::oneshot::channel();

        self.command_tx.send(McpCommand::Screenshot {
            filename: filename.clone(),
            response_tx,
        }).map_err(|e| McpError::internal_error(format!("Failed to send command: {}", e), None))?;

        let result = response_rx.await
            .map_err(|_| McpError::internal_error("Event loop dropped response channel".to_string(), None))?;

        match result {
            Ok(msg) => Ok(CallToolResult::success(vec![Content::text(msg)])),
            Err(e) => Ok(CallToolResult::success(vec![Content::text(
                format!("Failed to take screenshot: {}", e),
            )])),
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
        tracing_subscriber::fmt().with_writer(std::io::stderr).init();
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
                McpCommand::LaunchApp { command, args, response_tx } => {
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
                McpCommand::Screenshot { filename, response_tx } => {
                    _data.state.pending_screenshot = Some((filename, response_tx));
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
