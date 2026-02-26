# roam-tui

[![Docs](https://img.shields.io/badge/docs-roam--tui.avelino.run-blue)](https://roam-tui.avelino.run)
[![Crates.io](https://img.shields.io/crates/v/roam-sdk)](https://crates.io/crates/roam-sdk)

A fast, keyboard-driven terminal client and Rust SDK for [Roam Research](https://roamresearch.com). Navigate, edit, and search your knowledge graph without leaving the terminal — or use the SDK to build your own tools.

**[Documentation](https://roam-tui.avelino.run)** | **[crates.io](https://crates.io/crates/roam-sdk)**

```
┌─────────────────────────────────────────────────────────┐
│ roam-tui   [Graph: my-graph]              Feb 24, 2026  │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  February 24, 2026                                      │
│                                                         │
│  • Meeting notes with [[John]]                          │
│    • Discussed Q1 roadmap                               │
│    • Action items: ((ref-uid))                          │
│  ▸ Project ideas  [3 collapsed]                         │
│  • Read chapter 5 of **Deep Work**                      │
│    • Key insight: time blocking works                   │
│                                                         │
│  February 23, 2026                                      │
│                                                         │
│  • Finished migration to new API                        │
│  • {{DONE}} Review PR #42                               │
│                                                         │
├─────────────────────────────────────────────────────────┤
│ [i]edit  [o]new  [/]search  [?]help  [h/l]collapse/expand│
└─────────────────────────────────────────────────────────┘
```

## Why

Roam Research is powerful but lives in the browser. If you spend most of your day in the terminal, context switching costs add up. `roam-tui` gives you direct access to your graph — fast, focused, and keyboard-first.

**Not a replacement** for the web UI. Use `roam-tui` for quick capture, daily note workflows, and navigating your graph. Use the web for complex queries, graph visualization, and plugin-heavy workflows.

## Installation

### From source

```bash
git clone https://github.com/avelino/roam-tui
cd roam-tui
cargo install --path .
```

### Requirements

- Rust 1.70+
- A Roam Research graph with an API token ([how to get one](https://roamresearch.com/#/app/developer-documentation))

## Quick start

Run `roam` for the first time — it creates a config file and tells you where:

```bash
roam
# → Created default config at: ~/.config/roam-tui/config.toml
# → Please edit it with your Roam graph name and API token, then run again.
```

Add your credentials:

```toml
# ~/.config/roam-tui/config.toml

[graph]
name = "your-graph-name"
api_token = "roam-graph-token-XXXXX"
```

Or use an environment variable:

```bash
export ROAM_GRAPH_API__TOKEN="roam-graph-token-XXXXX"
```

Run again:

```bash
roam
```

Your daily note loads. Start navigating.

## Features

### Daily notes

Opens today's note on launch. Navigate to previous days with `P` / `PageUp` (vim) or scroll past the last block to auto-load older days. Jump back to today with `G`.

### Outline editing

Full block editing with vim-style modal workflow:

- `i` to edit the selected block, `Esc` to save and exit
- `o` to create a new block below
- `dd` to delete a block
- `Tab` / `Shift+Tab` to indent/unindent
- Undo with `u`, redo with `Ctrl+R`
- Auto-pairing for `()`, `[]`, `{}`
- `{{TODO}}` / `{{DONE}}` toggle with `Alt+Enter` or `Ctrl+Enter`

### Collapse and expand

Blocks with children can be collapsed (`h`) and expanded (`l`). Press `Enter` to toggle. Collapsed blocks hide their children from navigation and display — just like the web UI.

### Search

Press `/` to open a search popup. Type to filter across all loaded blocks and cached references. Navigate results with arrow keys, press `Enter` to jump to the block.

### Block references

Type `((` in edit mode to open the autocomplete popup. Search for any block by content, select with `Enter`. References are resolved and displayed inline.

### Markdown rendering

Blocks render with full Roam syntax support:

| Syntax | Rendering |
|--------|-----------|
| `**bold**` | Bold text |
| `__italic__` | Italic text |
| `~~strikethrough~~` | Strikethrough |
| `^^highlight^^` | Highlighted text |
| `` `code` `` | Inline code |
| `[[Page Name]]` | Page link (cyan) |
| `((block-uid))` | Block reference (resolved) |
| `#tag` | Tag (cyan) |
| `{{TODO}}` / `{{DONE}}` | Checkbox markers |
| `{{embed: ((uid))}}` | Embedded block |

### Code blocks

Fenced code blocks (` ``` `) render with syntax highlighting via tree-sitter. Supported languages: Rust, Python, JavaScript, TypeScript, Go, C, Bash, JSON, TOML, YAML, HTML, CSS, Markdown.

### Help

Press `?` to see all keybindings for your current preset. Any key closes the help overlay.

### Error display

API errors (rate limits, auth failures, network issues) appear as a popup overlay with a human-readable title and hint instead of raw JSON. Any key dismisses the popup.

## Keybindings

Three built-in presets. Set with `keybindings.preset` in config.

### Navigation

| Action | vim | emacs | vscode |
|--------|-----|-------|--------|
| Move up | `k` / `Up` | `Ctrl+P` / `Up` | `Up` |
| Move down | `j` / `Down` | `Ctrl+N` / `Down` | `Down` |
| Cursor left | `Left` | `Left` | `Left` |
| Cursor right | `Right` | `Right` | `Right` |
| Collapse | `h` | `Ctrl+B` | `Ctrl+Left` |
| Expand | `l` | `Ctrl+F` | `Ctrl+Right` |
| Toggle open | `Enter` | `Enter` | `Enter` |
| Next day | `N` / `PageDown` | `Alt+N` / `PageDown` | `Alt+Up` / `PageDown` |
| Previous day | `P` / `PageUp` | `Alt+P` / `PageUp` | `Alt+Down` / `PageUp` |
| Go to today | `G` | `Ctrl+D` | `Ctrl+D` |
| Navigate back | `Ctrl+O` / `Shift+Left` / `Alt+[` | `Shift+Left` / `Alt+[` | `Shift+Left` / `Alt+[` |
| Navigate forward | `Shift+Right` / `Alt+]` | `Shift+Right` / `Alt+]` | `Shift+Right` / `Alt+]` |

### Actions

| Action | vim | emacs | vscode |
|--------|-----|-------|--------|
| Edit block | `i` | `Enter` | `Enter` |
| New block | `o` | `Alt+Enter` | `Ctrl+Enter` |
| Delete block | `dd` | — | — |
| Search | `/` | `Ctrl+S` | `Ctrl+Shift+F` |
| Undo | `u` | `Ctrl+/` | `Ctrl+Z` |
| Redo | `Ctrl+R` | `Ctrl+Shift+/` | `Ctrl+Shift+Z` |
| Help | `?` | `Ctrl+H` | `F1` |
| Quick switcher | `Ctrl+P` | — | `Ctrl+P` |
| Toggle sidebar | `b` | — | `Ctrl+B` |
| Quit | `q` | `Ctrl+Q` | `Ctrl+Q` |

### Edit mode

| Action | Key |
|--------|-----|
| Save and exit | `Esc` |
| Move cursor | Arrow keys, `Home`, `End` |
| Move by word | `Ctrl+Left` / `Ctrl+Right` |
| Toggle TODO | `Alt+Enter` or `Ctrl+Enter` |
| Block ref autocomplete | `((` |
| Indent | `Tab` |
| Unindent | `Shift+Tab` |

### Custom keybindings

Override any key in the config file:

```toml
[keybindings]
preset = "vim"

[keybindings.bindings]
quit = "Ctrl+q"
search = "Ctrl+f"
```

## Configuration

Config file location: `~/.config/roam-tui/config.toml`

All options with defaults:

```toml
[graph]
name = ""                    # required — your Roam graph name
api_token = ""               # required — or set ROAM_GRAPH_API__TOKEN env var

[ui]
theme = "dark"               # dark | light
sidebar_default = true       # show sidebar on startup
sidebar_width_percent = 35   # sidebar width as percentage

[keybindings]
preset = "vim"               # vim | emacs | vscode

# [keybindings.bindings]     # override individual keys
# quit = "Ctrl+q"
```

Environment variables override config file values. Prefix with `ROAM_`, use `__` for nesting:

```bash
ROAM_GRAPH_NAME=my-graph
ROAM_GRAPH_API__TOKEN=roam-graph-token-XXXXX
```

## Architecture

```
src/
  main.rs             Entry point, terminal setup
  app.rs              State machine, event loop, actions
  config.rs           TOML + env var configuration
  edit_buffer.rs      Text editing with cursor management
  markdown.rs         Roam-flavored markdown parser
  highlight.rs        Tree-sitter syntax highlighting
  api/
    client.rs         HTTP client (reqwest + rustls)
    queries.rs        Datalog query builders
    types.rs          Block, DailyNote, WriteAction types
  keys/
    mod.rs            Keybinding resolution
    preset.rs         vim/emacs/vscode presets
    parser.rs         Key string parser ("Ctrl+k" → KeyEvent)
  ui/
    mod.rs            Layout + popups (search, help, autocomplete)
    header.rs         Graph name + date
    main_area.rs      Block tree rendering
    status_bar.rs     Hints + mode indicator
```

Built on [Ratatui](https://ratatui.rs) + [Tokio](https://tokio.rs) + [Roam Backend API](https://roamresearch.com/#/app/developer-documentation).

## Roadmap

What's working now and what's next.

### Implemented

- [x] Daily notes with multi-day navigation
- [x] Block tree editing (create, edit, delete, indent, move)
- [x] Collapse/expand blocks
- [x] Undo/redo
- [x] Full-text search across blocks
- [x] Block reference autocomplete (`((`)
- [x] Markdown + Roam syntax rendering
- [x] Code block syntax highlighting (14 languages)
- [x] Three keybinding presets (vim, emacs, vscode)
- [x] Custom keybinding overrides
- [x] Help overlay
- [x] Optimistic UI updates (no lag on edits)
- [x] Auto-refresh from API
- [x] Page navigation (follow `[[links]]`)
- [x] Navigation history (back/forward)
- [x] Cursor navigation (left/right within blocks)
- [x] User-friendly error popups (rate limits, auth, network)

### Planned

- [ ] Quick switcher (fuzzy page navigation)
- [ ] Linked references / backlinks panel
- [ ] Sidebar with page references
- [ ] Unlinked references
- [ ] Light theme
- [ ] Breadcrumb display

## Development

```bash
git clone https://github.com/avelino/roam-tui
cd roam-tui
cargo test     # 339 tests
cargo run      # requires config with valid API token
```

Tests run without network access — API interactions use [wiremock](https://github.com/LukeMathWalker/wiremock-rs) for mocking.

## License

MIT — see [LICENSE](LICENSE) for details.
