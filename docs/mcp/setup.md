# MCP Setup

## Configuration

The MCP server needs your Roam graph name and API token. Configure via environment variables or config file.

### Environment variables

```bash
export ROAM_GRAPH_NAME="your-graph-name"
export ROAM_GRAPH_API__TOKEN="roam-graph-token-..."
```

### Config file

Same config file as the TUI (`~/.config/roam-tui/config.toml`):

```toml
[graph]
name = "your-graph-name"

[graph.api]
token = "roam-graph-token-..."
```

## Client configuration

### Claude Desktop / Claude Code

Add to your MCP settings:

```json
{
  "mcpServers": {
    "roam": {
      "command": "npx",
      "args": ["-y", "roam-tui@latest", "--mcp"],
      "env": {
        "ROAM_GRAPH_NAME": "your-graph-name",
        "ROAM_GRAPH_API__TOKEN": "roam-graph-token-..."
      }
    }
  }
}
```

### Using a local binary

If you installed via `cargo install roam-sdk`:

```json
{
  "mcpServers": {
    "roam": {
      "command": "roam",
      "args": ["--mcp"],
      "env": {
        "ROAM_GRAPH_NAME": "your-graph-name",
        "ROAM_GRAPH_API__TOKEN": "roam-graph-token-..."
      }
    }
  }
}
```

### Cursor

Add to `.cursor/mcp.json` in your project or `~/.cursor/mcp.json` globally:

```json
{
  "mcpServers": {
    "roam": {
      "command": "npx",
      "args": ["-y", "roam-tui@latest", "--mcp"],
      "env": {
        "ROAM_GRAPH_NAME": "your-graph-name",
        "ROAM_GRAPH_API__TOKEN": "roam-graph-token-..."
      }
    }
  }
}
```

## Supported platforms

The npm package includes pre-built binaries for:

| Platform | Architecture |
|----------|-------------|
| macOS | ARM64 (Apple Silicon), x86_64 |
| Linux | x86_64, ARM64 |
| Windows | x86_64 |

## Verifying

Once configured, ask your AI assistant to search your Roam graph:

> "Search my Roam graph for pages about 'project'"

It should use the `search` tool and return matching page titles.
