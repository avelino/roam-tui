# Multi-Block Selection

Select multiple blocks at once and perform batch operations: delete, indent, or dedent all selected blocks in one step.

## Selecting blocks

Extend the selection with `Shift+Up` and `Shift+Down` from the current cursor position. The selection is always a contiguous range.

| Action | Vim | Emacs | VSCode |
|--------|-----|-------|--------|
| Select up | `Shift+Up` | `Shift+Up` | `Shift+Up` |
| Select down | `Shift+Down` | `Shift+Down` | `Shift+Down` |

All selected blocks are visually highlighted.

### Resetting selection

Any regular movement key (`j`/`k`, arrows, page up/down) collapses the selection back to a single block cursor.

## Batch operations

With multiple blocks selected:

| Operation | Key | Effect |
|-----------|-----|--------|
| Delete | `dd` | Deletes all selected blocks |
| Indent | `Tab` | Indents all selected blocks under previous sibling |
| Dedent | `Shift+Tab` | Dedents all selected blocks to parent level |

### Delete

All blocks in the selection range are removed. The API call uses batch-actions to delete them in a single request.

### Indent / Dedent

Each block in the selection is indented (or dedented) individually, preserving relative order. All moves are sent as a batch API call.

## Undo

Batch operations create a single undo entry. Pressing undo (`u` in Vim, `Ctrl+/` in Emacs, `Ctrl+Z` in VSCode) reverts the entire batch operation in one step — all deleted blocks are restored, or all indent/dedent moves are reverted.

## Limitations

- Selection is always a **contiguous range** (no non-contiguous multi-select like Cmd+Click in Roam web)
- Selection does not span across **linked refs** sections
- Entering **insert mode** resets the selection
- Selection works only in **Normal mode**
