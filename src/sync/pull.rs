use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tokio::sync::Semaphore;

use crate::api::client::RoamClient;
use crate::api::queries::{all_page_titles_query, pages_modified_since_query, pull_page_by_title};
use crate::api::types::Block;
use crate::error::Result;
use crate::export::blocks_to_markdown;

use super::fs;
use super::store::SyncStore;
use super::{SyncConfig, SyncReport};

pub async fn pull_sync(
    client: &RoamClient,
    store: &mut SyncStore,
    config: &SyncConfig,
) -> Result<SyncReport> {
    let mut report = SyncReport::default();

    if !config.dry_run {
        fs::ensure_dirs(&config.output_dir).map_err(|e| {
            crate::error::RoamError::Generic(format!("Failed to create dirs: {}", e))
        })?;
    }

    // 1. Get all page titles + UIDs (1 API call)
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

    // 2. Query recently modified pages (1 API call)
    //    Uses :edit/time to find pages with blocks edited since last sync.
    // Capture timestamp BEFORE sync starts — any edit after this will be caught next time
    store.set_next_sync_ms(chrono::Utc::now().timestamp_millis());
    let last_sync_ms = store.last_sync_ms();
    let modified_uids: std::collections::HashSet<String> = if last_sync_ms > 0 {
        eprintln!("Querying pages modified since last sync...");
        match client
            .query(pages_modified_since_query(last_sync_ms), vec![])
            .await
        {
            Ok(resp) => resp
                .result
                .iter()
                .filter_map(|row| row.get(1)?.as_str().map(String::from))
                .collect(),
            Err(e) => {
                eprintln!(
                    "Warning: modified pages query failed ({}), pulling new only",
                    e
                );
                std::collections::HashSet::new()
            }
        }
    } else {
        std::collections::HashSet::new()
    };

    if !modified_uids.is_empty() {
        eprintln!("{} pages modified since last sync", modified_uids.len());
    }

    // 3. Partition: modified (update) + new (create) + unchanged (skip)
    let mut to_update: Vec<(String, String)> = Vec::new();
    let mut daily_to_pull: Vec<(String, String)> = Vec::new();
    let mut pages_to_pull: Vec<(String, String)> = Vec::new();

    for (title, uid) in &pages {
        if !store.has_page(uid) {
            // New page
            if is_daily_note_uid(uid) {
                daily_to_pull.push((title.clone(), uid.clone()));
            } else {
                pages_to_pull.push((title.clone(), uid.clone()));
            }
        } else if modified_uids.contains(uid) {
            // Existing page with recent edits
            to_update.push((title.clone(), uid.clone()));
        } else {
            report.unchanged += 1;
        }
    }

    // Daily notes: most recent first
    daily_to_pull.sort_by(|a, b| b.1.cmp(&a.1));

    let total = pages.len();
    let to_pull = to_update.len() + daily_to_pull.len() + pages_to_pull.len();

    eprintln!(
        "Found {} pages: {} to sync ({} updated, {} daily new, {} pages new), {} unchanged",
        total,
        to_pull,
        to_update.len(),
        daily_to_pull.len(),
        pages_to_pull.len(),
        report.unchanged
    );

    if to_pull == 0 {
        // Still flush to save sync timestamp
        if !config.dry_run {
            let _ = store.flush();
        }
        eprintln!("Everything up to date.");
        eprintln!(
            "API calls: {} (list{})",
            if last_sync_ms > 0 { 2 } else { 1 },
            if last_sync_ms > 0 {
                " + modified"
            } else {
                " only"
            }
        );
        return Ok(report);
    }

    // 3. Pull daily notes first, then pages — Ctrl+C triggers graceful flush
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancelled_clone = cancelled.clone();
    tokio::spawn(async move {
        let _ = tokio::signal::ctrl_c().await;
        eprintln!("\nCtrl+C received, finishing current page and saving...");
        cancelled_clone.store(true, Ordering::Relaxed);
    });

    let semaphore = Arc::new(Semaphore::new(config.concurrency));

    // Updated pages first (user just edited these)
    if !to_update.is_empty() {
        eprintln!("Updating modified pages...");
    }
    for (title, uid) in &to_update {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let _permit = semaphore.acquire().await.unwrap();
        let kind = if is_daily_note_uid(uid) {
            "daily"
        } else {
            "pages"
        };
        match pull_and_store_page(client, store, config, title, uid).await {
            Ok(block_count) => {
                eprintln!(
                    "  [updated] {}/{}.md ({} blocks)",
                    kind,
                    fs::sanitize_filename(title),
                    block_count
                );
                report.updated += 1;
            }
            Err(e) => {
                let msg = format!("Error updating '{}': {}", title, e);
                eprintln!("  [error] {}", msg);
                report.errors.push(msg);
            }
        }
    }

    // New daily notes
    if !cancelled.load(Ordering::Relaxed) && !daily_to_pull.is_empty() {
        eprintln!("Syncing new daily notes...");
    }
    for (title, uid) in &daily_to_pull {
        if cancelled.load(Ordering::Relaxed) {
            break;
        }
        let _permit = semaphore.acquire().await.unwrap();
        sync_one(client, store, config, title, uid, "daily", &mut report).await;
    }

    // New pages
    if !cancelled.load(Ordering::Relaxed) && !pages_to_pull.is_empty() {
        eprintln!("Syncing new pages...");
    }
    if !cancelled.load(Ordering::Relaxed) {
        for (title, uid) in &pages_to_pull {
            if cancelled.load(Ordering::Relaxed) {
                break;
            }
            let _permit = semaphore.acquire().await.unwrap();
            sync_one(client, store, config, title, uid, "pages", &mut report).await;
        }
    }

    // 4. Flush — always runs, even on Ctrl+C (saves progress)
    if !config.dry_run {
        if let Err(e) = store.flush() {
            eprintln!("Warning: ChronDB flush failed ({}), pages still on disk", e);
        }
    }

    if cancelled.load(Ordering::Relaxed) {
        eprintln!("Interrupted — progress saved. Run again to continue.");
    }

    eprintln!("{}", report);
    eprintln!(
        "API calls: 1 (list) + {} (pulls) = {} total",
        report.created + report.errors.len(),
        1 + report.created + report.errors.len()
    );

    Ok(report)
}

