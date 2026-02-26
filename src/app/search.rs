use std::collections::HashMap;

use crate::api::types::{Block, DailyNote};
use crate::edit_buffer::EditBuffer;

pub(super) const AUTOCOMPLETE_LIMIT: usize = 20;
pub(super) const SEARCH_LIMIT: usize = 50;

pub fn detect_block_ref_trigger(buffer: &EditBuffer) -> bool {
    let c = buffer.cursor;
    c >= 2
        && c + 1 < buffer.chars.len()
        && buffer.chars[c - 2] == '('
        && buffer.chars[c - 1] == '('
        && buffer.chars[c] == ')'
        && buffer.chars[c + 1] == ')'
}

pub fn filter_blocks(
    days: &[DailyNote],
    cache: &HashMap<String, String>,
    query: &str,
    limit: usize,
) -> Vec<(String, String)> {
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();
    for day in days {
        collect_matching_blocks(&day.blocks, &query_lower, &mut results, limit);
        if results.len() >= limit {
            break;
        }
    }
    // Also search resolved block refs from cache
    if results.len() < limit {
        for (uid, text) in cache {
            if results.len() >= limit {
                break;
            }
            if !text.is_empty()
                && (query_lower.is_empty() || text.to_lowercase().contains(&query_lower))
            {
                // Avoid duplicates (block already found from days)
                if !results.iter().any(|(u, _)| u == uid) {
                    results.push((uid.clone(), text.clone()));
                }
            }
        }
    }
    results.truncate(limit);
    results
}

fn collect_matching_blocks(
    blocks: &[Block],
    query: &str,
    results: &mut Vec<(String, String)>,
    limit: usize,
) {
    for block in blocks {
        if results.len() >= limit {
            return;
        }
        if !block.string.is_empty()
            && (query.is_empty() || block.string.to_lowercase().contains(query))
        {
            results.push((block.uid.clone(), block.string.clone()));
        }
        collect_matching_blocks(&block.children, query, results, limit);
    }
}
