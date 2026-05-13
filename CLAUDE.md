# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is MCPvil?

MCPvil is a minimal headless Wayland compositor (forked from Smithay's smallvil) with an MCP (Model Context Protocol) server. It lets AI agents launch, control, and screenshot GUI applications via MCP tools over stdio.

## Build & Lint

```bash
cargo build              # Build
cargo fmt -- --check     # Check formatting
cargo clippy -- -D warnings  # Lint (warnings are errors)
cargo fmt                # Auto-fix formatting
```

There are no tests. The pre-commit hook (`.githooks/pre-commit`, enabled via `core.hooksPath=.githooks`) runs `cargo fmt --check` and `cargo clippy -- -D warnings`.

## Architecture

The project is a single Rust binary with two concurrent systems:

1. **Smithay compositor** — runs on the main thread via a calloop `EventLoop`. Manages Wayland clients, rendering (pixman software renderer), and input.
2. **MCP server** — runs on a tokio runtime in a background thread. Receives JSON-RPC requests over stdio using the `rmcp` crate.

### Communication between MCP and compositor

MCP tool handlers in `MCPvilServer` send `McpCommand` variants through a calloop channel (`command_tx`). The event loop callback in `main()` receives these commands and executes them in the compositor context. Responses flow back via `tokio::sync::oneshot` channels embedded in each `McpCommand`.

### Key files

- **`src/main.rs`** — MCP tool request/response structs, `McpCommand` enum, `MCPvilServer` tool implementations (using `#[tool]` / `#[tool_router]` macros from rmcp), event loop command dispatch, and the `main()` entry point.
- **`src/state.rs`** — `Smallvil` struct holding all compositor state (space, seat, outputs, pending screenshot requests). Wayland socket listener setup.
- **`src/backend.rs`** — Pixman renderer initialization, render loop timer, optional winit GUI window (`open_window`), and pixman-to-winit blit.
- **`src/input.rs`** — `process_input_event` for handling real input from winit backend.
- **`src/screenshot.rs`** — `save_screenshot_to_file` and `capture_screenshot` — reads pixman framebuffer, optionally crops to window geometry, encodes to PNG.
- **`src/handlers/`** — Smithay protocol handler implementations (compositor, xdg_shell).
- **`src/grabs/`** — Move and resize grab logic for window management.

### Rendering

The compositor always uses a **PixmanRenderer** (software, headless). When `--gui` is passed or `open_window` MCP tool is called, a winit window opens and the pixman framebuffer is blitted to it via GlesRenderer. Screenshots are always taken from the pixman framebuffer. The default resolution is 1280x720.

### Adding a new MCP tool

1. Add a request struct (with `Serialize`, `Deserialize`, `JsonSchema` derives) in `main.rs`
2. Add a variant to the `McpCommand` enum with a `response_tx` oneshot sender
3. Add a `#[tool]` method to `MCPvilServer`
4. Handle the command in the `insert_source` callback in `main()`