/// Daily note UIDs follow the pattern MM-DD-YYYY (e.g. "03-30-2026").
fn is_daily_note_uid(uid: &str) -> bool {
    let parts: Vec<&str> = uid.split('-').collect();
    if parts.len() != 3 {
        return false;
    }
    parts[0].len() == 2
        && parts[1].len() == 2
        && parts[2].len() == 4
        && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit()))
}

async fn sync_one(
    client: &RoamClient,
    store: &mut SyncStore,
    config: &SyncConfig,
    title: &str,
    uid: &str,
    kind: &str,
    report: &mut SyncReport,
) {
    match pull_and_store_page(client, store, config, title, uid).await {
        Ok(block_count) => {
            eprintln!(
                "  [new] {}/{}.md ({} blocks)",
                kind,
                fs::sanitize_filename(title),
                block_count
            );
            report.created += 1;
        }
        Err(e) => {
            let msg = format!("Error syncing '{}': {}", title, e);
            eprintln!("  [error] {}", msg);
            report.errors.push(msg);
        }
    }
}

async fn pull_and_store_page(
    client: &RoamClient,
    store: &mut SyncStore,
    config: &SyncConfig,
    title: &str,
    uid: &str,
) -> Result<usize> {
    let (eid, selector) = pull_page_by_title(title);
    let response = client.pull(eid, &selector).await?;

    let blocks = parse_blocks_from_pull(&response.result);
    let block_count = count_blocks(&blocks);

    if !config.dry_run {
        let md = blocks_to_markdown(title, &blocks);

        let (path, file_path) = if is_daily_note_uid(uid) {
            let p = fs::daily_path_from_uid(&config.output_dir, uid);
            let rel = format!("daily/{}.md", uid);
            (p, rel)
        } else {
            let p = fs::page_path(&config.output_dir, title);
            let rel = format!("pages/{}.md", fs::sanitize_filename(title));
            (p, rel)
        };

        std::fs::write(&path, md)
            .map_err(|e| crate::error::RoamError::Generic(format!("Write error: {}", e)))?;

        store.put_page(uid, title, &file_path, block_count);
    }

    Ok(block_count)
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
