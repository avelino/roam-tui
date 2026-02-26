# Configuration

Config file location: `~/.config/roam-tui/config.toml`

## Full example

```toml
[graph]
name = "my-graph"
api_token = "roam-graph-token-..."

[ui]
theme = "dark"
sidebar_default = true
sidebar_width_percent = 35

[keybindings]
preset = "vim"

[keybindings.bindings]
quit = "Ctrl+q"
search = "Ctrl+f"
```

## Options

### `[graph]` — required

| Key | Type | Description |
|---|---|---|
| `name` | string | Your Roam graph name (as shown in the URL) |
| `api_token` | string | API token with read/write access |

### `[ui]` — optional

| Key | Type | Default | Description |
|---|---|---|---|
| `theme` | string | `"dark"` | Color theme: `"dark"` or `"light"` |
| `sidebar_default` | bool | `true` | Show sidebar on startup |
| `sidebar_width_percent` | u16 | `35` | Sidebar width as percentage of terminal |

### `[keybindings]` — optional

| Key | Type | Default | Description |
|---|---|---|---|
| `preset` | string | `"vim"` | Base preset: `"vim"`, `"emacs"`, or `"vscode"` |

### `[keybindings.bindings]` — optional

Override individual keys from the preset. Keys are action names, values are key strings.

```toml
[keybindings.bindings]
quit = "Ctrl+q"
search = "Ctrl+f"
move_up = "Ctrl+k"
```

See [Keybindings](keybindings.md) for all available actions and key format.

## Environment variables

Every config option can be set via environment variables with the `ROAM_` prefix. Use `__` (double underscore) for nesting.

| Variable | Config equivalent |
|---|---|
| `ROAM_GRAPH_NAME` | `graph.name` |
| `ROAM_GRAPH_API__TOKEN` | `graph.api_token` |
| `ROAM_UI_THEME` | `ui.theme` |
| `ROAM_UI_SIDEBAR__DEFAULT` | `ui.sidebar_default` |
| `ROAM_UI_SIDEBAR__WIDTH__PERCENT` | `ui.sidebar_width_percent` |
| `ROAM_KEYBINDINGS_PRESET` | `keybindings.preset` |

Environment variables override file values. This is useful for keeping tokens out of config files:

```bash
export ROAM_GRAPH_API__TOKEN="roam-graph-token-..."
roam
```
