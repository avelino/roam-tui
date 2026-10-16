use std::collections::HashMap;

use chrono::{Datelike, NaiveDate};

use crate::api::types::{Block, DailyNote};

use super::state::{linked_ref_section_count, BlockInfo, LinkedRefsState};

pub fn resolve_block_at_index(
    days: &[DailyNote],
    linked_refs: &HashMap<String, LinkedRefsState>,
    index: usize,
) -> Option<BlockInfo> {
    let mut counter = 0;
    for day in days {
        if let Some(info) = resolve_in_blocks(&day.blocks, &day.uid, 0, index, &mut counter) {
            return Some(info);
        }
        // Skip past this day's linked ref section
        if let Some(lr) = linked_refs.get(&day.title) {
            counter += linked_ref_section_count(lr);
        }
    }
    None
}

pub fn find_block_index_by_uid(
    days: &[DailyNote],
    linked_refs: &HashMap<String, LinkedRefsState>,
    uid: &str,
) -> Option<usize> {
    let mut counter = 0;
    for day in days {
        if let Some(idx) = find_index_in_blocks(&day.blocks, uid, &mut counter) {
            return Some(idx);
        }
        // Skip past this day's linked ref section
        if let Some(lr) = linked_refs.get(&day.title) {
            counter += linked_ref_section_count(lr);
        }
    }
    None
}

fn find_index_in_blocks(blocks: &[Block], uid: &str, counter: &mut usize) -> Option<usize> {
    for block in blocks {
        if block.uid == uid {
            return Some(*counter);
        }
        *counter += 1;
        if block.open {
            if let Some(idx) = find_index_in_blocks(&block.children, uid, counter) {
                return Some(idx);
            }
        }
    }
    None
}

fn resolve_in_blocks(
    blocks: &[Block],
    parent_uid: &str,
    depth: usize,
    target: usize,
    counter: &mut usize,
) -> Option<BlockInfo> {
    for block in blocks {
        if *counter == target {
            return Some(BlockInfo {
                block_uid: block.uid.clone(),
                parent_uid: parent_uid.to_string(),
                text: block.string.clone(),
                order: block.order,
                depth,
            });
        }
        *counter += 1;
        if block.open {
            if let Some(info) =
                resolve_in_blocks(&block.children, &block.uid, depth + 1, target, counter)
            {
                return Some(info);
            }
        }
    }
    None
}

// --- Optimistic local tree updates ---

pub fn update_block_text_in_days(days: &mut [DailyNote], uid: &str, new_text: &str) -> bool {
    for day in days.iter_mut() {
        if update_block_text(&mut day.blocks, uid, new_text) {
            return true;
        }
    }
    false
}

fn update_block_text(blocks: &mut [Block], uid: &str, new_text: &str) -> bool {
    for block in blocks.iter_mut() {
        if block.uid == uid {
            block.string = new_text.to_string();
            return true;
        }
        if update_block_text(&mut block.children, uid, new_text) {
            return true;
        }
    }
    false
}

pub fn remove_block_from_days(days: &mut [DailyNote], uid: &str) -> bool {
    for day in days.iter_mut() {
        if remove_block(&mut day.blocks, uid) {
            return true;
        }
    }
    false
}

fn remove_block(blocks: &mut Vec<Block>, uid: &str) -> bool {
    if let Some(pos) = blocks.iter().position(|b| b.uid == uid) {
        blocks.remove(pos);
        return true;
    }
    for block in blocks.iter_mut() {
        if remove_block(&mut block.children, uid) {
            return true;
        }
    }
    false
}

pub fn indent_block_in_days(days: &mut [DailyNote], block_uid: &str) -> Option<(String, i64)> {
    for day in days.iter_mut() {
        if let Some(result) = try_indent_in_list(&mut day.blocks, block_uid) {
            return Some(result);
        }
    }
    None
}

