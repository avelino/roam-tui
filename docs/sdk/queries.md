# Queries

Helper functions for building Roam API queries. Available at `roam_sdk::queries`.

## Daily notes

### `daily_note_uid_for_date`

Generate the UID for a daily note from a date.

```rust
pub fn daily_note_uid_for_date(month: u32, day: u32, year: i32) -> String
```

```rust
let uid = queries::daily_note_uid_for_date(2, 21, 2026);
assert_eq!(uid, "02-21-2026");
```

### `pull_daily_note`

Build a pull request for a daily note by its UID.

```rust
pub fn pull_daily_note(uid: &str) -> (serde_json::Value, String)
```

Returns `(eid, selector)` ready to pass to `client.pull()`.

```rust
let uid = queries::daily_note_uid_for_date(2, 21, 2026);
let (eid, selector) = queries::pull_daily_note(&uid);
let resp = client.pull(eid, &selector).await?;
```

The selector includes: `:block/uid`, `:node/title`, `:block/string`, `:block/children` (recursive), `:block/order`, `:block/open`, `:block/refs`.

## Pages

### `pull_page_by_title`

Build a pull request for any page by title.

```rust
pub fn pull_page_by_title(title: &str) -> (serde_json::Value, String)
```

Uses `[:node/title "..."]` as the entity lookup. Same selector as daily notes.

```rust
let (eid, selector) = queries::pull_page_by_title("Projects");
let resp = client.pull(eid, &selector).await?;
```

## Linked references

### `linked_refs_query`

Build a Datalog query that finds all blocks referencing a page.

```rust
pub fn linked_refs_query(page_title: &str) -> String
```

Returns a query string for `client.query()`. Double quotes in the title are escaped.

```rust
let query = queries::linked_refs_query("My Project");
let resp = client.query(query, vec![]).await?;
```

The query returns rows of `[uid, block_string, source_page_title]`. Parse the results with `types::parse_linked_refs()`:

```rust
let groups = types::parse_linked_refs(&resp.result, "My Project");
```

## Writing your own queries

You can pass any Datalog query string directly to `client.query()`:

```rust
// Find all blocks containing "TODO"
let query = r#"[:find ?uid ?s
                :where [?b :block/string ?s]
                       [?b :block/uid ?uid]
                       [(clojure.string/includes? ?s "TODO")]]"#;

let resp = client.query(query.into(), vec![]).await?;
```

Query format notes:
- Use `:find` with simple variable bindings (not pull expressions)
- `:where` clauses use Datomic-style pattern matching
- `args` must always be provided (use `vec![]` for no arguments)
- Results are `Vec<Vec<serde_json::Value>>` with values in `:find` variable order
