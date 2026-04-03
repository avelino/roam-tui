# Sync

Bidirectional sync between your Roam graph and local markdown files. Edit in Roam or in your editor — changes flow both ways.

```bash
roam sync                       # pull changes from Roam
roam sync --direction push      # push local edits to Roam
roam sync --direction both      # pull then push
```

## How it works

### Pull (remote → local)

1. Queries the Roam API for the full page list (1 API call)
2. Queries pages modified since last sync using `:page/edit-time` (1 API call)
3. Compares against locally tracked pages in [ChronDB](https://github.com/avelino/chrondb)
4. Pulls only new or modified pages (1 API call per page)
5. Writes markdown files to the sync directory
6. Commits and pushes to git remote (if configured)

Daily notes are synced first (most recent first), then regular pages.

### Push (local → remote)

1. Reads all local `.md` files
2. Parses markdown back into Roam's block tree structure
3. Pulls current state from Roam to get block UIDs
4. Diffs local blocks against remote blocks by position in the tree
5. Applies changes to Roam via API:
   - **Updated blocks** → content changed in place
   - **New blocks** → created with parent UID and children
   - **Deleted blocks** → removed from Roam

### Bidirectional (`--direction both`)

Runs pull first, then push. Safe ordering — remote changes are downloaded before local changes are pushed, avoiding blind overwrites.

## Output structure

```
~/.config/roam-tui/sync/
  pages/
    Project Alpha.md
    Meeting Notes.md
    C++ _ Rust.md          # special chars sanitized
  daily/
    03-30-2026.md          # daily notes by UID
    03-29-2026.md
```

Page titles with `/ \ : * ? " < > |` are replaced with `_` in filenames.

## Markdown format

Files use a simple format that round-trips cleanly with Roam:

```markdown
# Page Title

- First block
  - Child block
    - Grandchild
- Second block
- [[Page Reference]] and ((block-ref)) preserved as-is
```

Each `- ` line is a block. Indentation (2 spaces per level) represents nesting. Roam syntax (`[[links]]`, `((refs))`, `{{commands}}`) is preserved verbatim.

## Flags

| Flag | Default | Description |
|------|---------|-------------|
| `--direction` | `pull` | Sync direction: `pull`, `push`, or `both` |
| `-d`, `--dir` | config `sync.dir` | Output directory for markdown files |
| `--daily` | false | Include daily notes |
| `--dry-run` | false | Show what would change, don't write |
| `--concurrency` | `5` | Parallel page fetches |
| `--filter` | none | Only sync pages matching this prefix |
| `--history` | none | Show version history for a page UID |

## Examples

```bash
# Pull everything from Roam
roam sync

# Edit a file locally, then push changes back
vim ~/.config/roam-tui/sync/pages/Project\ Alpha.md
roam sync --direction push

# Full bidirectional sync
roam sync --direction both

# Only pages starting with "Project/"
roam sync --filter "Project/"

# Preview without writing
roam sync --dry-run

# Custom output directory
roam sync --dir ~/notes/roam
```

## Configuration

```toml
[sync]
dir = "~/.config/roam-tui/sync"               # markdown output
db_dir = "~/.config/roam-tui/.chrondb"         # ChronDB storage
remote = "git@github.com:user/notes.git"       # git remote for markdown files
```

Override via environment variables:

| Variable | Config equivalent |
|----------|-------------------|
| `ROAM_SYNC_DIR` | `sync.dir` |
| `ROAM_SYNC_DB__DIR` | `sync.db_dir` |
| `ROAM_SYNC_REMOTE` | `sync.remote` |

## Git remote

When `remote` is configured, `roam sync` automatically commits and pushes markdown files after each sync. This gives you a versioned backup of your notes on GitHub, GitLab, or any git remote.

The git repo is initialized automatically in the sync directory on first run. SSH keys from your system are used for authentication.

## Change detection

Pull uses Roam's `:page/edit-time` attribute to detect which pages changed since the last sync. This catches direct edits and changes to blocks referenced on the page.

Push compares local markdown against the current Roam state by position in the block tree. Blocks are matched by their order — content differences generate updates, missing blocks generate deletions, extra blocks generate creations.

## Storage

Sync state is stored in [ChronDB](https://github.com/avelino/chrondb), a git-based key/value database. It tracks which pages have been synced, the last sync timestamp, and provides version history.

Default location: `~/.config/roam-tui/.chrondb`

## Graceful interruption

Press `Ctrl+C` during sync to stop gracefully. The current page finishes, all progress is saved to ChronDB, and markdown files already written remain on disk. Run `roam sync` again to continue from where you left off.

If ChronDB's index gets corrupted (e.g., from a hard kill), it's rebuilt automatically from git data on the next run.

## API usage

| Scenario | API calls |
|----------|-----------|
| First sync (500 pages) | 501 (1 list + 500 pulls) |
| Re-sync (nothing changed) | 2 (list + modified check) |
| Re-sync (3 pages changed) | 5 (list + modified + 3 pulls) |
| Push (10 files changed) | 10 pulls + N writes |
