# Export

Export daily notes and pages to Markdown or JSON format. Available both as a CLI subcommand (without launching the TUI) and as an in-app keybinding.

## CLI export

### Daily note

```bash
# Today's note as markdown (stdout)
roam export

# Specific date
roam export --date 2026-03-15

# As JSON
roam export --date 2026-03-15 --format json

# Write to file
roam export --date 2026-03-15 --output notes.md
```

### Page

```bash
# Export a page by title
roam export --page "Project Alpha" --format md

# As JSON to file
roam export --page "Meeting Notes" --format json --output meeting.json
```

### Options

| Flag | Description | Default |
|------|-------------|---------|
| `--date` | Date in `YYYY-MM-DD` format | Today |
| `--page` | Page title (overrides --date) | — |
| `--format` | `md` or `json` | `md` |
| `--output`, `-o` | File path | stdout |

## In-TUI export

Press the export keybinding while viewing any page or daily notes:

| Preset | Keybinding |
|--------|-----------|
| Vim | `Ctrl+E` |
| Emacs | `Alt+E` |
| VSCode | `Ctrl+E` |

The current view is exported as markdown to `~/roam-export/<title>.md`. A status message shows the file path.

## Markdown format

The exported markdown preserves Roam's block hierarchy using indented bullet points:

```markdown
# March 15th, 2026

- Meeting with [[John]]
  - Discussed Q1 roadmap
  - Action items
    - Follow up on budget
- **Deep Work** chapter 5
  - Key insight: time blocking works
```

- Indentation: 2 spaces per depth level
- Roam syntax is preserved: `[[links]]`, `((block refs))`, `{{TODO}}`, `**bold**`, etc.
- Code blocks keep their fenced syntax
- Multiple daily notes are separated by `---`

## JSON format

```json
[
  {
    "title": "March 15th, 2026",
    "uid": "03-15-2026",
    "date": "2026-03-15",
    "blocks": [
      {
        "uid": "abc123",
        "string": "Meeting with [[John]]",
        "order": 0,
        "open": true,
        "children": [
          {
            "uid": "def456",
            "string": "Discussed Q1 roadmap",
            "order": 0,
            "open": true,
            "children": []
          }
        ]
      }
    ]
  }
]
```

Each block includes `uid`, `string`, `order`, `open`, and nested `children`.
