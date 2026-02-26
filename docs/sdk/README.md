# SDK Overview

`roam-sdk` is a Rust client for the [Roam Research API](https://roamresearch.com). It provides an async HTTP client, typed data structures, and query builders.

## What you get

| Module | What's inside |
|---|---|
| `RoamClient` | Async HTTP client with `pull`, `query`, and `write` methods |
| `types` | `Block`, `DailyNote`, `WriteAction`, `LinkedRefGroup`, and more |
| `queries` | Helpers to build Datalog queries and pull selectors |
| `RoamError` | Typed errors for API, network, and parsing failures |

## Design

- **Async-first** — built on `reqwest` + `tokio`
- **rustls** — no OpenSSL dependency
- **Typed mutations** — `WriteAction` enum covers create, update, delete, and move
- **Raw results where needed** — pull responses return `serde_json::Value` for flexibility with Roam's dynamic schema

## Example

```rust
use roam_sdk::{RoamClient, queries, types};

#[tokio::main]
async fn main() -> roam_sdk::Result<()> {
    let client = RoamClient::new("my-graph", "my-token");

    // Fetch a page
    let (eid, selector) = queries::pull_page_by_title("Projects");
    let resp = client.pull(eid, &selector).await?;
    println!("{}", resp.result);

    // Update a block
    client.write(types::WriteAction::UpdateBlock {
        block: types::BlockUpdate {
            uid: "block-uid".into(),
            string: "New content".into(),
        },
    }).await?;

    Ok(())
}
```
