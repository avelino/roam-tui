# Getting Started

## Add the dependency

```bash
cargo add roam-sdk
```

Or add to `Cargo.toml`:

```toml
[dependencies]
roam-sdk = "0.1"
tokio = { version = "1", features = ["full"] }
```

## Create a client

```rust
use roam_sdk::RoamClient;

let client = RoamClient::new("your-graph-name", "your-api-token");
```

The graph name is the one that appears in your Roam URL. The API token can be generated in Roam under **Settings > Graph > API tokens**.

## Read a daily note

```rust
use roam_sdk::{RoamClient, queries, types::DailyNote};
use chrono::NaiveDate;

#[tokio::main]
async fn main() -> roam_sdk::Result<()> {
    let client = RoamClient::new("my-graph", "my-token");

    let date = NaiveDate::from_ymd_opt(2026, 2, 21).unwrap();
    let uid = queries::daily_note_uid_for_date(2, 21, 2026); // "02-21-2026"
    let (eid, selector) = queries::pull_daily_note(&uid);

    let resp = client.pull(eid, &selector).await?;
    let note = DailyNote::from_pull_response(date, uid, &resp.result);

    println!("Title: {}", note.title);
    for block in &note.blocks {
        println!("  - {}", block.string);
    }

    Ok(())
}
```

## Read any page

```rust
let (eid, selector) = queries::pull_page_by_title("My Page");
let resp = client.pull(eid, &selector).await?;
let note = DailyNote::from_pull_response(
    NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(), // dummy date for non-daily pages
    resp.result.get(":block/uid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string(),
    &resp.result,
);
```

## Find linked references

```rust
use roam_sdk::{queries, types};

let query = queries::linked_refs_query("Projects");
let resp = client.query(query, vec![]).await?;
let groups = types::parse_linked_refs(&resp.result, "Projects");

for group in &groups {
    println!("From page: {}", group.page_title);
    for block in &group.blocks {
        println!("  - {}", block.string);
    }
}
```

## Write operations

```rust
use roam_sdk::types::*;

// Create a block
client.write(WriteAction::CreateBlock {
    location: BlockLocation {
        parent_uid: "page-uid".into(),
        order: OrderValue::Position("last".into()),
    },
    block: NewBlock {
        string: "Hello from Rust!".into(),
        uid: None,
        open: None,
    },
}).await?;

// Update a block
client.write(WriteAction::UpdateBlock {
    block: BlockUpdate {
        uid: "block-uid".into(),
        string: "Updated text".into(),
    },
}).await?;

// Delete a block
client.write(WriteAction::DeleteBlock {
    block: BlockRef { uid: "block-uid".into() },
}).await?;

// Move a block
client.write(WriteAction::MoveBlock {
    block: BlockRef { uid: "block-uid".into() },
    location: BlockLocation {
        parent_uid: "new-parent-uid".into(),
        order: OrderValue::Index(0),
    },
}).await?;
```

## Error handling

```rust
use roam_sdk::RoamError;

match client.pull(eid, &selector).await {
    Ok(resp) => println!("{}", resp.result),
    Err(RoamError::Api { status: 429, .. }) => {
        eprintln!("Rate limited — wait and retry");
    }
    Err(RoamError::Api { status: 401, .. }) => {
        eprintln!("Invalid token — check your credentials");
    }
    Err(e) => eprintln!("Error: {}", e),
}
```
