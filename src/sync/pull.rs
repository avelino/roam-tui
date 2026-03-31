use std::collections::HashMap;
use std::sync::Arc;

use serde_json::json;
use tokio::sync::Semaphore;

use crate::api::client::RoamClient;
use crate::api::queries::{
    all_blocks_fingerprint_query, all_page_titles_query, pull_page_by_title,
};
use crate::api::types::Block;
use crate::error::Result;
use crate::export::blocks_to_markdown;

use super::fs;
use super::store::SyncStore;
use super::{SyncConfig, SyncReport};

pub async fn pull_sync(
    client: &RoamClient,
    store: &SyncStore,
    config: &SyncConfig,
) -> Result<SyncReport> {
    let mut report = SyncReport::default();

    if !config.dry_run {
        fs::ensure_dirs(&config.output_dir).map_err(|e| {
            crate::error::RoamError::Generic(format!("Failed to create dirs: {}", e))
        })?;
    }

    // 1. Get all page titles (1 API call)
    eprintln!("Querying page list...");
    let page_response = client.query(all_page_titles_query(), vec![]).await?;

    let mut pages: Vec<(String, String)> = page_response
        .result
        .iter()
        .filter_map(|row| {
            let title = row.first()?.as_str()?.to_string();
            let uid = row.get(1)?.as_str()?.to_string();
            Some((title, uid))
        })
        .collect();

    if let Some(ref filter) = config.filter {
        pages.retain(|(title, _)| title.starts_with(filter.as_str()));
    }

    pages.sort_by(|a, b| a.0.cmp(&b.0));

    // 2. Get lightweight fingerprint of all blocks (1 API call)
    //    Returns (page_uid, block_uid, block_string) for every block in the graph.
    //    We compare each block against ChronDB to find which pages actually changed.
    eprintln!("Querying block fingerprints...");
    let fingerprint_response = client.query(all_blocks_fingerprint_query(), vec![]).await?;

    // Group blocks by page UID
    let mut remote_blocks: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for row in &fingerprint_response.result {
        if let (Some(page_uid), Some(block_uid), Some(block_string)) = (
            row.first().and_then(|v| v.as_str()),
            row.get(1).and_then(|v| v.as_str()),
            row.get(2).and_then(|v| v.as_str()),
        ) {
            remote_blocks
                .entry(page_uid.to_string())
                .or_default()
                .push((block_uid.to_string(), block_string.to_string()));
        }
    }

    // 3. Determine which pages need a full pull
    let mut pages_to_pull: Vec<(String, String)> = Vec::new();
    let mut uid_to_title: HashMap<String, String> = HashMap::new();

    for (title, uid) in &pages {
        uid_to_title.insert(uid.clone(), title.clone());

        let needs_pull = match remote_blocks.get(uid) {
            None => {
                // Page with no blocks — check if we have it stored
                store.get_page(uid)?.is_none()
            }
            Some(blocks) => {
                // Compare each block against ChronDB
                let mut changed = false;
                for (block_uid, block_string) in blocks {
                    match store.get_block(block_uid)? {
                        None => {
                            changed = true;
                            break;
                        }
                        Some(stored) => {
                            if stored.get("string").and_then(|v| v.as_str())
                                != Some(block_string.as_str())
                            {
                                changed = true;
                                break;
                            }
                        }
                    }
                }
                if !changed {
                    // Check for deleted blocks: stored blocks not in remote
                    if let Ok(stored_page) = store.get_page(uid) {
                        if stored_page.is_none() {
                            changed = true;
                        }
                    }
                }
                changed
            }
        };

        if needs_pull {
            pages_to_pull.push((title.clone(), uid.clone()));
        } else {
            report.unchanged += 1;
        }
    }

    let total = pages.len();
    let to_pull = pages_to_pull.len();
    let skipped = total - to_pull;

    eprintln!(
        "Found {} pages: {} need sync, {} unchanged (skipped)",
        total, to_pull, skipped
    );

    // 4. Pull only changed pages (N API calls, where N = changed pages only)
    let semaphore = Arc::new(Semaphore::new(config.concurrency));

    for (title, uid) in &pages_to_pull {
        let _permit = semaphore.acquire().await.unwrap();

        match pull_and_store_page(client, store, config, title, uid).await {
            Ok(PullResult::Created(block_count)) => {
                eprintln!(
                    "  [new]     pages/{}.md ({} blocks)",
                    fs::sanitize_filename(title),
                    block_count
                );
                report.created += 1;
            }
            Ok(PullResult::Updated(changed)) => {
                eprintln!(
                    "  [updated] pages/{}.md ({} blocks changed)",
                    fs::sanitize_filename(title),
                    changed
                );
                report.updated += 1;
            }
            Err(e) => {
                let msg = format!("Error syncing '{}': {}", title, e);
                eprintln!("  [error]   {}", msg);
                report.errors.push(msg);
            }
        }
    }

    // 5. Update manifest
    if !config.dry_run {
        let manifest = json!({
            "version": 1,
            "last_sync": chrono::Utc::now().to_rfc3339(),
            "page_count": total,
            "api_calls": 2 + to_pull,
        });
        let _ = store.put_manifest(&manifest);
    }

    eprintln!("{}", report);
    eprintln!(
        "API calls: 2 (list + fingerprint) + {} (changed pages) = {} total",
        to_pull,
        2 + to_pull
    );

    Ok(report)
}

