# TUI Overview

`roam` is a terminal-based client for Roam Research. It lets you browse daily notes, navigate pages, edit blocks, and search — all from your terminal.

## Features

- Browse daily notes with infinite scroll (older days load on demand)
- Navigate to any page via `[[links]]` or the search popup
- Edit blocks with optimistic updates (changes apply instantly, sync in background)
- Undo/redo for text edits, block creation, deletion, and moves
- Indent/dedent blocks with Tab/Shift+Tab
- Reorder blocks among siblings with Alt+Up/Alt+Down
- Block references `((uid))` resolve inline
- Linked references section per day/page
- Syntax highlighting for fenced code blocks (14 languages)
- Vim, Emacs, and VSCode keybinding presets
- [Multi-block selection](multi-select.md) with batch delete, indent, and dedent
- [Export](../export.md) current view to markdown (Ctrl+E)
- Dark and light themes
- Auto-refresh every 30 seconds

## Modal interface

The TUI uses a modal state machine:

| Mode | Description |
|---|---|
| **Normal** | Navigate blocks, open pages, trigger search |
| **Insert** | Edit block text, cursor movement, paired brackets |
| **Search** | Filter blocks by text, jump to result |
| **Autocomplete** | Type `((` in insert mode to search and insert block references |

Press `Esc` to return to Normal mode from any other mode.
