# Keybindings

## Presets

Set the preset in your config:

```toml
[keybindings]
preset = "vim"  # or "emacs" or "vscode"
```

## Vim (default)

### Normal mode

| Action | Keys |
|---|---|
| Move up | `k` / `Up` |
| Move down | `j` / `Down` |
| Collapse block | `h` |
| Expand block | `l` |
| Enter / toggle | `Enter` |
| Edit block | `i` |
| Create block below | `o` |
| Delete block | `dd` |
| Undo | `u` |
| Redo | `Ctrl+R` |
| Indent | `Tab` |
| Unindent | `Shift+Tab` |
| Search | `/` |
| Quick switcher | `Ctrl+P` |
| Next day | `N` / `PageDown` |
| Previous day | `P` / `PageUp` |
| Go to today | `G` |
| Toggle sidebar | `b` |
| Navigate back | `Ctrl+O` / `Shift+Left` / `Alt+[` |
| Navigate forward | `Shift+Right` / `Alt+]` |
| Help | `?` |
| Quit | `q` |

### Insert mode

| Action | Keys |
|---|---|
| Exit to normal | `Esc` |
| Move cursor | Arrow keys |
| Word left/right | `Ctrl+Left` / `Ctrl+Right` |
| Home / End | `Home` / `End` or `Ctrl+A` / `Ctrl+E` |
| Toggle TODO | `Ctrl+Enter` or `Alt+Enter` |
| Indent block | `Tab` |
| Dedent block | `Shift+Tab` |
| Block ref autocomplete | Type `((` |

## Emacs

| Action | Keys |
|---|---|
| Move up | `Ctrl+P` / `Up` |
| Move down | `Ctrl+N` / `Down` |
| Collapse block | `Ctrl+B` |
| Expand block | `Ctrl+F` |
| Enter / toggle | `Enter` |
| Edit block | `Enter` |
| Create block below | `Alt+Enter` |
| Undo | `Ctrl+/` |
| Redo | `Ctrl+Shift+/` |
| Search | `Ctrl+S` |
| Next day | `Alt+N` / `PageDown` |
| Previous day | `Alt+P` / `PageUp` |
| Go to today | `Ctrl+D` |
| Navigate back | `Shift+Left` / `Alt+[` |
| Navigate forward | `Shift+Right` / `Alt+]` |
| Help | `Ctrl+H` |
| Quit | `Ctrl+Q` |

## VSCode

| Action | Keys |
|---|---|
| Move up | `Up` |
| Move down | `Down` |
| Collapse block | `Ctrl+Left` |
| Expand block | `Ctrl+Right` |
| Enter / toggle | `Enter` |
| Edit block | `Enter` |
| Create block below | `Ctrl+Enter` |
| Undo | `Ctrl+Z` |
| Redo | `Ctrl+Shift+Z` |
| Search | `Ctrl+Shift+F` |
| Quick switcher | `Ctrl+P` |
| Next day | `Alt+Up` / `PageDown` |
| Previous day | `Alt+Down` / `PageUp` |
| Go to today | `Ctrl+D` |
| Toggle sidebar | `Ctrl+B` |
| Navigate back | `Shift+Left` / `Alt+[` |
| Navigate forward | `Shift+Right` / `Alt+]` |
| Help | `F1` |
| Quit | `Ctrl+Q` |

## Custom overrides

Override any action from the preset:

```toml
[keybindings.bindings]
quit = "Ctrl+q"
search = "Ctrl+f"
move_up = "Ctrl+k"
move_down = "Ctrl+j"
```

### Key format

Modifiers: `Ctrl`, `Alt`, `Shift` (case-insensitive), combined with `+`.

Special keys: `Enter`, `Esc`, `Tab`, `Backspace`, `Delete`, `Up`, `Down`, `Left`, `Right`, `Home`, `End`, `PageUp`, `PageDown`, `F1`–`F12`.

Examples: `Ctrl+k`, `Alt+Enter`, `Shift+Left`, `Ctrl+Shift+Z`.

### Available actions

`quit`, `move_up`, `move_down`, `cursor_left`, `cursor_right`, `collapse`, `expand`, `enter`, `exit`, `edit_block`, `create_block`, `indent`, `unindent`, `undo`, `redo`, `search`, `quick_switcher`, `next_day`, `prev_day`, `go_daily`, `toggle_sidebar`, `nav_back`, `nav_forward`, `help`