fn try_indent_in_list(blocks: &mut Vec<Block>, block_uid: &str) -> Option<(String, i64)> {
    if let Some(pos) = blocks.iter().position(|b| b.uid == block_uid) {
        if pos == 0 {
            return None;
        }
        let mut block = blocks.remove(pos);
        let prev_sibling = &mut blocks[pos - 1];
        let new_order = prev_sibling
            .children
            .last()
            .map(|b| b.order + 1)
            .unwrap_or(0);
        block.order = new_order;
        let new_parent_uid = prev_sibling.uid.clone();
        prev_sibling.children.push(block);
        return Some((new_parent_uid, new_order));
    }
    for block in blocks.iter_mut() {
        if let Some(result) = try_indent_in_list(&mut block.children, block_uid) {
            return Some(result);
        }
    }
    None
}

pub fn dedent_block_in_days(days: &mut [DailyNote], block_uid: &str) -> Option<(String, i64)> {
    for day in days.iter_mut() {
        if let Some(result) = try_dedent_from_parent_list(&mut day.blocks, &day.uid, block_uid) {
            return Some(result);
        }
    }
    None
}

fn try_dedent_from_parent_list(
    grandparent_children: &mut Vec<Block>,
    grandparent_uid: &str,
    block_uid: &str,
) -> Option<(String, i64)> {
    for parent_idx in 0..grandparent_children.len() {
        if let Some(child_pos) = grandparent_children[parent_idx]
            .children
            .iter()
            .position(|b| b.uid == block_uid)
        {
            let mut block = grandparent_children[parent_idx].children.remove(child_pos);
            let new_order = grandparent_children[parent_idx].order + 1;
            block.order = new_order;
            grandparent_children.insert(parent_idx + 1, block);
            return Some((grandparent_uid.to_string(), new_order));
        }
    }
    for child in grandparent_children.iter_mut() {
        let uid = child.uid.clone();
        if let Some(result) = try_dedent_from_parent_list(&mut child.children, &uid, block_uid) {
            return Some(result);
        }
    }
    None
}

pub fn insert_block_in_days(
    days: &mut [DailyNote],
    parent_uid: &str,
    order: i64,
    new_block: Block,
) -> bool {
    for day in days.iter_mut() {
        if day.uid == parent_uid {
            let pos = day
                .blocks
                .iter()
                .position(|b| b.order >= order)
                .unwrap_or(day.blocks.len());
            day.blocks.insert(pos, new_block);
            return true;
        }
        if insert_block_in_children(&mut day.blocks, parent_uid, order, &new_block) {
            return true;
        }
    }
    false
}

fn insert_block_in_children(
    blocks: &mut [Block],
    parent_uid: &str,
    order: i64,
    new_block: &Block,
) -> bool {
    for block in blocks.iter_mut() {
        if block.uid == parent_uid {
            let pos = block
                .children
                .iter()
                .position(|b| b.order >= order)
                .unwrap_or(block.children.len());
            block.children.insert(pos, new_block.clone());
            return true;
        }
        if insert_block_in_children(&mut block.children, parent_uid, order, new_block) {
            return true;
        }
    }
    false
}

pub fn set_block_open(days: &mut [DailyNote], uid: &str, open: bool) -> bool {
    for day in days.iter_mut() {
        if set_open_recursive(&mut day.blocks, uid, open) {
            return true;
        }
    }
    false
}

fn set_open_recursive(blocks: &mut [Block], uid: &str, open: bool) -> bool {
    for block in blocks.iter_mut() {
        if block.uid == uid {
            block.open = open;
            return true;
        }
        if set_open_recursive(&mut block.children, uid, open) {
            return true;
        }
    }
    false
}

pub fn find_block_in_days(days: &[DailyNote], uid: &str) -> Option<Block> {
    for day in days {
        if let Some(block) = find_block_recursive(&day.blocks, uid) {
            return Some(block);
        }
    }
    None
}

