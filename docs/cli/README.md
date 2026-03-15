# CLI

The `roam` binary exposes all SDK operations as CLI subcommands. No subcommand launches the TUI.

```bash
roam                    # launch TUI
roam journal            # view today's daily note
roam search "meeting"   # search pages by title
roam get page "Books"   # get a page with all blocks
```

Output is JSON for structured data (pages, blocks, queries) and plain text for status messages.

## Commands

### journal (alias: j)

View or add to your daily note.

```bash
# View today's daily note
roam journal
roam j

# View a specific date
roam journal view --date 2026-03-10

# Add a block to today's note
roam journal add "Meeting with [[John]]"

# Add to a specific date, at the top
roam journal add "First block" --date 2026-03-10 --order first

# Add with children blocks
roam journal add "Parent block" --children '["child 1", "child 2"]'
```

| Flag | Description |
|------|-------------|
| `--date` | Date in YYYY-MM-DD format (default: today) |
| `--order` | Position: `first`, `last`, or numeric index |
| `--children` | JSON array of child block strings |

### search

Search pages by title or blocks by content.

```bash
# Search page titles
roam search "project"

# Search inside block content
roam search "action item" --blocks

# Limit results
roam search "meeting" --limit 5
```

| Flag | Description |
|------|-------------|
| `-b`, `--blocks` | Search block content instead of page titles |
| `-l`, `--limit` | Maximum number of results |

### get

Read pages, blocks, daily notes, backlinks, refs, and stats.

```bash
# Get a page with all blocks
roam get page "Books"

# Get a block by UID
roam get block "abc123def"

# Get today's daily note
roam get daily

# Get a specific date
roam get daily --date 2026-03-10

# Get backlinks (blocks referencing a page)
roam get backlinks "Project Alpha"

# Get outbound references from a block
roam get refs "abc123def"

# Get graph statistics
roam get stats
```

### query

Run raw Datalog queries against your graph.

```bash
# Find all pages
roam query '[:find ?title :where [?e :node/title ?title]]'

# Query with arguments
roam query '[:find ?uid ?s :where [?e :block/uid ?uid] [?e :block/string ?s]]' \
  --args '["some-uid"]'
```

| Flag | Description |
|------|-------------|
| `--args` | JSON array of query arguments |

### export

Export daily notes or pages to Markdown or JSON.

```bash
# Export today's daily note as markdown (stdout)
roam export

# Export a specific date as JSON
roam export --date 2026-03-10 --format json

# Export a page to a file
roam export --page "Books" --output books.md

# Export as JSON to file
roam export --page "Books" --format json --output books.json
```

| Flag | Description |
|------|-------------|
| `--date` | Date in YYYY-MM-DD format (default: today) |
| `--page` | Page title (exports page instead of daily note) |
| `--format` | Output format: `md` or `json` (default: `md`) |
| `-o`, `--output` | Output file path (default: stdout) |

### create

Create pages and blocks.

```bash
# Create a page
roam create page "New Project"

# Create a page with a specific UID
roam create page "New Project" --uid "custom-uid"

# Create a block under a parent
roam create block --parent "parent-uid" "Block content"

# Create at a specific position with children
roam create block --parent "parent-uid" "Parent" --order first \
  --children '["child 1", "child 2"]'
```

### update

Update block content.

```bash
roam update block "block-uid" "New content for this block"
```

### delete

Delete blocks or pages.

```bash
roam delete block "block-uid"
roam delete page "page-uid"
```

### move

Move a block to a new parent.

```bash
# Move to a new parent (appends at end)
roam move block "block-uid" --parent "new-parent-uid"

# Move to a specific position
roam move block "block-uid" --parent "new-parent-uid" --order first
```

| Flag | Description |
|------|-------------|
| `--parent` | New parent block/page UID |
| `--order` | Position: `first`, `last`, or numeric index |

### batch

Execute multiple write operations from a JSON file or stdin.

```bash
# From a file
roam batch operations.json

# From stdin
echo '[{"action": "create-block", ...}]' | roam batch

# Pipe from another command
cat operations.json | roam batch -
```

The input is a JSON array of [WriteAction](../sdk/types.md) objects.

### mcp

Start the MCP server over stdio (used by AI assistants).

```bash
roam mcp
roam --mcp  # legacy flag, same behavior
```

See [MCP Server docs](../mcp/README.md) for configuration details.

## Composing with other tools

The CLI outputs JSON, making it easy to pipe into `jq`, scripts, or other tools:

```bash
# Get all page titles containing "project"
roam search "project" | jq -r '.[].title'

# Export today's note and send to clipboard
roam export | pbcopy

# Add a timestamped entry
roam journal add "$(date +%H:%M) — Started deep work session"

# Batch create from a list
cat <<'EOF' | jq -c '[.[] | {action: "create-block", location: {parent_uid: "daily-uid", order: "last"}, block: {string: .}}]' | roam batch
["Task 1", "Task 2", "Task 3"]
EOF
```
