# Client

`RoamClient` is the async HTTP client for the Roam Research API.

## Creating a client

```rust
use roam_sdk::RoamClient;

let client = RoamClient::new("graph-name", "api-token");
```

The client is `Clone` — you can share it across async tasks.

## Methods

### `pull`

Fetch an entity by ID or lookup reference using a [pull expression](https://www.roamresearch.com/#/app/developer-documentation/page/eb8OVhaFC).

```rust
pub async fn pull(
    &self,
    eid: serde_json::Value,
    selector: &str,
) -> Result<PullResponse>
```

**Parameters:**

- `eid` — entity identifier. Either a direct eid or a lookup ref as an EDN string:
  - `json!("[:block/uid \"02-21-2026\"]")` — lookup by block UID
  - `json!("[:node/title \"My Page\"]")` — lookup by page title
- `selector` — EDN pull expression selecting which attributes to return

**Returns:** `PullResponse { result: serde_json::Value }`

**Example:**

```rust
use serde_json::json;

let eid = json!("[:node/title \"Projects\"]");
let selector = "[:block/uid :node/title :block/string {:block/children ...}]";

let resp = client.pull(eid, selector).await?;
let title = resp.result.get(":node/title").and_then(|v| v.as_str());
```

### `query`

Run a [Datalog query](https://www.roamresearch.com/#/app/developer-documentation/page/eb8OVhaFC) against the graph.

```rust
pub async fn query(
    &self,
    query: String,
    args: Vec<serde_json::Value>,
) -> Result<QueryResponse>
```

**Parameters:**

- `query` — Datalog query string with `:find` and `:where` clauses
- `args` — arguments for `:in` clause bindings (pass `vec![]` if none)

**Returns:** `QueryResponse { result: Vec<Vec<serde_json::Value>> }`

Each inner `Vec` is one result row, with values matching the `:find` variables.

**Example:**

```rust
let query = r#"[:find ?uid ?s
                :where [?b :block/string ?s]
                       [?b :block/uid ?uid]
                       [(clojure.string/includes? ?s "TODO")]]"#;

let resp = client.query(query.into(), vec![]).await?;
for row in &resp.result {
    let uid = row[0].as_str().unwrap_or("");
    let text = row[1].as_str().unwrap_or("");
    println!("{}: {}", uid, text);
}
```

### `write`

Execute a write operation (create, update, delete, or move a block).

```rust
pub async fn write(&self, action: WriteAction) -> Result<()>
```

**Parameters:**

- `action` — a `WriteAction` variant describing the mutation

**Example:**

```rust
use roam_sdk::types::*;

client.write(WriteAction::UpdateBlock {
    block: BlockUpdate {
        uid: "abc123".into(),
        string: "Updated content".into(),
    },
}).await?;
```

See [Types](types.md) for all `WriteAction` variants.

## Authentication

The client sends the API token as a Bearer token in the `X-Authorization` header on every request. All communication goes over HTTPS via rustls (no OpenSSL needed).

## Base URL

Requests go to `https://api.roamresearch.com/api/graph/{graph_name}/`:

| Endpoint | Method |
|---|---|
| `/pull` | `pull()` |
| `/q` | `query()` |
| `/write` | `write()` |
