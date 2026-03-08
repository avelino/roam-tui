# roam-tui

A fast, keyboard-driven terminal client and MCP server for [Roam Research](https://roamresearch.com). Connects directly to the [Roam Research cloud API](https://roamresearch.com/#/app/developer-documentation) — no local Roam desktop app needed.

## MCP Server Setup

Add to your MCP client config (e.g. `claude_desktop_config.json`):

```json
{
  "mcpServers": {
    "roam": {
      "command": "npx",
      "args": ["-y", "roam-tui@latest", "--mcp"],
      "env": {
        "ROAM_GRAPH_NAME": "your-graph-name",
        "ROAM_GRAPH_API__TOKEN": "roam-graph-token-XXXXX"
      }
    }
  }
}
```

## Available Tools

| Tool | Description |
|------|-------------|
| `search` | Search pages by title |
| `get_page` | Get a page with all blocks |
| `get_block` | Get a block by UID |
| `get_backlinks` | Find blocks that reference a page |
| `roam_query` | Run a raw Datalog query |
| `create_page` | Create a new page |
| `create_block` | Create a block under a parent |
| `update_block` | Update block content |
| `delete_block` | Delete a block |
| `delete_page` | Delete a page |
| `move_block` | Move a block to another parent |

## TUI Client

This package also includes a full terminal UI client:

```bash
npx roam-tui@latest
```

Vim/emacs/vscode keybinding presets, block editing, search, undo/redo, syntax highlighting, and more.

## Links

- [GitHub](https://github.com/avelino/roam-tui)
- [Documentation](https://roam-tui.avelino.run)
- [crates.io](https://crates.io/crates/roam-sdk)
- [License: MIT](https://github.com/avelino/roam-tui/blob/main/LICENSE)
