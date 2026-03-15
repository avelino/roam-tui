# roam-sdk

A Rust SDK and terminal UI for [Roam Research](https://roamresearch.com).

The `roam-sdk` crate provides three things in one package:

- **SDK** — a Rust client for the Roam Research API. Use it to build your own tools, scripts, or integrations.
- **TUI** — a terminal-based interface for navigating and editing your Roam graph.
- **MCP Server** — expose your Roam graph to AI assistants via the Model Context Protocol.

## Quick links

| I want to... | Go to |
|---|---|
| Use the terminal app | [TUI Installation](tui/installation.md) |
| Configure the app | [Configuration](tui/configuration.md) |
| Learn keybindings | [Keybindings](tui/keybindings.md) |
| Export notes | [Export](export.md) |
| Select multiple blocks | [Multi-Block Selection](tui/multi-select.md) |
| Set up MCP for AI assistants | [MCP Setup](mcp/setup.md) |
| See MCP tools | [Tools Reference](mcp/tools.md) |
| Use the Rust SDK | [SDK Getting Started](sdk/getting-started.md) |
| See API reference | [Client](sdk/client.md), [Types](sdk/types.md), [Queries](sdk/queries.md) |

## Architecture

```
roam-sdk (crate)
├── lib.rs          → SDK: RoamClient, types, queries, errors
├── main.rs         → TUI: terminal interface using the SDK
├── mcp.rs          → MCP: stdio server with 18 tools
└── export.rs       → Export: markdown and JSON formatters
```

The TUI is built on [Ratatui](https://ratatui.rs) + [Tokio](https://tokio.rs) and uses the SDK internally. The MCP server uses [rmcp](https://crates.io/crates/rmcp) for the Model Context Protocol implementation. Both ship from the same crate — install the binary with `cargo install roam-sdk`, or add the library with `cargo add roam-sdk`.
