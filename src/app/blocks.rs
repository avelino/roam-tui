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
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("tui-{:x}", nanos)
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
