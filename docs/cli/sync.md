# Sync

Sync your Roam graph to local markdown files. Inspired by [Obsidian Headless Sync](https://obsidian.md/help/sync/headless).

```bash
roam sync              # pull all pages to local markdown files
roam sync --dry-run    # show what would change without writing
```

## How it works

1. Queries the Roam API for the full page list (1 API call)
2. Compares against locally tracked pages in [ChronDB](https://github.com/avelino/chrondb)
3. Pulls only new/unsynced pages (1 API call per page)
4. Writes markdown files to the sync directory

Subsequent syncs skip already-synced pages — a stable graph with 500 pages costs **1 API call**.

## Output structure

```
~/.config/roam-tui/sync/
  pages/
    Project Alpha.md
    Meeting Notes.md
    C++ _ Rust.md          # special chars sanitized
  daily/                   # (future: daily notes)
```

Page titles with `/ \ : * ? " < > |` are replaced with `_` in filenames.

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--direction` | `pull` | Sync direction: `pull`, `push` (future), `both` (future) |
| `-d`, `--dir` | `~/.config/roam-tui/sync` | Output directory for markdown files |
| `--daily` | false | Include daily notes |
| `--dry-run` | false | Show what would change, don't write |
| `--concurrency` | `5` | Parallel page fetches |
| `--filter` | none | Only sync pages matching this prefix |
| `--history` | none | Show version history for a page UID |

## Examples

```bash
# Sync everything
roam sync

# Only pages starting with "Project/"
roam sync --filter "Project/"

# Custom output directory
roam sync --dir ~/notes/roam

# Preview without writing
roam sync --dry-run

# Check sync history for a page
roam sync --history "page-uid-here"
```

## Configuration

Default paths are set in `~/.config/roam-tui/config.toml`:

```toml
[sync]
dir = "~/.config/roam-tui/sync"        # markdown output
db_dir = "~/.config/roam-tui/.chrondb"  # ChronDB storage
```

Override via environment variables:

| Variable | Config equivalent |
|----------|-------------------|
| `ROAM_SYNC_DIR` | `sync.dir` |
| `ROAM_SYNC_DB__DIR` | `sync.db_dir` |

## Storage

Sync state is stored in [ChronDB](https://github.com/avelino/chrondb), a git-based key/value database. It tracks which pages have been synced and provides version history.

Default location: `~/.config/roam-tui/.chrondb`

ChronDB is opened and closed per operation (batch pattern) to avoid memory issues with large graphs.

## API usage

| Scenario | API calls |
|----------|-----------|
| First sync (500 pages) | 501 (1 list + 500 pulls) |
| Re-sync (nothing changed) | 1 (list only) |
| Re-sync (5 new pages) | 6 (1 list + 5 pulls) |

The Roam API has a limit of ~50 req/min/graph. With `--concurrency 5`, large initial syncs are throttled automatically.

## Roadmap

- **Phase 1** (current): Pull-only sync (remote → local)
- **Phase 2**: Push sync (local → remote) with markdown-to-blocks parser
- **Phase 3**: Bidirectional sync with conflict detection
- **Future**: `--watch` mode for continuous sync
