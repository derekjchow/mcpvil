# MCPvil

A fork of [smallvil](https://github.com/Smithay/smithay/tree/master/smallvil) from the [Smithay](https://github.com/Smithay/smithay) project with an [MCP (Model Context Protocol)](https://modelcontextprotocol.io/) server bolted onto it.

MCPvil is a minimal Wayland compositor that exposes its functionality through MCP, allowing AI agents and other MCP clients to interact with the compositor over stdio.

## MCP Tools

| Tool | Description |
|------|-------------|
| `launch_app` | Launches an application inside the compositor |
| `screenshot` | Captures a screenshot of the active window and saves it as a PNG |

## Building

```bash
cargo build
```

## Usage

MCPvil communicates over stdio using the MCP protocol (newline-delimited JSON-RPC). It can be used with any MCP-compatible client.

```bash
# Run directly (MCP server on stdio, compositor logs on stderr)
./target/debug/mcpvil

# With debug logging
RUST_LOG=debug ./target/debug/mcpvil
```

## Dependencies

- [Smithay](https://github.com/Smithay/smithay) — Wayland compositor library
- [rmcp](https://crates.io/crates/rmcp) — Rust MCP server library
- [image](https://crates.io/crates/image) — Screenshot encoding