fn find_block_recursive(blocks: &[Block], uid: &str) -> Option<Block> {
    for block in blocks {
        if block.uid == uid {
            return Some(block.clone());
        }
        if let Some(found) = find_block_recursive(&block.children, uid) {
            return Some(found);
        }
    }
    None
}

pub(super) fn find_block_parent_info(days: &[DailyNote], uid: &str) -> Option<(String, i64)> {
    for day in days {
        if let Some(result) = find_parent_info_recursive(&day.blocks, &day.uid, uid) {
            return Some(result);
        }
    }
    None
}

fn find_parent_info_recursive(
    blocks: &[Block],
    parent_uid: &str,
    uid: &str,
) -> Option<(String, i64)> {
    for block in blocks {
        if block.uid == uid {
            return Some((parent_uid.to_string(), block.order));
        }
        if let Some(result) = find_parent_info_recursive(&block.children, &block.uid, uid) {
            return Some(result);
        }
    }
    None
}

/// Move a block one position up among its siblings.
///
/// Swaps the block with the sibling immediately above it (same parent),
/// swapping their `order` fields and their position in the `children` vec.
/// Returns `Some((parent_uid, new_order, sibling_uid))` on success — where
/// `sibling_uid` is the UID of the sibling the block was swapped with.
/// Returns `None` when the block is already the first child of its parent
/// or cannot be found.
pub fn move_block_up_in_days(
    days: &mut [DailyNote],
    block_uid: &str,
) -> Option<(String, i64, String)> {
    for day in days.iter_mut() {
        let day_uid = day.uid.clone();
        if let Some(result) = try_swap_with_prev_sibling(&mut day.blocks, &day_uid, block_uid) {
            return Some(result);
        }
    }
    None
}

fn try_swap_with_prev_sibling(
    blocks: &mut [Block],
    parent_uid: &str,
    block_uid: &str,
) -> Option<(String, i64, String)> {
    if let Some(pos) = blocks.iter().position(|b| b.uid == block_uid) {
        if pos == 0 {
            return None;
        }
        let sibling_uid = blocks[pos - 1].uid.clone();
        let cur_order = blocks[pos].order;
        let prev_order = blocks[pos - 1].order;
        blocks[pos].order = prev_order;
        blocks[pos - 1].order = cur_order;
        blocks.swap(pos - 1, pos);
        return Some((parent_uid.to_string(), prev_order, sibling_uid));
    }
    for block in blocks.iter_mut() {
        let child_parent_uid = block.uid.clone();
        if let Some(result) =
            try_swap_with_prev_sibling(&mut block.children, &child_parent_uid, block_uid)
        {
            return Some(result);
        }
    }
    None
}

/// Move a block one position down among its siblings.
///
/// Swaps the block with the sibling immediately below it (same parent),
/// swapping their `order` fields and their position in the `children` vec.
/// Returns `Some((parent_uid, new_order, sibling_uid))` on success — where
/// `sibling_uid` is the UID of the sibling the block was swapped with.
/// Returns `None` when the block is already the last child of its parent
/// or cannot be found.
pub fn move_block_down_in_days(
    days: &mut [DailyNote],
    block_uid: &str,
) -> Option<(String, i64, String)> {
    for day in days.iter_mut() {
        let day_uid = day.uid.clone();
        if let Some(result) = try_swap_with_next_sibling(&mut day.blocks, &day_uid, block_uid) {
            return Some(result);
        }
    }
    None
}

fn try_swap_with_next_sibling(
    blocks: &mut [Block],
    parent_uid: &str,
    block_uid: &str,
) -> Option<(String, i64, String)> {
    if let Some(pos) = blocks.iter().position(|b| b.uid == block_uid) {
        if pos + 1 >= blocks.len() {
            return None;
        }
        let sibling_uid = blocks[pos + 1].uid.clone();
        let cur_order = blocks[pos].order;
        let next_order = blocks[pos + 1].order;
        blocks[pos].order = next_order;
        blocks[pos + 1].order = cur_order;
        blocks.swap(pos, pos + 1);
        return Some((parent_uid.to_string(), next_order, sibling_uid));
    }
    for block in blocks.iter_mut() {
        let child_parent_uid = block.uid.clone();
        if let Some(result) =
            try_swap_with_next_sibling(&mut block.children, &child_parent_uid, block_uid)
        {
            return Some(result);
        }
    }
    None
}

