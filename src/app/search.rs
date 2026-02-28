use std::collections::HashMap;

use crate::api::types::{Block, DailyNote};
use crate::edit_buffer::EditBuffer;

pub(super) const AUTOCOMPLETE_LIMIT: usize = 20;
pub(super) const SEARCH_LIMIT: usize = 50;
pub(super) const QUICK_SWITCHER_LIMIT: usize = 50;

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

pub fn filter_page_titles(
    titles: &[(String, String)],
    query: &str,
    limit: usize,
) -> Vec<(String, String)> {
    if query.is_empty() {
        return titles.iter().take(limit).cloned().collect();
    }
    let query_lower = query.to_lowercase();
    let mut prefix_matches: Vec<(String, String)> = Vec::new();
    let mut contains_matches: Vec<(String, String)> = Vec::new();
    for (title, uid) in titles {
        let title_lower = title.to_lowercase();
        if title_lower.starts_with(&query_lower) {
            prefix_matches.push((title.clone(), uid.clone()));
        } else if title_lower.contains(&query_lower) {
            contains_matches.push((title.clone(), uid.clone()));
        }
        if prefix_matches.len() + contains_matches.len() >= limit {
            break;
        }
    }
    prefix_matches.extend(contains_matches);
    prefix_matches.truncate(limit);
    prefix_matches
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
