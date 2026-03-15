# MCP Server

The roam-sdk includes a built-in [Model Context Protocol](https://modelcontextprotocol.io/) (MCP) server that exposes your Roam graph to AI assistants like Claude, Cursor, and other MCP-compatible clients.

## Features

- **18 tools** covering read, write, search, and export operations
- Runs over **stdio transport** — works with any MCP client
- **Batch writes** — execute multiple operations in a single API call
- **Full-text search** — search inside block content, not just page titles
- **Daily note access** — retrieve notes by date or today's note
- **Markdown export** — convert pages to clean markdown
- **Graph statistics** — page and block counts at a glance

## Quick start

```bash
# Run directly
roam --mcp

# Or via npx (no install needed)
npx roam-tui@latest --mcp
```

The server reads config from `~/.config/roam-tui/config.toml` or environment variables.

## Next steps

- [Setup & Configuration](setup.md) — configure your MCP client
- [Tools Reference](tools.md) — all 18 tools with parameters and examples