/// Swap two siblings (same parent) by UID, atomically.
///
/// Swaps both their position in the children vec AND their `order` fields,
/// so the local tree remains consistent (orders sequential, no duplicates).
/// Returns `true` if the swap succeeded, `false` if either uid was not found
/// or they don't share a parent.
pub fn swap_siblings_by_uid_in_days(days: &mut [DailyNote], uid_a: &str, uid_b: &str) -> bool {
    for day in days.iter_mut() {
        if try_swap_siblings_by_uid(&mut day.blocks, uid_a, uid_b) {
            return true;
        }
    }
    false
}

fn try_swap_siblings_by_uid(blocks: &mut [Block], uid_a: &str, uid_b: &str) -> bool {
    let pos_a = blocks.iter().position(|b| b.uid == uid_a);
    let pos_b = blocks.iter().position(|b| b.uid == uid_b);
    if let (Some(a), Some(b)) = (pos_a, pos_b) {
        let order_a = blocks[a].order;
        let order_b = blocks[b].order;
        blocks[a].order = order_b;
        blocks[b].order = order_a;
        blocks.swap(a, b);
        return true;
    }
    for block in blocks.iter_mut() {
        if try_swap_siblings_by_uid(&mut block.children, uid_a, uid_b) {
            return true;
        }
    }
    false
}

pub fn move_block_in_days(
    days: &mut [DailyNote],
    block_uid: &str,
    target_parent_uid: &str,
    target_order: i64,
) -> bool {
    if let Some(block) = find_block_in_days(days, block_uid) {
        remove_block_from_days(days, block_uid);
        let mut moved = block;
        moved.order = target_order;
        insert_block_in_days(days, target_parent_uid, target_order, moved)
    } else {
        false
    }
}

pub(super) fn generate_uid() -> String {
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    const CHARS: &[u8] = b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let count = COUNTER.fetch_add(1, Ordering::Relaxed) as u64;

    // Mix timestamp, counter, and process id for uniqueness
    let seed = nanos
        .wrapping_mul(6364136223846793005)
        .wrapping_add(count ^ (std::process::id() as u64));

    let mut uid = String::with_capacity(9);
    let mut val = seed;
    for _ in 0..9 {
        uid.push(CHARS[(val % 62) as usize] as char);
        val /= 62;
        // Remix to avoid sequential patterns
        val = val.wrapping_mul(2862933555777941757).wrapping_add(nanos);
    }
    uid
}

pub(crate) fn format_roam_daily_title(date: NaiveDate) -> String {
    let month = match date.month() {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "",
    };
    let day = date.day();
    let suffix = match day {
        1 | 21 | 31 => "st",
        2 | 22 => "nd",
        3 | 23 => "rd",
        _ => "th",
    };
    format!("{} {}{}, {}", month, day, suffix, date.year())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_uid_is_9_chars() {
        let uid = generate_uid();
        assert_eq!(uid.len(), 9, "UID should be 9 chars, got: {}", uid);
    }

    #[test]
    fn generate_uid_is_alphanumeric() {
        let uid = generate_uid();
        assert!(
            uid.chars().all(|c| c.is_ascii_alphanumeric()),
            "UID should be alphanumeric, got: {}",
            uid
        );
    }

    #[test]
    fn generate_uid_is_unique() {
        let mut uids: std::collections::HashSet<String> = std::collections::HashSet::new();
        for _ in 0..1000 {
            let uid = generate_uid();
            assert!(uids.insert(uid.clone()), "Duplicate UID: {}", uid);
        }
    }
}
