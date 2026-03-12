# MCPvil

A fork of [smallvil](https://github.com/Smithay/smithay/tree/master/smallvil) from the [Smithay](https://github.com/Smithay/smithay) project with an [MCP (Model Context Protocol)](https://modelcontextprotocol.io/) server bolted onto it.

MCPvil is a minimal Wayland compositor that exposes its functionality through MCP, allowing AI agents and other MCP clients to interact with the compositor over stdio.

## MCP Tools

| Tool | Description |
|------|-------------|
| `launch_app` | Launch an application inside the compositor |
| `close_app` | Kill an application by PID |
| `save_screenshot_to_file` | Save a screenshot as a PNG file |
| `capture_screenshot` | Capture a screenshot as base64-encoded PNG |
| `mouse_move` | Move the mouse pointer to coordinates |
| `mouse_click` | Click a mouse button at coordinates |
| `key_press` | Simulate a key press via evdev keycode |
| `scroll` | Scroll at coordinates in a given direction |
| `open_window` | Open a GUI window for visual inspection |

## Installation

### System Dependencies (Debian/Ubuntu)

```bash
sudo apt-get install build-essential pkg-config cmake \
  libwayland-dev wayland-protocols libinput-dev libudev-dev \
  libseat-dev libxkbcommon-dev libegl-dev libgles2-mesa-dev \
  libgbm-dev libdrm-dev libxcb1-dev libx11-xcb-dev \
  libdbus-1-dev libsystemd-dev libpixman-1-dev
```

### Install via Cargo

```bash
cargo install --git https://github.com/derekjchow/mcpvil
```

### Build from Source

```bash
git clone https://github.com/derekjchow/mcpvil.git
cd mcpvil
cargo build
```

## Usage

MCPvil communicates over stdio using the MCP protocol (newline-delimited JSON-RPC). It can be used with any MCP-compatible client.

```bash
# Run directly (MCP server on stdio, compositor logs on stderr)
mcpvil

# With debug logging
RUST_LOG=debug mcpvil

# GUI mode (opens a window for visual inspection)
mcpvil --gui
```

## Setup with MCP Clients

### Claude Code

```bash
claude mcp add mcpvil -- mcpvil
```

### Gemini CLI

Add to `~/.gemini/settings.json`:

```json
{
  "mcpServers": {
    "mcpvil": {
      "command": "mcpvil"
    }
  }
}
```

## Dependencies

- [Smithay](https://github.com/Smithay/smithay) — Wayland compositor library
- [rmcp](https://crates.io/crates/rmcp) — Rust MCP server library
- [image](https://crates.io/crates/image) — Screenshot encoding
