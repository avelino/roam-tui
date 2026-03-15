# MCP Tools Reference

All 18 tools available in the Roam MCP server.

## Read operations

### search

Search pages by title in the Roam graph.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | yes | Case-insensitive substring match against page titles |
| `limit` | number | no | Maximum results to return (default: no limit) |

Returns an array of `{title, uid}` objects.

### search_blocks

Full-text search inside block content across the entire graph.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | yes | Case-insensitive substring match against block text |
| `limit` | number | no | Maximum results (default: 50) |

Returns an array of `{uid, string, page_title}` objects.

### get_page

Get a page by title with its full block tree.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `title` | string | yes | Exact page title |

Returns the raw Roam pull result with `:node/title`, `:block/uid`, `:block/children`, `:block/string`, `:block/refs`.

### get_block

Get a block by UID with its full subtree.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `uid` | string | yes | Block UID |

Returns the block with children, refs, order, and open/collapsed state.

### get_daily_note

Get a daily note by date.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `date` | string | no | Date in `YYYY-MM-DD` format. Defaults to today. |

Returns the full block tree for that day's note.

### get_backlinks

Get all blocks across the graph that reference a page by title.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `title` | string | yes | Page title to find backlinks for |

Returns results grouped by source page: `[{page_title, blocks: [{uid, string}]}]`.

### get_block_refs

Get all outbound references from a block.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `uid` | string | yes | Block UID |

Returns referenced entities with their UIDs and titles (pages via `[[]]` and blocks via `(())`).

### get_graph_stats

Get graph statistics. No parameters.

Returns `{pages: number, blocks: number}`.

### export_page_as_markdown

Export a page as formatted markdown.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `title` | string | yes | Page title to export |

Returns a markdown string with `# Title` heading and indented bullet list preserving block hierarchy.

### roam_query

Run a raw Datalog query against the Roam graph.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `query` | string | yes | Datalog query string |
| `args` | string | no | JSON array of query arguments |

**Example queries:**

Find all pages:
```
[:find ?title ?uid :where [?e :node/title ?title] [?e :block/uid ?uid]]
```

Find blocks containing text (with args):
```
[:find ?uid ?s :in $ ?search :where [?b :block/string ?s] [?b :block/uid ?uid] [(clojure.string/includes? ?s ?search)]]
```
Args: `["search term"]`

## Write operations

### create_page

Create a new page.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `title` | string | yes | Page title |
| `uid` | string | no | Custom UID (auto-generated if omitted) |

### create_block

Create a single block under a parent.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `parent_uid` | string | yes | UID of parent block or page |
| `content` | string | yes | Block content (Roam markdown) |
| `order` | string | no | `"first"`, `"last"`, or numeric index (default: `"last"`) |

### create_block_with_children

Create a block with nested children in a single operation.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `parent_uid` | string | yes | UID of parent block or page |
| `content` | string | yes | Block content |
| `order` | string | no | Position (default: `"last"`) |
| `uid` | string | no | Custom UID for the parent block |
| `children` | string | no | JSON array of child block strings |

**Example children:** `["Child 1", "Child 2", "Child 3"]`

### update_block

Update the text content of an existing block.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `uid` | string | yes | Block UID |
| `content` | string | yes | New content |

### delete_block

Delete a block and all its children.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `uid` | string | yes | Block UID |

### delete_page

Delete a page and all its blocks.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `uid` | string | yes | Page UID |

### move_block

Move a block to a new parent.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `uid` | string | yes | Block UID to move |
| `parent_uid` | string | yes | UID of the new parent |
| `order` | string | no | Position (default: `"last"`) |

### batch_write

Execute multiple write operations in sequence. Each action is sent as an individual API request, stopping on the first error.

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `actions` | string | yes | JSON array of action objects |

Each action follows the Roam write API format:

```json
[
  {"action": "create-page", "page": {"title": "New Page"}},
  {"action": "create-block", "location": {"parent-uid": "abc", "order": "last"}, "block": {"string": "Content"}},
  {"action": "update-block", "block": {"uid": "xyz", "string": "Updated text"}},
  {"action": "delete-block", "block": {"uid": "old-uid"}},
  {"action": "move-block", "block": {"uid": "b1"}, "location": {"parent-uid": "new-parent", "order": 0}}
]
```

## Suggested workflows

### Read a page and summarize
1. `search` to find the page title
2. `get_page` or `export_page_as_markdown` to get content
3. Process the content

### Add structured notes
1. `create_page` to create a new page
2. `create_block_with_children` to add content with nested structure
3. Or use `batch_write` for complex multi-level structures

### Daily review
1. `get_daily_note` (no date = today)
2. `get_backlinks` to see what references today's note
3. `search_blocks` to find related content

### Graph exploration
1. `get_graph_stats` to understand graph size
2. `search` to find pages by topic
3. `get_backlinks` to discover connections
4. `get_block_refs` to follow outbound links from specific blocks