enum PullResult {
    Created(usize),
    Updated(usize),
}

async fn pull_and_store_page(
    client: &RoamClient,
    store: &SyncStore,
    config: &SyncConfig,
    title: &str,
    uid: &str,
) -> Result<PullResult> {
    let (eid, selector) = pull_page_by_title(title);
    let response = client.pull(eid, &selector).await?;

    let blocks = parse_blocks_from_pull(&response.result);
    let is_new = store.get_page(uid)?.is_none();

    let mut changed_count = 0;
    store_blocks_recursive(store, config, uid, &blocks, &mut changed_count)?;

    // Store page metadata
    if !config.dry_run {
        let page_meta = json!({
            "title": title,
            "uid": uid,
            "file_path": format!("pages/{}.md", fs::sanitize_filename(title)),
            "block_count": count_blocks(&blocks),
        });
        store.put_page(uid, &page_meta)?;

        // Write markdown file
        let md = blocks_to_markdown(title, &blocks);
        let path = fs::page_path(&config.output_dir, title);
        std::fs::write(&path, md)
            .map_err(|e| crate::error::RoamError::Generic(format!("Write error: {}", e)))?;
    }

    if is_new {
        Ok(PullResult::Created(count_blocks(&blocks)))
    } else {
        Ok(PullResult::Updated(changed_count))
    }
}

fn store_blocks_recursive(
    store: &SyncStore,
    config: &SyncConfig,
    page_uid: &str,
    blocks: &[Block],
    changed_count: &mut usize,
) -> Result<()> {
    for block in blocks {
        let block_doc = json!({
            "string": block.string,
            "order": block.order,
            "page": page_uid,
            "open": block.open,
        });

        if !config.dry_run {
            store.put_block(&block.uid, &block_doc)?;
        }
        *changed_count += 1;

        store_blocks_recursive(store, config, page_uid, &block.children, changed_count)?;
    }
    Ok(())
}

fn parse_blocks_from_pull(result: &serde_json::Value) -> Vec<Block> {
    result
        .get(":block/children")
        .and_then(|v| v.as_array())
        .map(|arr| {
            let mut blocks: Vec<Block> = arr.iter().map(parse_block).collect();
            blocks.sort_by_key(|b| b.order);
            blocks
        })
        .unwrap_or_default()
}

fn parse_block(val: &serde_json::Value) -> Block {
    let uid = val
        .get(":block/uid")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let string = val
        .get(":block/string")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let order = val
        .get(":block/order")
        .and_then(|v| v.as_i64())
        .unwrap_or(0);
    let open = val
        .get(":block/open")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    let children = val
        .get(":block/children")
        .and_then(|v| v.as_array())
        .map(|arr| {
            let mut children: Vec<Block> = arr.iter().map(parse_block).collect();
            children.sort_by_key(|b| b.order);
            children
        })
        .unwrap_or_default();

    Block {
        uid,
        string,
        order,
        children,
        open,
        refs: vec![],
    }
}

fn count_blocks(blocks: &[Block]) -> usize {
    blocks.iter().map(|b| 1 + count_blocks(&b.children)).sum()
}
