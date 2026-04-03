use crate::api::client::RoamClient;
use crate::api::queries::pull_page_by_title;
use crate::api::types::{BlockLocation, BlockRef, BlockUpdate, NewBlock, OrderValue, WriteAction};
use crate::error::Result;

use super::parse::{diff_blocks, markdown_to_blocks, BlockDiff, NewBlockTree};
use super::pull::parse_blocks_from_pull;
use super::store::SyncStore;
use super::{SyncConfig, SyncReport};

pub async fn push_sync(
    client: &RoamClient,
    _store: &mut SyncStore,
    config: &SyncConfig,
) -> Result<SyncReport> {
    let mut report = SyncReport::default();

    let pages_dir = config.output_dir.join("pages");
    let daily_dir = config.output_dir.join("daily");

    let mut files: Vec<std::path::PathBuf> = Vec::new();
    for dir in [&pages_dir, &daily_dir] {
        if dir.exists() {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().is_some_and(|e| e == "md") {
                        files.push(path);
                    }
                }
            }
        }
    }

    if files.is_empty() {
        eprintln!("No local markdown files to push.");
        return Ok(report);
    }

    eprintln!("Scanning {} local files for changes...", files.len());

    for file_path in &files {
        let md = match std::fs::read_to_string(file_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        let (title, local_blocks) = markdown_to_blocks(&md);
        if title.is_empty() {
            continue;
        }

        // Pull current state from Roam to get UIDs and page UID
        let (eid, selector) = pull_page_by_title(&title);
        let remote = match client.pull(eid, &selector).await {
            Ok(r) => r,
            Err(_) => continue,
        };

        let page_uid = remote
            .result
            .get(":block/uid")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        if page_uid.is_empty() {
            continue;
        }

        let remote_blocks = parse_blocks_from_pull(&remote.result);
        let diffs = diff_blocks(&remote_blocks, &local_blocks, &page_uid);

        if diffs.is_empty() {
            report.unchanged += 1;
            continue;
        }

        let mut actions_applied = 0;
        for diff in &diffs {
            match diff {
                BlockDiff::Updated { uid, new_content } => {
                    if uid.is_empty() {
                        continue;
                    }
                    let action = WriteAction::UpdateBlock {
                        block: BlockUpdate {
                            uid: uid.clone(),
                            string: new_content.clone(),
                        },
                    };
                    if let Err(e) = client.write(action).await {
                        report.errors.push(format!("Update block {}: {}", uid, e));
                    } else {
                        actions_applied += 1;
                    }
                }
                BlockDiff::Deleted { uid } => {
                    if uid.is_empty() {
                        continue;
                    }
                    let action = WriteAction::DeleteBlock {
                        block: BlockRef { uid: uid.clone() },
                    };
                    if let Err(e) = client.write(action).await {
                        report.errors.push(format!("Delete block {}: {}", uid, e));
                    } else {
                        actions_applied += 1;
                    }
                }
                BlockDiff::Added {
                    parent_uid,
                    order,
                    content,
                    children,
                } => {
                    if parent_uid.is_empty() {
                        continue;
                    }
                    match create_block_tree(client, parent_uid, *order, content, children).await {
                        Ok(count) => actions_applied += count,
                        Err(e) => {
                            report
                                .errors
                                .push(format!("Create block under {}: {}", parent_uid, e));
                        }
                    }
                }
            }
        }

        if actions_applied > 0 {
            eprintln!(
                "  [pushed] {} ({} changes)",
                super::fs::sanitize_filename(&title),
                actions_applied
            );
            report.updated += 1;
        }
    }

    eprintln!(
        "Push complete: {} updated, {} unchanged",
        report.updated, report.unchanged
    );

    Ok(report)
}

/// Create a block and its children recursively.
/// Returns the number of blocks created.
async fn create_block_tree(
    client: &RoamClient,
    parent_uid: &str,
    order: i64,
    content: &str,
    children: &[NewBlockTree],
) -> Result<usize> {
    let uid = crate::api::types::generate_block_uid();
    let action = WriteAction::CreateBlock {
        location: BlockLocation {
            parent_uid: parent_uid.to_string(),
            order: OrderValue::Index(order),
        },
        block: NewBlock {
            string: content.to_string(),
            uid: Some(uid.clone()),
            open: Some(true),
        },
    };
    client.write(action).await?;

    let mut count = 1;
    for (i, child) in children.iter().enumerate() {
        count += Box::pin(create_block_tree(
            client,
            &uid,
            i as i64,
            &child.content,
            &child.children,
        ))
        .await?;
    }

    Ok(count)
}
