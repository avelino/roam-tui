# Types

All types are in `roam_sdk::types` (re-exported from `roam_sdk::api::types`).

## Data structures

### `Block`

A block in the Roam graph. Blocks form a tree via `children`.

```rust
pub struct Block {
    pub uid: String,
    pub string: String,
    pub order: i64,
    pub children: Vec<Block>,
    pub open: bool,
    pub refs: Vec<RefEntity>,
}
```

- `uid` — unique block identifier
- `string` — the block's text content (Roam markdown)
- `order` — sort position among siblings
- `children` — nested child blocks (recursive tree)
- `open` — whether children are expanded or collapsed
- `refs` — page/block references contained in this block (not serialized to JSON)

### `RefEntity`

A reference target found in a block.

```rust
pub struct RefEntity {
    pub uid: String,
    pub title: Option<String>,   // page title, if it's a page ref
    pub string: Option<String>,  // block text, if it's a block ref
}
```

### `DailyNote`

A daily note page with its blocks.

```rust
pub struct DailyNote {
    pub date: chrono::NaiveDate,
    pub uid: String,
    pub title: String,
    pub blocks: Vec<Block>,
}
```

Parse from a pull response:

```rust
let note = DailyNote::from_pull_response(date, uid, &pull_response.result);
```

Blocks are automatically sorted by `order`. Nested children are parsed recursively.

### `LinkedRefBlock` / `LinkedRefGroup`

Results from a linked references query, grouped by source page.

```rust
pub struct LinkedRefBlock {
    pub uid: String,
    pub string: String,
    pub page_title: String,
}

pub struct LinkedRefGroup {
    pub page_title: String,
    pub blocks: Vec<LinkedRefBlock>,
}
```

Parse from a query response:

```rust
let groups = parse_linked_refs(&query_response.result, "Current Page");
```

Self-references (blocks from the current page) are automatically filtered out. Groups are sorted alphabetically by page title, blocks within each group sorted by text.

## Write actions

### `WriteAction`

An enum representing mutations to the graph.

```rust
pub enum WriteAction {
    CreateBlock { location: BlockLocation, block: NewBlock },
    UpdateBlock { block: BlockUpdate },
    DeleteBlock { block: BlockRef },
    MoveBlock { block: BlockRef, location: BlockLocation },
}
```

Serializes with a `"action"` tag: `"create-block"`, `"update-block"`, `"delete-block"`, `"move-block"`.

### `BlockLocation`

Where to place a block.

```rust
pub struct BlockLocation {
    pub parent_uid: String,
    pub order: OrderValue,
}
```

Serializes `parent_uid` as `"parent-uid"`.

### `OrderValue`

Position within siblings.

```rust
pub enum OrderValue {
    Index(i64),           // specific position (0-based)
    Position(String),     // "last", "first"
}
```

Serializes as either a number (`0`) or a string (`"last"`).

### `NewBlock`

Data for creating a block.

```rust
pub struct NewBlock {
    pub string: String,
    pub uid: Option<String>,
    pub open: Option<bool>,
}
```

`uid` and `open` are omitted from JSON when `None`.

### `BlockUpdate`

Data for updating a block's text.

```rust
pub struct BlockUpdate {
    pub uid: String,
    pub string: String,
}
```

### `BlockRef`

A block reference (for delete and move).

```rust
pub struct BlockRef {
    pub uid: String,
}
```

## API request/response types

### `PullResponse`

```rust
pub struct PullResponse {
    pub result: serde_json::Value,
}
```

The `result` is a raw JSON value matching the pull selector shape. Use `.get(":attribute")` to access fields.

### `QueryResponse`

```rust
pub struct QueryResponse {
    pub result: Vec<Vec<serde_json::Value>>,
}
```

Each inner `Vec` is a result row. Values correspond to `:find` variables in order.

## Error types

### `RoamError`

```rust
pub enum RoamError {
    Api { status: u16, message: String },
    Http(reqwest::Error),
    Config(String),
    Io(std::io::Error),
    Json(serde_json::Error),
    TomlDe(toml::de::Error),
}
```

Implements `std::error::Error` and `Display`. Conversions from `reqwest::Error`, `std::io::Error`, `serde_json::Error`, and `toml::de::Error` via `From`.

### `Result<T>`

```rust
pub type Result<T> = std::result::Result<T, RoamError>;
```
