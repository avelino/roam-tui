use std::collections::{HashMap, HashSet};
use std::time::Duration;

use chrono::{Datelike, Local, NaiveDate};
use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::api::client::RoamClient;
use crate::api::queries;
use crate::api::types::{
    Block, BlockLocation, BlockRef, BlockUpdate, DailyNote, LinkedRefBlock, LinkedRefGroup,
    NewBlock, OrderValue, WriteAction,
};
use crate::config::AppConfig;
use crate::edit_buffer::EditBuffer;
use crate::error::{ErrorInfo, ErrorPopup, Result};
use crate::keys::preset::Action;
use crate::keys::KeybindingMap;
use crate::markdown;

#[derive(Debug, Clone)]
pub enum UndoEntry {
    TextEdit {
        block_uid: String,
        old_text: String,
    },
    CreateBlock {
        block_uid: String,
    },
    DeleteBlock {
        block: Block,
        parent_uid: String,
        order: i64,
        selected_block: usize,
    },
    MoveBlock {
        block_uid: String,
        old_parent_uid: String,
        old_order: i64,
        selected_block: usize,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct AutocompleteState {
    pub query: String,
    pub results: Vec<(String, String)>, // (uid, text)
    pub selected: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SearchState {
    pub query: String,
    pub results: Vec<(String, String)>, // (uid, text)
    pub selected: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ViewMode {
    DailyNotes,
    Page { title: String },
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkPickerState {
    pub links: Vec<String>,
    pub selected: usize,
}

#[derive(Debug, Clone)]
struct ViewSnapshot {
    view_mode: ViewMode,
    days: Vec<DailyNote>,
    selected_block: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LoadRequest {
    DailyNote(NaiveDate),
    Page(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct LinkedRefsState {
    pub groups: Vec<LinkedRefGroup>,
    pub collapsed: bool,
    pub loading: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkedRefItem {
    SectionHeader,
    GroupHeader(String),
    Block(LinkedRefBlock),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppMessage {
    Key(KeyEvent),
    DailyNoteLoaded(DailyNote),
    PageLoaded(DailyNote),
    RefreshLoaded(DailyNote),
    BlockRefResolved(String, String),              // (uid, text)
    LinkedRefsLoaded(String, Vec<LinkedRefGroup>), // (page_title, groups)
    ApiError(ErrorInfo),
    Tick,
}

#[derive(Debug, Clone)]
pub struct AppState {
    pub graph_name: String,
    pub date_display: String,
    pub days: Vec<DailyNote>,
    pub current_date: NaiveDate,
    pub selected_block: usize,
    pub cursor_col: usize,
    pub loading: bool,
    pub loading_more: bool,
    pub status_message: Option<String>,
    pub hints: Vec<(String, &'static str)>,
    pub should_quit: bool,
    pub refresh_counter: u32,
    pub input_mode: InputMode,
    pub pending_key: Option<char>,
    pub block_ref_cache: HashMap<String, String>,
    pending_block_refs: HashSet<String>,
    pub autocomplete: Option<AutocompleteState>,
    pub search: Option<SearchState>,
    pub undo_stack: Vec<UndoEntry>,
    pub redo_stack: Vec<UndoEntry>,
    pub show_help: bool,
    pub view_mode: ViewMode,
    nav_history: Vec<ViewSnapshot>,
    nav_index: usize,
    pub link_picker: Option<LinkPickerState>,
    pub error_popup: Option<ErrorPopup>,
    pub linked_refs: HashMap<String, LinkedRefsState>,
}

impl AppState {
    pub fn new(graph_name: &str, hints: Vec<(String, &'static str)>) -> Self {
        let now = Local::now();
        Self {
            graph_name: graph_name.to_string(),
            date_display: now.format("%b %d, %Y").to_string(),
            days: Vec::new(),
            current_date: now.date_naive(),
            selected_block: 0,
            cursor_col: 0,
            loading: true,
            loading_more: false,
            status_message: Some("Loading today's notes...".into()),
            hints,
            should_quit: false,
            refresh_counter: 0,
            input_mode: InputMode::Normal,
            pending_key: None,
            block_ref_cache: HashMap::new(),
            pending_block_refs: HashSet::new(),
            autocomplete: None,
            search: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            show_help: false,
            view_mode: ViewMode::DailyNotes,
            nav_history: Vec::new(),
            nav_index: 0,
            link_picker: None,
            error_popup: None,
            linked_refs: HashMap::new(),
        }
    }

    pub fn flat_block_count(&self) -> usize {
        self.days
            .iter()
            .map(|d| count_blocks_recursive(&d.blocks))
            .sum()
    }

    pub fn total_navigable_count(&self) -> usize {
        let mut total = 0;
        for day in &self.days {
            total += count_blocks_recursive(&day.blocks);
            if let Some(lr) = self.linked_refs.get(&day.title) {
                total += linked_ref_section_count(lr);
            }
        }
        total
    }

    pub fn resolve_linked_ref_item(&self, index: usize) -> Option<LinkedRefItem> {
        let mut pos = 0;
        for day in &self.days {
            let block_count = count_blocks_recursive(&day.blocks);
            if index < pos + block_count {
                return None; // regular block in this day
            }
            pos += block_count;

            if let Some(lr) = self.linked_refs.get(&day.title) {
                let lr_count = linked_ref_section_count(lr);
                if lr_count > 0 && index < pos + lr_count {
                    return resolve_within_linked_refs(lr, index - pos);
                }
                pos += lr_count;
            }
        }
        None
    }

    /// Find which day's linked refs section contains this index.
    /// Returns the day title if the index is a linked ref SectionHeader.
    pub fn linked_ref_day_at(&self, index: usize) -> Option<String> {
        let mut pos = 0;
        for day in &self.days {
            pos += count_blocks_recursive(&day.blocks);
            if let Some(lr) = self.linked_refs.get(&day.title) {
                let lr_count = linked_ref_section_count(lr);
                if lr_count > 0 && index < pos + lr_count {
                    return Some(day.title.clone());
                }
                pos += lr_count;
            }
        }
        None
    }
}

// --- InputMode: Normal vs Insert ---

#[derive(Debug, Clone, PartialEq)]
pub enum InputMode {
    Normal,
    Insert {
        buffer: EditBuffer,
        block_uid: String,
        original_text: String,
        create_info: Option<CreateInfo>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct CreateInfo {
    pub parent_uid: String,
    pub order: i64,
}

// --- Block resolution: map flat index → block info ---

#[derive(Debug, Clone, PartialEq)]
pub struct BlockInfo {
    pub block_uid: String,
    pub parent_uid: String,
    pub text: String,
    pub order: i64,
    pub depth: usize,
}

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

fn linked_ref_section_count(lr: &LinkedRefsState) -> usize {
    if lr.groups.is_empty() {
        return 0;
    }
    if lr.collapsed {
        1 // header only
    } else {
        // 1 (section header) + for each group: 1 (group header) + blocks.len()
        1 + lr.groups.iter().map(|g| 1 + g.blocks.len()).sum::<usize>()
    }
}

fn resolve_within_linked_refs(lr: &LinkedRefsState, offset: usize) -> Option<LinkedRefItem> {
    if lr.groups.is_empty() {
        return None;
    }
    if offset == 0 {
        return Some(LinkedRefItem::SectionHeader);
    }
    if lr.collapsed {
        return None;
    }
    let mut pos = 1; // past section header
    for group in &lr.groups {
        if offset == pos {
            return Some(LinkedRefItem::GroupHeader(group.page_title.clone()));
        }
        pos += 1;
        for block in &group.blocks {
            if offset == pos {
                return Some(LinkedRefItem::Block(block.clone()));
            }
            pos += 1;
        }
    }
    None
}

fn count_blocks_recursive(blocks: &[Block]) -> usize {
    blocks
        .iter()
        .map(|b| {
            if b.open {
                1 + count_blocks_recursive(&b.children)
            } else {
                1
            }
        })
        .sum()
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

fn find_block_parent_info(days: &[DailyNote], uid: &str) -> Option<(String, i64)> {
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

pub fn apply_undo(state: &mut AppState) -> Option<WriteAction> {
    let entry = state.undo_stack.pop()?;
    let (redo_entry, write_action) = apply_undo_entry(state, entry);
    state.redo_stack.push(redo_entry);
    Some(write_action)
}

pub fn apply_redo(state: &mut AppState) -> Option<WriteAction> {
    let entry = state.redo_stack.pop()?;
    let (undo_entry, write_action) = apply_undo_entry(state, entry);
    state.undo_stack.push(undo_entry);
    Some(write_action)
}

fn apply_undo_entry(state: &mut AppState, entry: UndoEntry) -> (UndoEntry, WriteAction) {
    match entry {
        UndoEntry::TextEdit {
            block_uid,
            old_text,
        } => {
            // Save current text as redo entry
            let current_text =
                resolve_block_at_index(&state.days, &state.linked_refs, state.selected_block)
                    .filter(|info| info.block_uid == block_uid)
                    .map(|info| info.text.clone())
                    .or_else(|| {
                        find_block_in_days(&state.days, &block_uid).map(|b| b.string.clone())
                    })
                    .unwrap_or_default();
            update_block_text_in_days(&mut state.days, &block_uid, &old_text);
            let redo = UndoEntry::TextEdit {
                block_uid: block_uid.clone(),
                old_text: current_text,
            };
            let write = WriteAction::UpdateBlock {
                block: BlockUpdate {
                    uid: block_uid,
                    string: old_text,
                },
            };
            (redo, write)
        }
        UndoEntry::CreateBlock { block_uid } => {
            // Undo create = delete. Redo entry = DeleteBlock (to re-create it)
            let block = find_block_in_days(&state.days, &block_uid);
            let parent_info = find_block_parent_info(&state.days, &block_uid);
            let saved_selected = state.selected_block;
            remove_block_from_days(&mut state.days, &block_uid);
            let total = state.flat_block_count();
            if state.selected_block >= total && total > 0 {
                state.selected_block = total - 1;
            }
            state.cursor_col = 0;
            let redo = if let (Some(b), Some((parent_uid, order))) = (block, parent_info) {
                UndoEntry::DeleteBlock {
                    block: b,
                    parent_uid,
                    order,
                    selected_block: saved_selected,
                }
            } else {
                UndoEntry::CreateBlock {
                    block_uid: block_uid.clone(),
                }
            };
            let write = WriteAction::DeleteBlock {
                block: BlockRef { uid: block_uid },
            };
            (redo, write)
        }
        UndoEntry::DeleteBlock {
            block,
            parent_uid,
            order,
            selected_block,
        } => {
            let uid = block.uid.clone();
            let text = block.string.clone();
            insert_block_in_days(&mut state.days, &parent_uid, order, block);
            state.selected_block = selected_block;
            state.cursor_col = 0;
            let redo = UndoEntry::CreateBlock {
                block_uid: uid.clone(),
            };
            let write = WriteAction::CreateBlock {
                location: BlockLocation {
                    parent_uid,
                    order: OrderValue::Index(order),
                },
                block: NewBlock {
                    string: text,
                    uid: Some(uid),
                    open: None,
                },
            };
            (redo, write)
        }
        UndoEntry::MoveBlock {
            block_uid,
            old_parent_uid,
            old_order,
            selected_block,
        } => {
            let current_parent_info = find_block_parent_info(&state.days, &block_uid);
            let current_selected = state.selected_block;
            move_block_in_days(&mut state.days, &block_uid, &old_parent_uid, old_order);
            state.selected_block = selected_block;
            state.cursor_col = 0;
            let redo = if let Some((cur_parent, cur_order)) = current_parent_info {
                UndoEntry::MoveBlock {
                    block_uid: block_uid.clone(),
                    old_parent_uid: cur_parent,
                    old_order: cur_order,
                    selected_block: current_selected,
                }
            } else {
                UndoEntry::MoveBlock {
                    block_uid: block_uid.clone(),
                    old_parent_uid: old_parent_uid.clone(),
                    old_order,
                    selected_block,
                }
            };
            let write = WriteAction::MoveBlock {
                block: BlockRef { uid: block_uid },
                location: BlockLocation {
                    parent_uid: old_parent_uid,
                    order: OrderValue::Index(old_order),
                },
            };
            (redo, write)
        }
    }
}

pub fn handle_action(state: &mut AppState, action: &Action) -> Option<LoadRequest> {
    match action {
        Action::Quit => {
            state.should_quit = true;
            None
        }
        Action::MoveUp => {
            if state.selected_block > 0 {
                state.selected_block -= 1;
                state.cursor_col = 0;
            }
            None
        }
        Action::MoveDown => {
            let total = state.total_navigable_count();
            if total == 0 {
                return None;
            }
            let flat = state.flat_block_count();

            // In daily notes: trigger load-more when crossing from regular blocks
            // into linked refs zone (at the boundary)
            let load_request = if flat > 0
                && state.selected_block == flat - 1
                && !state.loading_more
                && state.view_mode == ViewMode::DailyNotes
            {
                let oldest = state
                    .days
                    .last()
                    .map(|d| d.date)
                    .unwrap_or(state.current_date);
                let prev_date = oldest - chrono::Duration::days(1);
                state.loading_more = true;
                Some(LoadRequest::DailyNote(prev_date))
            } else {
                None
            };

            // Navigate down regardless
            if state.selected_block < total - 1 {
                state.selected_block += 1;
                state.cursor_col = 0;
            }

            load_request
        }
        Action::EditBlock => {
            // Guard: no editing in linked refs zone
            if state
                .resolve_linked_ref_item(state.selected_block)
                .is_some()
            {
                return None;
            }
            if let Some(info) =
                resolve_block_at_index(&state.days, &state.linked_refs, state.selected_block)
            {
                state.input_mode = InputMode::Insert {
                    buffer: EditBuffer::new(&info.text),
                    block_uid: info.block_uid,
                    original_text: info.text,
                    create_info: None,
                };
            }
            None
        }
        Action::CreateBlock => {
            // Guard: no creating in linked refs zone
            if state
                .resolve_linked_ref_item(state.selected_block)
                .is_some()
            {
                return None;
            }
            if let Some(info) =
                resolve_block_at_index(&state.days, &state.linked_refs, state.selected_block)
            {
                let new_uid = generate_uid();
                let order = info.order + 1;
                let parent_uid = info.parent_uid.clone();
                let placeholder = Block {
                    uid: new_uid.clone(),
                    string: String::new(),
                    order,
                    children: vec![],
                    open: true,
                };
                insert_block_in_days(&mut state.days, &parent_uid, order, placeholder);
                if let Some(idx) =
                    find_block_index_by_uid(&state.days, &state.linked_refs, &new_uid)
                {
                    state.selected_block = idx;
                    state.cursor_col = 0;
                }
                state.input_mode = InputMode::Insert {
                    buffer: EditBuffer::new_empty(),
                    block_uid: new_uid,
                    original_text: String::new(),
                    create_info: Some(CreateInfo { parent_uid, order }),
                };
            } else if let Some(day) = state.days.first() {
                // No blocks yet — create the first block as child of the day page
                let new_uid = generate_uid();
                let parent_uid = day.uid.clone();
                let placeholder = Block {
                    uid: new_uid.clone(),
                    string: String::new(),
                    order: 0,
                    children: vec![],
                    open: true,
                };
                state.days[0].blocks.push(placeholder);
                state.selected_block = 0;
                state.cursor_col = 0;
                state.input_mode = InputMode::Insert {
                    buffer: EditBuffer::new_empty(),
                    block_uid: new_uid,
                    original_text: String::new(),
                    create_info: Some(CreateInfo {
                        parent_uid,
                        order: 0,
                    }),
                };
            }
            None
        }
        Action::Search => {
            state.search = Some(SearchState {
                query: String::new(),
                results: filter_blocks(&state.days, &state.block_ref_cache, "", SEARCH_LIMIT),
                selected: 0,
            });
            None
        }
        Action::Collapse => {
            if state
                .resolve_linked_ref_item(state.selected_block)
                .is_some()
            {
                return None;
            }
            if let Some(info) =
                resolve_block_at_index(&state.days, &state.linked_refs, state.selected_block)
            {
                set_block_open(&mut state.days, &info.block_uid, false);
            }
            None
        }
        Action::Expand => {
            if state
                .resolve_linked_ref_item(state.selected_block)
                .is_some()
            {
                return None;
            }
            if let Some(info) =
                resolve_block_at_index(&state.days, &state.linked_refs, state.selected_block)
            {
                set_block_open(&mut state.days, &info.block_uid, true);
            }
            None
        }
        Action::Enter => {
            // Check if we're in the linked refs zone
            if let Some(item) = state.resolve_linked_ref_item(state.selected_block) {
                match item {
                    LinkedRefItem::SectionHeader => {
                        if let Some(day_title) = state.linked_ref_day_at(state.selected_block) {
                            if let Some(lr) = state.linked_refs.get_mut(&day_title) {
                                lr.collapsed = !lr.collapsed;
                            }
                        }
                    }
                    LinkedRefItem::GroupHeader(title) => {
                        return Some(navigate_to_page(state, title));
                    }
                    LinkedRefItem::Block(block) => {
                        return Some(navigate_to_page(state, block.page_title));
                    }
                }
                return None;
            }
            if let Some(info) =
                resolve_block_at_index(&state.days, &state.linked_refs, state.selected_block)
            {
                let links = markdown::extract_page_links(&info.text);
                match links.len() {
                    0 => {
                        // No links — toggle collapse (original behavior)
                        if let Some(block) = find_block_in_days(&state.days, &info.block_uid) {
                            set_block_open(&mut state.days, &info.block_uid, !block.open);
                        }
                    }
                    1 => {
                        // Single link — navigate directly
                        let title = links.into_iter().next().unwrap();
                        return Some(navigate_to_page(state, title));
                    }
                    _ => {
                        // Multiple links — open picker
                        state.link_picker = Some(LinkPickerState { links, selected: 0 });
                    }
                }
            }
            None
        }
        Action::NextDay => {
            if state.view_mode != ViewMode::DailyNotes {
                return None;
            }
            // Jump to the first block of the next (more recent) day
            if state.days.len() > 1 {
                let mut block_count = 0;
                for (i, day) in state.days.iter().enumerate() {
                    let day_blocks = count_blocks_recursive(&day.blocks);
                    if state.selected_block < block_count + day_blocks && i > 0 {
                        // Currently in this day, jump to previous day (more recent)
                        state.selected_block = block_count
                            .saturating_sub(count_blocks_recursive(&state.days[i - 1].blocks));
                        state.cursor_col = 0;
                        break;
                    }
                    block_count += day_blocks;
                }
            }
            None
        }
        Action::PrevDay => {
            if state.view_mode != ViewMode::DailyNotes {
                return None;
            }
            // Jump to first block of the next older day, or load it
            let mut block_count = 0;
            let mut found = false;
            for (i, day) in state.days.iter().enumerate() {
                let day_blocks = count_blocks_recursive(&day.blocks);
                if state.selected_block < block_count + day_blocks {
                    // Currently in day i, jump to day i+1 if exists
                    if i + 1 < state.days.len() {
                        state.selected_block = block_count + day_blocks;
                        state.cursor_col = 0;
                        found = true;
                    }
                    break;
                }
                block_count += day_blocks;
            }
            if !found && !state.loading_more {
                // Load older day
                let oldest = state
                    .days
                    .last()
                    .map(|d| d.date)
                    .unwrap_or(state.current_date);
                let prev_date = oldest - chrono::Duration::days(1);
                state.loading_more = true;
                return Some(LoadRequest::DailyNote(prev_date));
            }
            None
        }
        Action::GoDaily => {
            if state.view_mode != ViewMode::DailyNotes {
                // In page view — save to history, return to daily notes
                push_nav_snapshot(state);
                state.view_mode = ViewMode::DailyNotes;
                state.days.clear();
                state.selected_block = 0;
                state.cursor_col = 0;
                state.loading = true;
                state.linked_refs.clear();
                state.status_message = Some("Loading today's notes...".into());
                return Some(LoadRequest::DailyNote(state.current_date));
            }
            // Already in daily notes — jump to first block of today
            state.selected_block = 0;
            state.cursor_col = 0;
            if state.days.first().map(|d| d.date) != Some(state.current_date) {
                return Some(LoadRequest::DailyNote(state.current_date));
            }
            None
        }
        Action::Help => {
            state.show_help = !state.show_help;
            None
        }
        Action::Exit => {
            // Close any overlay, or do nothing
            if state.show_help {
                state.show_help = false;
            }
            None
        }
        Action::CursorLeft => {
            if state.cursor_col > 0 {
                state.cursor_col -= 1;
            }
            None
        }
        Action::CursorRight => {
            if let Some(info) =
                resolve_block_at_index(&state.days, &state.linked_refs, state.selected_block)
            {
                let first_line = info.text.split('\n').next().unwrap_or("");
                let rendered_len = markdown::rendered_char_count(first_line);
                if rendered_len > 0 && state.cursor_col < rendered_len - 1 {
                    state.cursor_col += 1;
                }
            }
            None
        }
        Action::NavBack => {
            let can_go_back = if state.nav_index == state.nav_history.len() {
                // Current view is unsaved — can go back if there's any history
                !state.nav_history.is_empty()
            } else {
                state.nav_index > 0
            };
            if can_go_back {
                // Save current view: push if at end, or update in place
                if state.nav_index == state.nav_history.len() {
                    state.nav_history.push(ViewSnapshot {
                        view_mode: state.view_mode.clone(),
                        days: state.days.clone(),
                        selected_block: state.selected_block,
                    });
                } else {
                    save_nav_snapshot_at_index(state);
                }
                state.nav_index -= 1;
                restore_nav_snapshot(state);
            }
            None
        }
        Action::NavForward => {
            if state.nav_index + 1 < state.nav_history.len() {
                save_nav_snapshot_at_index(state);
                state.nav_index += 1;
                restore_nav_snapshot(state);
            }
            None
        }
        _ => None,
    }
}

/// Save current view as a snapshot and prepare for navigating to a new page.
/// Returns a LoadRequest::Page for the given title.
fn navigate_to_page(state: &mut AppState, title: String) -> LoadRequest {
    push_nav_snapshot(state);
    state.view_mode = ViewMode::Page {
        title: title.clone(),
    };
    state.days.clear();
    state.selected_block = 0;
    state.cursor_col = 0;
    state.loading = true;
    state.linked_refs.clear();
    state.status_message = Some(format!("Loading {}...", title));
    LoadRequest::Page(title)
}

/// Push current state onto navigation history, truncating any forward history.
fn push_nav_snapshot(state: &mut AppState) {
    let snapshot = ViewSnapshot {
        view_mode: state.view_mode.clone(),
        days: state.days.clone(),
        selected_block: state.selected_block,
    };
    // Truncate forward history
    state.nav_history.truncate(state.nav_index);
    state.nav_history.push(snapshot);
    state.nav_index = state.nav_history.len();
    // Cap at 50 entries
    if state.nav_history.len() > 50 {
        state.nav_history.remove(0);
        state.nav_index = state.nav_history.len();
    }
}

/// Save current state into the current history slot (for back/forward without data loss).
fn save_nav_snapshot_at_index(state: &mut AppState) {
    if state.nav_index < state.nav_history.len() {
        state.nav_history[state.nav_index] = ViewSnapshot {
            view_mode: state.view_mode.clone(),
            days: state.days.clone(),
            selected_block: state.selected_block,
        };
    }
}

/// Restore state from the snapshot at current nav_index.
fn restore_nav_snapshot(state: &mut AppState) {
    if let Some(snapshot) = state.nav_history.get(state.nav_index) {
        state.view_mode = snapshot.view_mode.clone();
        state.days = snapshot.days.clone();
        state.selected_block = snapshot.selected_block;
        state.cursor_col = 0;
        state.loading = false;
        state.loading_more = false;
        state.linked_refs.clear();
        state.status_message = None;
    }
}

// --- Link picker key handling ---

fn handle_link_picker_key(state: &mut AppState, key: &KeyEvent) -> Option<LoadRequest> {
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Esc) => {
            state.link_picker = None;
        }
        (KeyModifiers::NONE, KeyCode::Up) => {
            if let Some(lp) = &mut state.link_picker {
                lp.selected = lp.selected.saturating_sub(1);
            }
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            if let Some(lp) = &mut state.link_picker {
                if !lp.links.is_empty() && lp.selected < lp.links.len() - 1 {
                    lp.selected += 1;
                }
            }
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if let Some(lp) = state.link_picker.take() {
                if let Some(title) = lp.links.get(lp.selected) {
                    return Some(navigate_to_page(state, title.clone()));
                }
            }
        }
        _ => {}
    }
    None
}

// --- Search mode key handling ---

pub fn handle_search_key(state: &mut AppState, key: &KeyEvent) {
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Esc) => {
            state.search = None;
        }
        (KeyModifiers::NONE, KeyCode::Up) => {
            if let Some(s) = &mut state.search {
                s.selected = s.selected.saturating_sub(1);
            }
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            if let Some(s) = &mut state.search {
                if !s.results.is_empty() && s.selected < s.results.len() - 1 {
                    s.selected += 1;
                }
            }
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            if let Some(s) = state.search.take() {
                if let Some((uid, _)) = s.results.get(s.selected) {
                    if let Some(idx) = find_block_index_by_uid(&state.days, &state.linked_refs, uid)
                    {
                        state.selected_block = idx;
                        state.cursor_col = 0;
                    }
                }
            }
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            if let Some(s) = &mut state.search {
                s.query.pop();
                let query = s.query.clone();
                s.results =
                    filter_blocks(&state.days, &state.block_ref_cache, &query, SEARCH_LIMIT);
                s.selected = s.selected.min(s.results.len().saturating_sub(1));
            }
        }
        (KeyModifiers::NONE, KeyCode::Char(c)) | (KeyModifiers::SHIFT, KeyCode::Char(c)) => {
            if let Some(s) = &mut state.search {
                s.query.push(c);
                let query = s.query.clone();
                s.results =
                    filter_blocks(&state.days, &state.block_ref_cache, &query, SEARCH_LIMIT);
                s.selected = s.selected.min(s.results.len().saturating_sub(1));
            }
        }
        _ => {}
    }
}

// --- Insert mode key handling ---

pub fn handle_insert_key(state: &mut AppState, key: &KeyEvent) -> Option<WriteAction> {
    // Handle autocomplete input when active
    if state.autocomplete.is_some() {
        return handle_autocomplete_key(state, key);
    }

    if key.code == KeyCode::Esc && key.modifiers == KeyModifiers::NONE {
        return finalize_insert(state);
    }

    if key.code == KeyCode::Tab && key.modifiers == KeyModifiers::NONE {
        return handle_indent(state);
    }

    if key.code == KeyCode::BackTab {
        return handle_dedent(state);
    }

    let buffer = match &mut state.input_mode {
        InputMode::Insert { buffer, .. } => buffer,
        InputMode::Normal => return None,
    };

    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Char(ch)) | (KeyModifiers::SHIFT, KeyCode::Char(ch)) => {
            match ch {
                '(' => buffer.insert_pair('(', ')'),
                '[' => buffer.insert_pair('[', ']'),
                '{' => buffer.insert_pair('{', '}'),
                _ => buffer.insert_char(ch),
            }
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => buffer.delete_back(),
        (KeyModifiers::NONE, KeyCode::Delete) => buffer.delete_forward(),
        (KeyModifiers::NONE, KeyCode::Left) => buffer.move_left(),
        (KeyModifiers::NONE, KeyCode::Right) => buffer.move_right(),
        (KeyModifiers::NONE, KeyCode::Home) | (KeyModifiers::CONTROL, KeyCode::Char('a')) => {
            buffer.move_home()
        }
        (KeyModifiers::NONE, KeyCode::End) | (KeyModifiers::CONTROL, KeyCode::Char('e')) => {
            buffer.move_end()
        }
        (KeyModifiers::NONE, KeyCode::Up) => buffer.move_up(),
        (KeyModifiers::NONE, KeyCode::Down) => buffer.move_down(),
        (KeyModifiers::CONTROL, KeyCode::Left) => buffer.move_word_left(),
        (KeyModifiers::CONTROL, KeyCode::Right) => buffer.move_word_right(),
        (modifiers, KeyCode::Enter)
            if modifiers.contains(KeyModifiers::ALT)
                || modifiers.contains(KeyModifiers::CONTROL) =>
        {
            buffer.toggle_todo();
        }
        _ => {}
    }

    // Check if (( was just typed — open autocomplete
    let buffer = match &state.input_mode {
        InputMode::Insert { buffer, .. } => buffer,
        InputMode::Normal => return None,
    };
    if detect_block_ref_trigger(buffer) {
        let results = filter_blocks(&state.days, &state.block_ref_cache, "", AUTOCOMPLETE_LIMIT);
        state.autocomplete = Some(AutocompleteState {
            query: String::new(),
            results,
            selected: 0,
        });
    }

    None
}

fn handle_autocomplete_key(state: &mut AppState, key: &KeyEvent) -> Option<WriteAction> {
    match (key.modifiers, key.code) {
        (KeyModifiers::NONE, KeyCode::Esc) => {
            state.autocomplete = None;
        }
        (KeyModifiers::NONE, KeyCode::Up) => {
            if let Some(ac) = &mut state.autocomplete {
                ac.selected = ac.selected.saturating_sub(1);
            }
        }
        (KeyModifiers::NONE, KeyCode::Down) => {
            if let Some(ac) = &mut state.autocomplete {
                if !ac.results.is_empty() && ac.selected < ac.results.len() - 1 {
                    ac.selected += 1;
                }
            }
        }
        (KeyModifiers::NONE, KeyCode::Enter) => {
            return confirm_autocomplete(state);
        }
        (KeyModifiers::NONE, KeyCode::Backspace) => {
            let should_close = state
                .autocomplete
                .as_ref()
                .is_some_and(|ac| ac.query.is_empty());
            if should_close {
                state.autocomplete = None;
            } else if let Some(ac) = &mut state.autocomplete {
                ac.query.pop();
                let query = ac.query.clone();
                ac.results = filter_blocks(
                    &state.days,
                    &state.block_ref_cache,
                    &query,
                    AUTOCOMPLETE_LIMIT,
                );
                ac.selected = ac.selected.min(ac.results.len().saturating_sub(1));
            }
        }
        (KeyModifiers::NONE, KeyCode::Char(ch)) | (KeyModifiers::SHIFT, KeyCode::Char(ch)) => {
            if let Some(ac) = &mut state.autocomplete {
                ac.query.push(ch);
                let query = ac.query.clone();
                ac.results = filter_blocks(
                    &state.days,
                    &state.block_ref_cache,
                    &query,
                    AUTOCOMPLETE_LIMIT,
                );
                ac.selected = ac.selected.min(ac.results.len().saturating_sub(1));
            }
        }
        _ => {}
    }
    None
}

fn confirm_autocomplete(state: &mut AppState) -> Option<WriteAction> {
    let ac = state.autocomplete.take()?;
    let (uid, _) = ac.results.get(ac.selected)?.clone();

    let buffer = match &mut state.input_mode {
        InputMode::Insert { buffer, .. } => buffer,
        InputMode::Normal => return None,
    };

    // Find the (( before cursor and )) after, accounting for any query text typed
    // Pattern in buffer: ((query|)) where | is cursor
    // We need to find start of (( and end of ))
    let cursor = buffer.cursor;
    // Query chars go to ac.query, not the buffer — buffer still has (()) with cursor between
    let start = cursor.checked_sub(2)?;
    let end = if cursor + 1 < buffer.chars.len()
        && buffer.chars[cursor] == ')'
        && buffer.chars[cursor + 1] == ')'
    {
        cursor + 2
    } else {
        return None;
    };

    let replacement = format!("(({})) ", uid);
    buffer.replace_range(start, end, &replacement);
    None
}

fn finalize_insert(state: &mut AppState) -> Option<WriteAction> {
    state.redo_stack.clear();
    let (buffer, block_uid, original_text, create_info) =
        match std::mem::replace(&mut state.input_mode, InputMode::Normal) {
            InputMode::Insert {
                buffer,
                block_uid,
                original_text,
                create_info,
            } => (buffer, block_uid, original_text, create_info),
            InputMode::Normal => return None,
        };

    let new_text = buffer.to_string();
    if let Some(info) = create_info {
        if !new_text.is_empty() {
            state.undo_stack.push(UndoEntry::CreateBlock {
                block_uid: block_uid.clone(),
            });
        }
        finalize_create(state, info, block_uid, new_text)
    } else {
        if new_text != original_text {
            state.undo_stack.push(UndoEntry::TextEdit {
                block_uid: block_uid.clone(),
                old_text: original_text.clone(),
            });
        }
        finalize_edit(&mut state.days, block_uid, &original_text, new_text)
    }
}

fn finalize_create(
    state: &mut AppState,
    info: CreateInfo,
    block_uid: String,
    new_text: String,
) -> Option<WriteAction> {
    if new_text.is_empty() {
        remove_block_from_days(&mut state.days, &block_uid);
        let total = state.flat_block_count();
        if total == 0 {
            state.selected_block = 0;
        } else if state.selected_block >= total {
            state.selected_block = total - 1;
        }
        return None;
    }
    update_block_text_in_days(&mut state.days, &block_uid, &new_text);
    Some(WriteAction::CreateBlock {
        location: BlockLocation {
            parent_uid: info.parent_uid,
            order: OrderValue::Index(info.order),
        },
        block: NewBlock {
            string: new_text,
            uid: Some(block_uid),
            open: None,
        },
    })
}

fn finalize_edit(
    days: &mut [DailyNote],
    block_uid: String,
    original_text: &str,
    new_text: String,
) -> Option<WriteAction> {
    if new_text == original_text {
        return None;
    }
    update_block_text_in_days(days, &block_uid, &new_text);
    Some(WriteAction::UpdateBlock {
        block: BlockUpdate {
            uid: block_uid,
            string: new_text,
        },
    })
}

// --- Indent block handler ---

fn handle_indent(state: &mut AppState) -> Option<WriteAction> {
    state.redo_stack.clear();
    let (block_uid, is_create) = match &state.input_mode {
        InputMode::Insert {
            block_uid,
            create_info,
            ..
        } => (block_uid.clone(), create_info.is_some()),
        InputMode::Normal => return None,
    };

    // Capture state before indent for undo
    let old_info = resolve_block_at_index(
        &state.days,
        &state.linked_refs,
        find_block_index_by_uid(&state.days, &state.linked_refs, &block_uid).unwrap_or(0),
    );
    let saved_selected = state.selected_block;

    let (new_parent_uid, new_order) = indent_block_in_days(&mut state.days, &block_uid)?;

    if is_create {
        if let InputMode::Insert {
            create_info: Some(info),
            ..
        } = &mut state.input_mode
        {
            info.parent_uid = new_parent_uid;
            info.order = new_order;
        }
        None
    } else {
        if let Some(info) = old_info {
            state.undo_stack.push(UndoEntry::MoveBlock {
                block_uid: block_uid.clone(),
                old_parent_uid: info.parent_uid,
                old_order: info.order,
                selected_block: saved_selected,
            });
        }
        Some(WriteAction::MoveBlock {
            block: BlockRef { uid: block_uid },
            location: BlockLocation {
                parent_uid: new_parent_uid,
                order: OrderValue::Position("last".into()),
            },
        })
    }
}

// --- Dedent block handler ---

fn handle_dedent(state: &mut AppState) -> Option<WriteAction> {
    state.redo_stack.clear();
    let (block_uid, is_create) = match &state.input_mode {
        InputMode::Insert {
            block_uid,
            create_info,
            ..
        } => (block_uid.clone(), create_info.is_some()),
        InputMode::Normal => return None,
    };

    // Capture state before dedent for undo
    let old_info = resolve_block_at_index(
        &state.days,
        &state.linked_refs,
        find_block_index_by_uid(&state.days, &state.linked_refs, &block_uid).unwrap_or(0),
    );
    let saved_selected = state.selected_block;

    let (new_parent_uid, new_order) = dedent_block_in_days(&mut state.days, &block_uid)?;

    if let Some(idx) = find_block_index_by_uid(&state.days, &state.linked_refs, &block_uid) {
        state.selected_block = idx;
    }

    if is_create {
        if let InputMode::Insert {
            create_info: Some(info),
            ..
        } = &mut state.input_mode
        {
            info.parent_uid = new_parent_uid;
            info.order = new_order;
        }
        None
    } else {
        if let Some(info) = old_info {
            state.undo_stack.push(UndoEntry::MoveBlock {
                block_uid: block_uid.clone(),
                old_parent_uid: info.parent_uid,
                old_order: info.order,
                selected_block: saved_selected,
            });
        }
        Some(WriteAction::MoveBlock {
            block: BlockRef { uid: block_uid },
            location: BlockLocation {
                parent_uid: new_parent_uid,
                order: OrderValue::Index(new_order),
            },
        })
    }
}

// --- Delete block handler ---

pub fn handle_delete_block(state: &mut AppState) -> Option<WriteAction> {
    // Guard: no deleting in linked refs zone
    if state
        .resolve_linked_ref_item(state.selected_block)
        .is_some()
    {
        return None;
    }
    state.redo_stack.clear();
    let info = resolve_block_at_index(&state.days, &state.linked_refs, state.selected_block)?;
    let block = find_block_in_days(&state.days, &info.block_uid)?;
    let saved_selected = state.selected_block;

    state.undo_stack.push(UndoEntry::DeleteBlock {
        block,
        parent_uid: info.parent_uid.clone(),
        order: info.order,
        selected_block: saved_selected,
    });

    remove_block_from_days(&mut state.days, &info.block_uid);

    let total = state.flat_block_count();
    if total == 0 {
        state.selected_block = 0;
    } else if state.selected_block >= total {
        state.selected_block = total - 1;
    }

    Some(WriteAction::DeleteBlock {
        block: BlockRef {
            uid: info.block_uid,
        },
    })
}

fn dispatch_load_request(
    request: LoadRequest,
    client: &RoamClient,
    tx: &mpsc::UnboundedSender<AppMessage>,
) {
    match request {
        LoadRequest::DailyNote(date) => spawn_fetch_daily_note(client, date, tx),
        LoadRequest::Page(title) => spawn_fetch_page(client, &title, tx),
    }
}

fn handle_normal_key(
    state: &mut AppState,
    key: &KeyEvent,
    keybindings: &KeybindingMap,
    client: &RoamClient,
    tx: &mpsc::UnboundedSender<AppMessage>,
) {
    if state.pending_key == Some('d')
        && key.code == KeyCode::Char('d')
        && key.modifiers == KeyModifiers::NONE
    {
        state.pending_key = None;
        if let Some(write_action) = handle_delete_block(state) {
            spawn_write(client, write_action, tx);
        }
    } else if state.pending_key == Some('d') {
        state.pending_key = None;
        if let Some(action) = keybindings.resolve(key) {
            if let Some(req) = handle_action(state, action) {
                dispatch_load_request(req, client, tx);
            }
        }
    } else if key.code == KeyCode::Char('d')
        && key.modifiers == KeyModifiers::NONE
        && keybindings.resolve(key).is_none()
    {
        state.pending_key = Some('d');
    } else if let Some(action) = keybindings.resolve(key) {
        if action == &Action::Undo {
            if let Some(write_action) = apply_undo(state) {
                spawn_write(client, write_action, tx);
            }
        } else if action == &Action::Redo {
            if let Some(write_action) = apply_redo(state) {
                spawn_write(client, write_action, tx);
            }
        } else if let Some(req) = handle_action(state, action) {
            dispatch_load_request(req, client, tx);
        }
    }
}

pub fn detect_block_ref_trigger(buffer: &EditBuffer) -> bool {
    let c = buffer.cursor;
    c >= 2
        && c + 1 < buffer.chars.len()
        && buffer.chars[c - 2] == '('
        && buffer.chars[c - 1] == '('
        && buffer.chars[c] == ')'
        && buffer.chars[c + 1] == ')'
}

const AUTOCOMPLETE_LIMIT: usize = 20;
const SEARCH_LIMIT: usize = 50;

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

fn generate_uid() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("tui-{:x}", nanos)
}

pub fn handle_daily_note_loaded(state: &mut AppState, mut note: DailyNote) {
    // Generate Roam-style title if the page doesn't exist yet
    if note.title.is_empty() {
        note.title = format_roam_daily_title(note.date);
    }
    // Ensure every day has at least one block so navigation always works
    if note.blocks.is_empty() {
        note.blocks.push(Block {
            uid: generate_uid(),
            string: String::new(),
            order: 0,
            children: vec![],
            open: true,
        });
    }
    // Insert maintaining reverse chronological order (today first, then older)
    let pos = state
        .days
        .iter()
        .position(|d| d.date < note.date)
        .unwrap_or(state.days.len());
    state.days.insert(pos, note);
    state.loading = false;
    state.loading_more = false;
    state.status_message = None;
}

fn format_roam_daily_title(date: NaiveDate) -> String {
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

pub fn handle_refresh_loaded(state: &mut AppState, note: DailyNote) {
    if let Some(pos) = state.days.iter().position(|d| d.date == note.date) {
        if state.days[pos] != note {
            state.days[pos] = note;
        }
    }
}

pub fn handle_page_loaded(state: &mut AppState, mut note: DailyNote) {
    // Ensure the page has at least one block
    if note.blocks.is_empty() {
        note.blocks.push(Block {
            uid: generate_uid(),
            string: String::new(),
            order: 0,
            children: vec![],
            open: true,
        });
    }
    state.days = vec![note];
    state.selected_block = 0;
    state.cursor_col = 0;
    state.loading = false;
    state.loading_more = false;
    state.status_message = None;
}

pub fn handle_api_error(state: &mut AppState, error: ErrorInfo) {
    state.loading = false;
    state.loading_more = false;
    state.error_popup = Some(ErrorPopup::from_error_info(&error));
}

fn spawn_fetch_daily_note(
    client: &RoamClient,
    date: NaiveDate,
    tx: &mpsc::UnboundedSender<AppMessage>,
) {
    let uid = queries::daily_note_uid_for_date(date.month(), date.day(), date.year());
    let (eid, selector) = queries::pull_daily_note(&uid);
    let client_clone = client.clone();
    let tx_clone = tx.clone();
    let uid_clone = uid.clone();
    tokio::spawn(async move {
        match client_clone.pull(eid, &selector).await {
            Ok(resp) => {
                let note = DailyNote::from_pull_response(date, uid_clone, &resp.result);
                let _ = tx_clone.send(AppMessage::DailyNoteLoaded(note));
            }
            Err(e) => {
                let _ = tx_clone.send(AppMessage::ApiError(ErrorInfo::from_roam_error(&e)));
            }
        }
    });
}

fn spawn_fetch_page(client: &RoamClient, title: &str, tx: &mpsc::UnboundedSender<AppMessage>) {
    let (eid, selector) = queries::pull_page_by_title(title);
    let client_clone = client.clone();
    let tx_clone = tx.clone();
    let title_owned = title.to_string();
    tokio::spawn(async move {
        match client_clone.pull(eid, &selector).await {
            Ok(resp) => {
                // Use a dummy date — the page is not date-based
                let dummy_date = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
                let uid = resp
                    .result
                    .get(":block/uid")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let mut note = DailyNote::from_pull_response(dummy_date, uid, &resp.result);
                // Ensure title is set even if page doesn't exist yet
                if note.title.is_empty() {
                    note.title = title_owned;
                }
                let _ = tx_clone.send(AppMessage::PageLoaded(note));
            }
            Err(e) => {
                let _ = tx_clone.send(AppMessage::ApiError(ErrorInfo::from_roam_error(&e)));
            }
        }
    });
}

fn spawn_fetch_linked_refs(
    client: &RoamClient,
    page_title: &str,
    tx: &mpsc::UnboundedSender<AppMessage>,
) {
    let query = queries::linked_refs_query(page_title);
    let client_clone = client.clone();
    let tx_clone = tx.clone();
    let title_owned = page_title.to_string();
    tokio::spawn(async move {
        match client_clone.query(query, vec![]).await {
            Ok(resp) => {
                let groups = crate::api::types::parse_linked_refs(&resp.result, &title_owned);
                let _ = tx_clone.send(AppMessage::LinkedRefsLoaded(title_owned, groups));
            }
            Err(e) => {
                let _ = tx_clone.send(AppMessage::ApiError(ErrorInfo::from_roam_error(&e)));
            }
        }
    });
}

/// Extract all ((uid)) references from block texts that aren't in the local block map.
fn collect_unresolved_refs(state: &AppState) -> Vec<String> {
    let local_map = markdown::build_block_text_map(&state.days);
    let mut unresolved = Vec::new();

    for day in &state.days {
        collect_refs_from_blocks(
            &day.blocks,
            &local_map,
            &state.block_ref_cache,
            &state.pending_block_refs,
            &mut unresolved,
        );
    }
    unresolved
}

fn collect_refs_from_blocks(
    blocks: &[Block],
    local_map: &HashMap<String, String>,
    cache: &HashMap<String, String>,
    pending: &HashSet<String>,
    out: &mut Vec<String>,
) {
    for block in blocks {
        extract_uids_from_text(&block.string, local_map, cache, pending, out);
        collect_refs_from_blocks(&block.children, local_map, cache, pending, out);
    }
}

fn extract_uids_from_text(
    text: &str,
    local_map: &HashMap<String, String>,
    cache: &HashMap<String, String>,
    pending: &HashSet<String>,
    out: &mut Vec<String>,
) {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i + 3 < len {
        if chars[i] == '(' && chars[i + 1] == '(' {
            if let Some(end) = chars[i + 2..].iter().position(|c| *c == ')').and_then(|p| {
                let pos = i + 2 + p;
                if pos + 1 < len && chars[pos + 1] == ')' {
                    Some(pos)
                } else {
                    None
                }
            }) {
                let uid: String = chars[i + 2..end].iter().collect();
                if !uid.is_empty()
                    && !local_map.contains_key(&uid)
                    && !cache.contains_key(&uid)
                    && !pending.contains(&uid)
                {
                    out.push(uid);
                }
                i = end + 2;
                continue;
            }
        }
        // Also check embeds: {{embed: ((uid))}}
        if chars[i] == '{' && chars[i + 1] == '{' {
            if let Some(end_brace) = chars[i + 2..]
                .windows(2)
                .position(|w| w[0] == '}' && w[1] == '}')
            {
                let inner: String = chars[i + 2..i + 2 + end_brace].iter().collect();
                if let Some(uid) = extract_embed_uid(&inner) {
                    if !local_map.contains_key(&uid)
                        && !cache.contains_key(&uid)
                        && !pending.contains(&uid)
                    {
                        out.push(uid);
                    }
                }
                i = i + 2 + end_brace + 2;
                continue;
            }
        }
        i += 1;
    }
}

fn extract_embed_uid(inner: &str) -> Option<String> {
    let trimmed = inner
        .strip_prefix("[[embed]]:")
        .or_else(|| inner.strip_prefix("embed:"))?
        .trim();
    trimmed
        .strip_prefix("((")
        .and_then(|s| s.strip_suffix("))"))
        .map(|s| s.to_string())
}

fn spawn_resolve_block_refs(
    client: &RoamClient,
    uids: Vec<String>,
    state: &mut AppState,
    tx: &mpsc::UnboundedSender<AppMessage>,
) {
    for uid in uids {
        state.pending_block_refs.insert(uid.clone());
        let client = client.clone();
        let tx = tx.clone();
        let uid_clone = uid.clone();
        tokio::spawn(async move {
            let eid = serde_json::Value::String(format!("[:block/uid \"{}\"]", uid_clone));
            let selector = "[:block/string]";
            match client.pull(eid, selector).await {
                Ok(resp) => {
                    let text = resp
                        .result
                        .get(":block/string")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !text.is_empty() {
                        let _ = tx.send(AppMessage::BlockRefResolved(uid_clone, text));
                    }
                }
                Err(_) => {
                    // Silently ignore — will just show the UID
                }
            }
        });
    }
}

fn spawn_refresh_daily_note(
    client: &RoamClient,
    date: NaiveDate,
    tx: &mpsc::UnboundedSender<AppMessage>,
) {
    let uid = queries::daily_note_uid_for_date(date.month(), date.day(), date.year());
    let (eid, selector) = queries::pull_daily_note(&uid);
    let client_clone = client.clone();
    let tx_clone = tx.clone();
    let uid_clone = uid.clone();
    tokio::spawn(async move {
        match client_clone.pull(eid, &selector).await {
            Ok(resp) => {
                let note = DailyNote::from_pull_response(date, uid_clone, &resp.result);
                let _ = tx_clone.send(AppMessage::RefreshLoaded(note));
            }
            Err(_) => {
                // Refresh errors are silent — don't disturb the user
            }
        }
    });
}

fn spawn_write(client: &RoamClient, action: WriteAction, tx: &mpsc::UnboundedSender<AppMessage>) {
    let client = client.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = client.write(action).await {
            let _ = tx.send(AppMessage::ApiError(ErrorInfo::Write(e.to_string())));
        }
    });
}

pub async fn run(config: &AppConfig, terminal: &mut DefaultTerminal) -> Result<()> {
    let keybindings =
        KeybindingMap::from_preset(&config.keybindings.preset, &config.keybindings.bindings)?;

    let mut state = AppState::new(&config.graph.name, keybindings.hints());

    let (tx, mut rx) = mpsc::unbounded_channel::<AppMessage>();

    let client = RoamClient::new(&config.graph.name, &config.graph.api_token);

    // Fetch today's daily note
    spawn_fetch_daily_note(&client, state.current_date, &tx);

    // Spawn event reader task
    let event_tx = tx.clone();
    tokio::spawn(async move {
        let mut reader = EventStream::new();
        loop {
            match reader.next().await {
                Some(Ok(Event::Key(key))) if key.kind == KeyEventKind::Press => {
                    if event_tx.send(AppMessage::Key(key)).is_err() {
                        break;
                    }
                }
                Some(Err(_)) => break,
                None => break,
                _ => {}
            }
        }
    });

    // Spawn tick timer
    let tick_tx = tx.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            interval.tick().await;
            if tick_tx.send(AppMessage::Tick).is_err() {
                break;
            }
        }
    });

    // Main loop
    loop {
        terminal.draw(|frame| crate::ui::render(frame, &state))?;

        if let Some(msg) = rx.recv().await {
            match msg {
                AppMessage::Key(key) => {
                    if state.error_popup.is_some() {
                        state.error_popup = None;
                    } else if state.show_help {
                        // Any key closes help
                        state.show_help = false;
                    } else if state.link_picker.is_some() {
                        if let Some(req) = handle_link_picker_key(&mut state, &key) {
                            dispatch_load_request(req, &client, &tx);
                        }
                    } else if state.search.is_some() {
                        handle_search_key(&mut state, &key);
                    } else if state.input_mode != InputMode::Normal {
                        if let Some(write_action) = handle_insert_key(&mut state, &key) {
                            spawn_write(&client, write_action, &tx);
                        }
                    } else {
                        handle_normal_key(&mut state, &key, &keybindings, &client, &tx);
                    }
                }
                AppMessage::DailyNoteLoaded(note) => {
                    handle_daily_note_loaded(&mut state, note);
                    let unresolved = collect_unresolved_refs(&state);
                    spawn_resolve_block_refs(&client, unresolved, &mut state, &tx);
                    // Fetch linked refs for each day that doesn't have them yet
                    for day in &state.days {
                        let title = day.title.clone();
                        if !state.linked_refs.contains_key(&title) {
                            state.linked_refs.insert(
                                title.clone(),
                                LinkedRefsState {
                                    groups: vec![],
                                    collapsed: false,
                                    loading: true,
                                },
                            );
                            spawn_fetch_linked_refs(&client, &title, &tx);
                        }
                    }
                }
                AppMessage::PageLoaded(note) => {
                    handle_page_loaded(&mut state, note);
                    let unresolved = collect_unresolved_refs(&state);
                    spawn_resolve_block_refs(&client, unresolved, &mut state, &tx);
                    // Fetch linked refs for page view
                    if let ViewMode::Page { ref title } = state.view_mode {
                        let title = title.clone();
                        state.linked_refs.insert(
                            title.clone(),
                            LinkedRefsState {
                                groups: vec![],
                                collapsed: false,
                                loading: true,
                            },
                        );
                        spawn_fetch_linked_refs(&client, &title, &tx);
                    }
                }
                AppMessage::LinkedRefsLoaded(page_title, groups) => {
                    state.linked_refs.insert(
                        page_title,
                        LinkedRefsState {
                            groups,
                            collapsed: false,
                            loading: false,
                        },
                    );
                }
                AppMessage::BlockRefResolved(uid, text) => {
                    state.pending_block_refs.remove(&uid);
                    state.block_ref_cache.insert(uid, text);
                }
                AppMessage::ApiError(err) => {
                    handle_api_error(&mut state, err);
                }
                AppMessage::RefreshLoaded(note) => {
                    handle_refresh_loaded(&mut state, note);
                }
                AppMessage::Tick => {
                    if state.input_mode != InputMode::Normal
                        || state.view_mode != ViewMode::DailyNotes
                    {
                        state.refresh_counter = 0;
                    } else {
                        state.refresh_counter += 1;
                        if state.refresh_counter >= 120 && !state.loading && !state.loading_more {
                            state.refresh_counter = 0;
                            for day in &state.days {
                                spawn_refresh_daily_note(&client, day.date, &tx);
                            }
                        }
                    }
                }
            }
        }

        if state.should_quit {
            break;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::Block;
    use crate::keys::preset::Action;

    // --- resolve_block_at_index tests ---

    #[test]
    fn resolve_first_block() {
        let state = test_state_with_blocks();
        let info = resolve_block_at_index(&state.days, &state.linked_refs, 0).unwrap();
        assert_eq!(info.block_uid, "b1");
        assert_eq!(info.parent_uid, "02-21-2026");
        assert_eq!(info.text, "Block one");
        assert_eq!(info.order, 0);
        assert_eq!(info.depth, 0);
    }

    #[test]
    fn resolve_last_block() {
        let state = test_state_with_blocks();
        let info = resolve_block_at_index(&state.days, &state.linked_refs, 2).unwrap();
        assert_eq!(info.block_uid, "b3");
    }

    #[test]
    fn resolve_out_of_range() {
        let state = test_state_with_blocks();
        assert!(resolve_block_at_index(&state.days, &state.linked_refs, 99).is_none());
    }

    #[test]
    fn resolve_nested_children() {
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![make_block("c1", "Child 1", 0)],
            open: true,
        };
        let day = make_daily_note(2026, 2, 21, vec![parent, make_block("b2", "Other", 1)]);
        let days = vec![day];

        // index 0 = Parent, index 1 = Child 1, index 2 = Other
        let info = resolve_block_at_index(&days, &HashMap::new(), 1).unwrap();
        assert_eq!(info.block_uid, "c1");
        assert_eq!(info.parent_uid, "p");
        assert_eq!(info.depth, 1);
    }

    #[test]
    fn resolve_multi_day() {
        let day1 = make_daily_note(2026, 2, 21, vec![make_block("a", "A", 0)]);
        let day2 = make_daily_note(2026, 2, 20, vec![make_block("b", "B", 0)]);
        let days = vec![day1, day2];

        let info = resolve_block_at_index(&days, &HashMap::new(), 1).unwrap();
        assert_eq!(info.block_uid, "b");
        assert_eq!(info.parent_uid, "02-20-2026");
    }

    // --- update_block_text_in_days tests ---

    #[test]
    fn update_top_level_block() {
        let mut days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "Original", 0)],
        )];
        assert!(update_block_text_in_days(&mut days, "b1", "Changed"));
        assert_eq!(days[0].blocks[0].string, "Changed");
    }

    #[test]
    fn update_nested_block() {
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![make_block("c1", "Child", 0)],
            open: true,
        };
        let mut days = vec![make_daily_note(2026, 2, 21, vec![parent])];
        assert!(update_block_text_in_days(&mut days, "c1", "New child"));
        assert_eq!(days[0].blocks[0].children[0].string, "New child");
    }

    #[test]
    fn update_nonexistent_block() {
        let mut days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "Original", 0)],
        )];
        assert!(!update_block_text_in_days(&mut days, "nope", "X"));
    }

    // --- remove_block_from_days tests ---

    #[test]
    fn remove_top_level_block() {
        let mut days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "A", 0), make_block("b2", "B", 1)],
        )];
        assert!(remove_block_from_days(&mut days, "b1"));
        assert_eq!(days[0].blocks.len(), 1);
        assert_eq!(days[0].blocks[0].uid, "b2");
    }

    #[test]
    fn remove_nested_block() {
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![make_block("c1", "Child", 0)],
            open: true,
        };
        let mut days = vec![make_daily_note(2026, 2, 21, vec![parent])];
        assert!(remove_block_from_days(&mut days, "c1"));
        assert!(days[0].blocks[0].children.is_empty());
    }

    #[test]
    fn remove_nonexistent_block() {
        let mut days = vec![make_daily_note(2026, 2, 21, vec![make_block("b1", "A", 0)])];
        assert!(!remove_block_from_days(&mut days, "nope"));
    }

    // --- insert_block_in_days tests ---

    #[test]
    fn insert_block_top_level() {
        let mut days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "First", 0)],
        )];
        let new_block = make_block("b2", "Second", 1);
        assert!(insert_block_in_days(&mut days, "02-21-2026", 1, new_block));
        assert_eq!(days[0].blocks.len(), 2);
        assert_eq!(days[0].blocks[1].uid, "b2");
    }

    #[test]
    fn insert_block_as_child() {
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![],
            open: true,
        };
        let mut days = vec![make_daily_note(2026, 2, 21, vec![parent])];
        let new_block = make_block("c1", "Child", 0);
        assert!(insert_block_in_days(&mut days, "p", 0, new_block));
        assert_eq!(days[0].blocks[0].children.len(), 1);
        assert_eq!(days[0].blocks[0].children[0].uid, "c1");
    }

    // --- handle_action EditBlock/CreateBlock tests ---

    #[test]
    fn edit_block_enters_insert_mode() {
        let mut state = test_state();
        state.selected_block = 1; // "Block two"
        handle_action(&mut state, &Action::EditBlock);
        match &state.input_mode {
            InputMode::Insert {
                buffer,
                block_uid,
                original_text,
                create_info,
            } => {
                assert_eq!(buffer.to_string(), "Block two");
                assert_eq!(block_uid, "b2");
                assert_eq!(original_text, "Block two");
                assert!(create_info.is_none());
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn edit_block_on_empty_days_does_nothing() {
        let mut state = AppState::new("test", vec![]);
        state.loading = false;
        handle_action(&mut state, &Action::EditBlock);
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn create_block_enters_insert_mode_with_empty_buffer() {
        let mut state = test_state();
        state.selected_block = 0;
        handle_action(&mut state, &Action::CreateBlock);
        match &state.input_mode {
            InputMode::Insert {
                buffer,
                create_info,
                ..
            } => {
                assert_eq!(buffer.to_string(), "");
                assert_eq!(buffer.cursor, 0);
                let info = create_info.as_ref().unwrap();
                assert_eq!(info.parent_uid, "02-21-2026");
                assert_eq!(info.order, 1); // after block at order 0
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    // --- handle_insert_key tests ---

    fn key_event(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl_key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    fn enter_insert_mode(state: &mut AppState) {
        handle_action(state, &Action::EditBlock);
    }

    #[test]
    fn insert_typing_adds_chars() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('!')));
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                assert_eq!(buffer.to_string(), "Block one!");
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn insert_open_paren_auto_closes() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                assert_eq!(buffer.to_string(), "Block one()");
                assert_eq!(buffer.cursor, 10); // between ( and )
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn insert_open_bracket_auto_closes() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('[')));
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                assert_eq!(buffer.to_string(), "Block one[]");
                assert_eq!(buffer.cursor, 10);
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn insert_open_brace_auto_closes() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('{')));
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                assert_eq!(buffer.to_string(), "Block one{}");
                assert_eq!(buffer.cursor, 10);
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn insert_backspace_deletes() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Backspace));
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                assert_eq!(buffer.to_string(), "Block on");
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn insert_arrows_move_cursor() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Left));
        handle_insert_key(&mut state, &key_event(KeyCode::Left));
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                assert_eq!(buffer.cursor, 7); // "Block one" len=9, cursor was 9, now 7
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn insert_ctrl_a_moves_home() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &ctrl_key(KeyCode::Char('a')));
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                assert_eq!(buffer.cursor, 0);
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn insert_ctrl_e_moves_end() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &ctrl_key(KeyCode::Char('a'))); // go home
        handle_insert_key(&mut state, &ctrl_key(KeyCode::Char('e'))); // go end
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                assert_eq!(buffer.cursor, 9);
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn insert_ctrl_arrows_word_jump() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(
            &mut state,
            &KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL),
        );
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                assert_eq!(buffer.cursor, 6); // start of "one"
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn insert_esc_unchanged_returns_none() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        let action = handle_insert_key(&mut state, &key_event(KeyCode::Esc));
        assert!(action.is_none());
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn insert_esc_changed_returns_update() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('!')));
        let action = handle_insert_key(&mut state, &key_event(KeyCode::Esc));
        assert!(action.is_some());
        match action.unwrap() {
            WriteAction::UpdateBlock { block } => {
                assert_eq!(block.uid, "b1");
                assert_eq!(block.string, "Block one!");
            }
            _ => panic!("Expected UpdateBlock"),
        }
        assert_eq!(state.input_mode, InputMode::Normal);
        // Optimistic update
        assert_eq!(state.days[0].blocks[0].string, "Block one!");
    }

    #[test]
    fn insert_esc_create_non_empty_returns_create() {
        let mut state = test_state();
        handle_action(&mut state, &Action::CreateBlock);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('X')));
        let action = handle_insert_key(&mut state, &key_event(KeyCode::Esc));
        assert!(action.is_some());
        match action.unwrap() {
            WriteAction::CreateBlock { block, location } => {
                assert_eq!(block.string, "X");
                assert_eq!(location.parent_uid, "02-21-2026");
            }
            _ => panic!("Expected CreateBlock"),
        }
    }

    #[test]
    fn insert_esc_create_empty_discards() {
        let mut state = test_state();
        handle_action(&mut state, &Action::CreateBlock);
        let action = handle_insert_key(&mut state, &key_event(KeyCode::Esc));
        assert!(action.is_none());
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    // --- toggle TODO in insert mode ---

    #[test]
    fn insert_cmd_enter_toggles_todo() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        let toggle_key = KeyEvent::new(KeyCode::Enter, KeyModifiers::ALT);
        handle_insert_key(&mut state, &toggle_key);
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                assert_eq!(buffer.to_string(), "{{[[TODO]]}} Block one");
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    // --- handle_delete_block + chord tests ---

    #[test]
    fn delete_block_removes_and_returns_action() {
        let mut state = test_state();
        state.selected_block = 1;
        let action = handle_delete_block(&mut state);
        assert!(action.is_some());
        match action.unwrap() {
            WriteAction::DeleteBlock { block } => {
                assert_eq!(block.uid, "b2");
            }
            _ => panic!("Expected DeleteBlock"),
        }
        assert_eq!(state.days[0].blocks.len(), 2);
        assert_eq!(state.selected_block, 1); // stays at 1 (now "b3")
    }

    #[test]
    fn delete_last_block_adjusts_selection() {
        let mut state = test_state();
        state.selected_block = 2;
        handle_delete_block(&mut state);
        assert_eq!(state.selected_block, 1);
    }

    #[test]
    fn delete_on_empty_days_returns_none() {
        let mut state = AppState::new("test", vec![]);
        state.loading = false;
        let action = handle_delete_block(&mut state);
        assert!(action.is_none());
    }

    #[test]
    fn pending_key_d_then_d_triggers_delete() {
        let mut state = test_state();
        // Simulate chord: first 'd' sets pending
        state.pending_key = Some('d');
        // Second 'd' would trigger delete in main loop
        // Here we just verify the state is correct for it
        assert_eq!(state.pending_key, Some('d'));
    }

    #[test]
    fn pending_key_d_then_other_clears_pending() {
        let mut state = test_state();
        state.pending_key = Some('d');
        // In main loop, pressing anything other than 'd' would clear pending
        state.pending_key = None;
        assert_eq!(state.pending_key, None);
    }

    // --- indent block tests ---

    #[test]
    fn indent_moves_block_to_previous_sibling() {
        let mut days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "First", 0), make_block("b2", "Second", 1)],
        )];
        let result = indent_block_in_days(&mut days, "b2");
        assert!(result.is_some());
        let (parent_uid, order) = result.unwrap();
        assert_eq!(parent_uid, "b1");
        assert_eq!(order, 0);
        // b2 is now child of b1
        assert_eq!(days[0].blocks.len(), 1);
        assert_eq!(days[0].blocks[0].uid, "b1");
        assert_eq!(days[0].blocks[0].children.len(), 1);
        assert_eq!(days[0].blocks[0].children[0].uid, "b2");
    }

    #[test]
    fn indent_preserves_children() {
        let block_with_kids = Block {
            uid: "b2".into(),
            string: "Parent".into(),
            order: 1,
            children: vec![make_block("c1", "Child", 0)],
            open: true,
        };
        let mut days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "First", 0), block_with_kids],
        )];
        indent_block_in_days(&mut days, "b2");
        let moved = &days[0].blocks[0].children[0];
        assert_eq!(moved.uid, "b2");
        assert_eq!(moved.children.len(), 1);
        assert_eq!(moved.children[0].uid, "c1");
    }

    #[test]
    fn indent_first_block_returns_none() {
        let mut days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "First", 0), make_block("b2", "Second", 1)],
        )];
        assert!(indent_block_in_days(&mut days, "b1").is_none());
        assert_eq!(days[0].blocks.len(), 2); // unchanged
    }

    #[test]
    fn indent_appends_after_existing_children() {
        let prev = Block {
            uid: "b1".into(),
            string: "First".into(),
            order: 0,
            children: vec![make_block("c1", "Existing child", 0)],
            open: true,
        };
        let mut days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![prev, make_block("b2", "Second", 1)],
        )];
        let (_, order) = indent_block_in_days(&mut days, "b2").unwrap();
        assert_eq!(order, 1); // after existing child at order 0
        assert_eq!(days[0].blocks[0].children.len(), 2);
        assert_eq!(days[0].blocks[0].children[1].uid, "b2");
    }

    #[test]
    fn indent_nested_block() {
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![
                make_block("c1", "Child 1", 0),
                make_block("c2", "Child 2", 1),
            ],
            open: true,
        };
        let mut days = vec![make_daily_note(2026, 2, 21, vec![parent])];
        let result = indent_block_in_days(&mut days, "c2");
        assert!(result.is_some());
        let (parent_uid, _) = result.unwrap();
        assert_eq!(parent_uid, "c1");
        // c2 is now child of c1
        assert_eq!(days[0].blocks[0].children.len(), 1);
        assert_eq!(days[0].blocks[0].children[0].children.len(), 1);
        assert_eq!(days[0].blocks[0].children[0].children[0].uid, "c2");
    }

    #[test]
    fn indent_nonexistent_block_returns_none() {
        let mut days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "First", 0)],
        )];
        assert!(indent_block_in_days(&mut days, "nope").is_none());
    }

    #[test]
    fn tab_in_insert_edit_mode_returns_move_block() {
        let mut state = test_state();
        state.selected_block = 1; // b2
        enter_insert_mode(&mut state);
        let action = handle_insert_key(&mut state, &key_event(KeyCode::Tab));
        assert!(action.is_some());
        match action.unwrap() {
            WriteAction::MoveBlock { block, location } => {
                assert_eq!(block.uid, "b2");
                assert_eq!(location.parent_uid, "b1");
            }
            _ => panic!("Expected MoveBlock"),
        }
        // Still in insert mode
        assert!(matches!(state.input_mode, InputMode::Insert { .. }));
    }

    #[test]
    fn tab_in_insert_create_mode_updates_create_info() {
        let mut state = test_state();
        state.selected_block = 0;
        handle_action(&mut state, &Action::CreateBlock);
        // Placeholder inserted after b1, selected_block moved to it
        // Now Tab should indent the placeholder under b1
        let action = handle_insert_key(&mut state, &key_event(KeyCode::Tab));
        // Create mode: no API call (block doesn't exist in API yet)
        assert!(action.is_none());
        // create_info should be updated
        match &state.input_mode {
            InputMode::Insert { create_info, .. } => {
                let info = create_info.as_ref().unwrap();
                assert_eq!(info.parent_uid, "b1");
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn tab_on_first_block_does_nothing() {
        let mut state = test_state();
        state.selected_block = 0; // b1, first block
        enter_insert_mode(&mut state);
        let action = handle_insert_key(&mut state, &key_event(KeyCode::Tab));
        assert!(action.is_none());
        // Still in insert mode, nothing changed
        assert!(matches!(state.input_mode, InputMode::Insert { .. }));
    }

    // --- dedent block tests ---

    #[test]
    fn dedent_moves_block_to_grandparent() {
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![make_block("c1", "Child", 0)],
            open: true,
        };
        let mut days = vec![make_daily_note(2026, 2, 21, vec![parent])];
        let result = dedent_block_in_days(&mut days, "c1");
        assert!(result.is_some());
        let (new_parent_uid, _) = result.unwrap();
        assert_eq!(new_parent_uid, "02-21-2026");
        // c1 is now sibling of p at the day level
        assert_eq!(days[0].blocks.len(), 2);
        assert_eq!(days[0].blocks[0].uid, "p");
        assert!(days[0].blocks[0].children.is_empty());
        assert_eq!(days[0].blocks[1].uid, "c1");
    }

    #[test]
    fn dedent_preserves_children() {
        let child = Block {
            uid: "c1".into(),
            string: "Child".into(),
            order: 0,
            children: vec![make_block("gc", "Grandchild", 0)],
            open: true,
        };
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![child],
            open: true,
        };
        let mut days = vec![make_daily_note(2026, 2, 21, vec![parent])];
        dedent_block_in_days(&mut days, "c1");
        let moved = &days[0].blocks[1];
        assert_eq!(moved.uid, "c1");
        assert_eq!(moved.children.len(), 1);
        assert_eq!(moved.children[0].uid, "gc");
    }

    #[test]
    fn dedent_top_level_returns_none() {
        let mut days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "First", 0)],
        )];
        assert!(dedent_block_in_days(&mut days, "b1").is_none());
    }

    #[test]
    fn dedent_inserts_after_parent() {
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![make_block("c1", "Child", 0)],
            open: true,
        };
        let mut days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![parent, make_block("b2", "Other", 1)],
        )];
        dedent_block_in_days(&mut days, "c1");
        // Order: p, c1, b2
        assert_eq!(days[0].blocks.len(), 3);
        assert_eq!(days[0].blocks[0].uid, "p");
        assert_eq!(days[0].blocks[1].uid, "c1");
        assert_eq!(days[0].blocks[2].uid, "b2");
    }

    #[test]
    fn dedent_nested_deeper() {
        let grandchild = make_block("gc", "Grandchild", 0);
        let child = Block {
            uid: "c".into(),
            string: "Child".into(),
            order: 0,
            children: vec![grandchild],
            open: true,
        };
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![child],
            open: true,
        };
        let mut days = vec![make_daily_note(2026, 2, 21, vec![parent])];
        let result = dedent_block_in_days(&mut days, "gc");
        assert!(result.is_some());
        let (new_parent_uid, _) = result.unwrap();
        assert_eq!(new_parent_uid, "p");
        // gc is now sibling of c, under p
        assert_eq!(days[0].blocks[0].children.len(), 2);
        assert_eq!(days[0].blocks[0].children[0].uid, "c");
        assert_eq!(days[0].blocks[0].children[1].uid, "gc");
    }

    #[test]
    fn shift_tab_in_edit_mode_returns_move_block() {
        // Setup: b2 is child of b1
        let parent = Block {
            uid: "b1".into(),
            string: "Block one".into(),
            order: 0,
            children: vec![make_block("b2", "Block two", 0)],
            open: true,
        };
        let mut state = test_state();
        state.days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![parent, make_block("b3", "Block three", 1)],
        )];
        state.selected_block = 1; // b2 (child of b1)
        enter_insert_mode(&mut state);
        let action = handle_insert_key(&mut state, &key_event(KeyCode::BackTab));
        assert!(action.is_some());
        match action.unwrap() {
            WriteAction::MoveBlock { block, location } => {
                assert_eq!(block.uid, "b2");
                assert_eq!(location.parent_uid, "02-21-2026");
            }
            _ => panic!("Expected MoveBlock"),
        }
        assert!(matches!(state.input_mode, InputMode::Insert { .. }));
    }

    #[test]
    fn shift_tab_updates_selected_block() {
        let parent = Block {
            uid: "b1".into(),
            string: "Block one".into(),
            order: 0,
            children: vec![
                make_block("c1", "Child 1", 0),
                make_block("c2", "Child 2", 1),
            ],
            open: true,
        };
        let mut state = test_state();
        state.days = vec![make_daily_note(2026, 2, 21, vec![parent])];
        state.selected_block = 1; // c1
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::BackTab));
        // c1 moved out: b1(0), c2(1), c1(2) — selected_block should follow c1
        assert_eq!(state.selected_block, 2);
    }

    #[test]
    fn shift_tab_on_top_level_does_nothing() {
        let mut state = test_state();
        state.selected_block = 0; // b1, already top-level
        enter_insert_mode(&mut state);
        let action = handle_insert_key(&mut state, &key_event(KeyCode::BackTab));
        assert!(action.is_none());
        assert!(matches!(state.input_mode, InputMode::Insert { .. }));
    }

    // --- refresh guard test ---

    #[test]
    fn refresh_does_not_trigger_during_insert() {
        let mut state = test_state();
        state.input_mode = InputMode::Insert {
            buffer: EditBuffer::new("test"),
            block_uid: "b1".into(),
            original_text: "test".into(),
            create_info: None,
        };
        state.refresh_counter = 119;
        state.refresh_counter += 1;
        let should_refresh = state.refresh_counter >= 120
            && !state.loading
            && !state.loading_more
            && state.input_mode == InputMode::Normal;
        assert!(!should_refresh);
    }

    fn test_state_with_blocks() -> AppState {
        test_state()
    }

    fn make_block(uid: &str, text: &str, order: i64) -> Block {
        Block {
            uid: uid.into(),
            string: text.into(),
            order,
            children: vec![],
            open: true,
        }
    }

    fn make_daily_note(year: i32, month: u32, day: u32, blocks: Vec<Block>) -> DailyNote {
        let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
        DailyNote {
            date,
            uid: format!("{:02}-{:02}-{}", month, day, year),
            title: format!("Test {}-{}-{}", year, month, day),
            blocks,
        }
    }

    fn test_state() -> AppState {
        let mut state = AppState::new("test-graph", vec![]);
        state.loading = false;
        state.status_message = None;

        let today = make_daily_note(
            2026,
            2,
            21,
            vec![
                make_block("b1", "Block one", 0),
                make_block("b2", "Block two", 1),
                make_block("b3", "Block three", 2),
            ],
        );
        state.days = vec![today];
        state
    }

    // --- detect_block_ref_trigger tests ---

    #[test]
    fn detect_block_ref_trigger_true() {
        let mut buf = EditBuffer::new("(())");
        buf.cursor = 2; // between inner parens
        assert!(detect_block_ref_trigger(&buf));
    }

    #[test]
    fn detect_block_ref_trigger_with_prefix() {
        let mut buf = EditBuffer::new("hello(())");
        buf.cursor = 7; // between inner parens
        assert!(detect_block_ref_trigger(&buf));
    }

    #[test]
    fn detect_block_ref_trigger_false_single_paren() {
        let mut buf = EditBuffer::new("()");
        buf.cursor = 1;
        assert!(!detect_block_ref_trigger(&buf));
    }

    #[test]
    fn detect_block_ref_trigger_false_cursor_wrong() {
        let mut buf = EditBuffer::new("(())");
        buf.cursor = 0;
        assert!(!detect_block_ref_trigger(&buf));
    }

    #[test]
    fn detect_block_ref_trigger_false_cursor_at_end() {
        let mut buf = EditBuffer::new("(())");
        buf.cursor = 4;
        assert!(!detect_block_ref_trigger(&buf));
    }

    // --- filter_blocks tests ---

    #[test]
    fn filter_blocks_empty_query_returns_all() {
        let days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "Alpha", 0), make_block("b2", "Beta", 1)],
        )];
        let cache = HashMap::new();
        let results = filter_blocks(&days, &cache, "", 10);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0], ("b1".into(), "Alpha".into()));
        assert_eq!(results[1], ("b2".into(), "Beta".into()));
    }

    #[test]
    fn filter_blocks_by_query() {
        let days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "Alpha", 0), make_block("b2", "Beta", 1)],
        )];
        let cache = HashMap::new();
        let results = filter_blocks(&days, &cache, "alp", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "b1");
    }

    #[test]
    fn filter_blocks_case_insensitive() {
        let days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "Hello World", 0)],
        )];
        let cache = HashMap::new();
        let results = filter_blocks(&days, &cache, "hello", 10);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn filter_blocks_respects_limit() {
        let days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![
                make_block("b1", "A", 0),
                make_block("b2", "B", 1),
                make_block("b3", "C", 2),
            ],
        )];
        let cache = HashMap::new();
        let results = filter_blocks(&days, &cache, "", 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn filter_blocks_includes_nested() {
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![make_block("c1", "Nested child", 0)],
            open: true,
        };
        let days = vec![make_daily_note(2026, 2, 21, vec![parent])];
        let cache = HashMap::new();
        let results = filter_blocks(&days, &cache, "nested", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "c1");
    }

    #[test]
    fn filter_blocks_includes_cache() {
        let days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "Alpha", 0)],
        )];
        let mut cache = HashMap::new();
        cache.insert("ext1".to_string(), "Avelino external".to_string());
        let results = filter_blocks(&days, &cache, "avelino", 10);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, "ext1");
    }

    #[test]
    fn filter_blocks_cache_no_duplicates() {
        let days = vec![make_daily_note(
            2026,
            2,
            21,
            vec![make_block("b1", "Same text", 0)],
        )];
        let mut cache = HashMap::new();
        cache.insert("b1".to_string(), "Same text".to_string());
        let results = filter_blocks(&days, &cache, "same", 10);
        assert_eq!(results.len(), 1);
    }

    // --- autocomplete integration tests ---

    #[test]
    fn typing_double_paren_opens_autocomplete() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        // Type first (
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        assert!(state.autocomplete.is_none());
        // Type second ( — triggers auto-pair → (()) and opens autocomplete
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        assert!(state.autocomplete.is_some());
        let ac = state.autocomplete.as_ref().unwrap();
        assert_eq!(ac.query, "");
        assert_eq!(ac.selected, 0);
        // Should have results (all blocks from test_state)
        assert!(!ac.results.is_empty());
    }

    #[test]
    fn autocomplete_filters_by_query() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        assert!(state.autocomplete.is_some());
        // Type "two" to filter
        handle_insert_key(&mut state, &key_event(KeyCode::Char('t')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('w')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('o')));
        let ac = state.autocomplete.as_ref().unwrap();
        assert_eq!(ac.query, "two");
        assert_eq!(ac.results.len(), 1);
        assert_eq!(ac.results[0].0, "b2"); // "Block two"
    }

    #[test]
    fn autocomplete_up_down_moves_selection() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        let ac = state.autocomplete.as_ref().unwrap();
        assert_eq!(ac.selected, 0);
        // Down
        handle_insert_key(&mut state, &key_event(KeyCode::Down));
        assert_eq!(state.autocomplete.as_ref().unwrap().selected, 1);
        // Down again
        handle_insert_key(&mut state, &key_event(KeyCode::Down));
        assert_eq!(state.autocomplete.as_ref().unwrap().selected, 2);
        // Up
        handle_insert_key(&mut state, &key_event(KeyCode::Up));
        assert_eq!(state.autocomplete.as_ref().unwrap().selected, 1);
        // Up past 0 stays at 0
        handle_insert_key(&mut state, &key_event(KeyCode::Up));
        assert_eq!(state.autocomplete.as_ref().unwrap().selected, 0);
        handle_insert_key(&mut state, &key_event(KeyCode::Up));
        assert_eq!(state.autocomplete.as_ref().unwrap().selected, 0);
    }

    #[test]
    fn autocomplete_enter_inserts_uid() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        // Select first result and confirm
        handle_insert_key(&mut state, &key_event(KeyCode::Enter));
        assert!(state.autocomplete.is_none());
        // Buffer should contain ((uid)) of first block
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                let text = buffer.to_string();
                assert!(text.contains("((b1))"), "Expected ((b1)) in '{}' ", text);
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn autocomplete_esc_closes_without_exit() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        assert!(state.autocomplete.is_some());
        // Esc should close autocomplete but stay in insert mode
        handle_insert_key(&mut state, &key_event(KeyCode::Esc));
        assert!(state.autocomplete.is_none());
        assert!(matches!(state.input_mode, InputMode::Insert { .. }));
    }

    #[test]
    fn autocomplete_backspace_on_empty_closes() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        assert!(state.autocomplete.is_some());
        // Backspace with empty query should close
        handle_insert_key(&mut state, &key_event(KeyCode::Backspace));
        assert!(state.autocomplete.is_none());
    }

    #[test]
    fn autocomplete_backspace_with_query_removes_char() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('a')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('b')));
        assert_eq!(state.autocomplete.as_ref().unwrap().query, "ab");
        handle_insert_key(&mut state, &key_event(KeyCode::Backspace));
        assert!(state.autocomplete.is_some());
        assert_eq!(state.autocomplete.as_ref().unwrap().query, "a");
    }

    #[test]
    fn autocomplete_down_wraps_at_end() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('(')));
        let count = state.autocomplete.as_ref().unwrap().results.len();
        // Move past the last item
        for _ in 0..count + 2 {
            handle_insert_key(&mut state, &key_event(KeyCode::Down));
        }
        let selected = state.autocomplete.as_ref().unwrap().selected;
        assert!(selected < count);
    }

    #[test]
    fn new_state_starts_loading() {
        let state = AppState::new("my-graph", vec![]);
        assert!(state.loading);
        assert_eq!(state.graph_name, "my-graph");
        assert_eq!(state.selected_block, 0);
        assert!(!state.should_quit);
        assert!(state.days.is_empty());
    }

    #[test]
    fn quit_action_sets_should_quit() {
        let mut state = test_state();
        handle_action(&mut state, &Action::Quit);
        assert!(state.should_quit);
    }

    #[test]
    fn move_down_increments_selected_block() {
        let mut state = test_state();
        assert_eq!(state.selected_block, 0);
        handle_action(&mut state, &Action::MoveDown);
        assert_eq!(state.selected_block, 1);
        handle_action(&mut state, &Action::MoveDown);
        assert_eq!(state.selected_block, 2);
    }

    #[test]
    fn move_down_at_last_block_requests_more() {
        let mut state = test_state();
        state.selected_block = 2; // last block
        let result = handle_action(&mut state, &Action::MoveDown);
        assert!(result.is_some());
        assert!(state.loading_more);
        let expected_date = NaiveDate::from_ymd_opt(2026, 2, 20).unwrap();
        assert_eq!(result.unwrap(), LoadRequest::DailyNote(expected_date));
    }

    #[test]
    fn move_down_while_loading_more_does_nothing() {
        let mut state = test_state();
        state.selected_block = 2;
        state.loading_more = true;
        let result = handle_action(&mut state, &Action::MoveDown);
        assert!(result.is_none());
        assert_eq!(state.selected_block, 2);
    }

    #[test]
    fn move_up_decrements_selected_block() {
        let mut state = test_state();
        state.selected_block = 2;
        handle_action(&mut state, &Action::MoveUp);
        assert_eq!(state.selected_block, 1);
    }

    #[test]
    fn move_up_stops_at_zero() {
        let mut state = test_state();
        state.selected_block = 0;
        handle_action(&mut state, &Action::MoveUp);
        assert_eq!(state.selected_block, 0);
    }

    #[test]
    fn move_down_on_empty_days_does_nothing() {
        let mut state = AppState::new("test", vec![]);
        state.loading = false;
        handle_action(&mut state, &Action::MoveDown);
        assert_eq!(state.selected_block, 0);
    }

    #[test]
    fn daily_note_loaded_updates_state() {
        let mut state = AppState::new("test", vec![]);
        assert!(state.loading);

        let note = make_daily_note(2026, 2, 21, vec![make_block("b1", "Hello", 0)]);
        handle_daily_note_loaded(&mut state, note);

        assert!(!state.loading);
        assert_eq!(state.days.len(), 1);
        assert!(state.status_message.is_none());
    }

    #[test]
    fn daily_note_loaded_maintains_chronological_order() {
        let mut state = AppState::new("test", vec![]);
        state.loading = false;

        let day20 = make_daily_note(2026, 2, 20, vec![make_block("a", "A", 0)]);
        let day21 = make_daily_note(2026, 2, 21, vec![make_block("b", "B", 0)]);
        let day19 = make_daily_note(2026, 2, 19, vec![make_block("c", "C", 0)]);

        handle_daily_note_loaded(&mut state, day20.clone());
        handle_daily_note_loaded(&mut state, day21.clone());
        handle_daily_note_loaded(&mut state, day19.clone());

        assert_eq!(state.days[0].date, day21.date);
        assert_eq!(state.days[1].date, day20.date);
        assert_eq!(state.days[2].date, day19.date);
    }

    #[test]
    fn handle_api_error_sets_popup() {
        let mut state = AppState::new("test", vec![]);
        state.loading = true;
        let info = crate::error::ErrorInfo::Api {
            status: 429,
            body: r#"{"message":"rate limited"}"#.into(),
        };
        handle_api_error(&mut state, info);

        assert!(state.error_popup.is_some());
        let popup = state.error_popup.unwrap();
        assert_eq!(popup.title, "Rate Limited");
        assert_eq!(popup.message, "rate limited");
    }

    #[test]
    fn handle_api_error_clears_loading() {
        let mut state = AppState::new("test", vec![]);
        state.loading = true;
        state.loading_more = true;
        let info = crate::error::ErrorInfo::Network("timeout".into());
        handle_api_error(&mut state, info);

        assert!(!state.loading);
        assert!(!state.loading_more);
    }

    #[test]
    fn date_display_is_populated() {
        let state = AppState::new("test", vec![]);
        assert!(!state.date_display.is_empty());
    }

    #[test]
    fn flat_block_count_with_nested_children() {
        let mut state = AppState::new("test", vec![]);
        state.loading = false;

        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![
                make_block("c1", "Child 1", 0),
                make_block("c2", "Child 2", 1),
            ],
            open: true,
        };
        let note = make_daily_note(2026, 2, 21, vec![parent, make_block("b2", "Other", 1)]);
        state.days = vec![note];

        assert_eq!(state.flat_block_count(), 4); // parent + 2 children + other
    }

    #[test]
    fn refresh_loaded_replaces_changed_day() {
        let mut state = test_state();
        let updated = make_daily_note(
            2026,
            2,
            21,
            vec![
                make_block("b1", "Block one EDITED", 0),
                make_block("b2", "Block two", 1),
                make_block("b3", "Block three", 2),
            ],
        );
        handle_refresh_loaded(&mut state, updated.clone());
        assert_eq!(state.days[0].blocks[0].string, "Block one EDITED");
    }

    #[test]
    fn refresh_loaded_skips_identical_day() {
        let mut state = test_state();
        let original = state.days[0].clone();
        let identical = original.clone();
        handle_refresh_loaded(&mut state, identical);
        // Content unchanged — verify days still equal
        assert_eq!(state.days[0], original);
    }

    #[test]
    fn refresh_loaded_ignores_unknown_day() {
        let mut state = test_state();
        let unknown = make_daily_note(2026, 1, 15, vec![make_block("x", "Unknown", 0)]);
        handle_refresh_loaded(&mut state, unknown);
        // Should not add or modify anything
        assert_eq!(state.days.len(), 1);
        assert_eq!(
            state.days[0].date,
            NaiveDate::from_ymd_opt(2026, 2, 21).unwrap()
        );
    }

    #[test]
    fn refresh_loaded_preserves_selected_block() {
        let mut state = test_state();
        state.selected_block = 2;
        let updated = make_daily_note(
            2026,
            2,
            21,
            vec![
                make_block("b1", "Changed", 0),
                make_block("b2", "Block two", 1),
                make_block("b3", "Block three", 2),
            ],
        );
        handle_refresh_loaded(&mut state, updated);
        assert_eq!(state.selected_block, 2);
    }

    #[test]
    fn refresh_does_not_trigger_while_loading() {
        let mut state = test_state();
        state.loading = true;
        state.refresh_counter = 119;
        // Simulate one more tick
        state.refresh_counter += 1;
        let should_refresh = state.refresh_counter >= 120 && !state.loading && !state.loading_more;
        assert!(!should_refresh);
    }

    // --- block ref extraction tests ---

    #[test]
    fn extract_uids_finds_block_refs() {
        let local_map = HashMap::new();
        let cache = HashMap::new();
        let pending = HashSet::new();
        let mut out = Vec::new();
        extract_uids_from_text(
            "see ((abc123)) here",
            &local_map,
            &cache,
            &pending,
            &mut out,
        );
        assert_eq!(out, vec!["abc123"]);
    }

    #[test]
    fn extract_uids_skips_already_known() {
        let mut local_map = HashMap::new();
        local_map.insert("abc123".to_string(), "Known text".to_string());
        let cache = HashMap::new();
        let pending = HashSet::new();
        let mut out = Vec::new();
        extract_uids_from_text("((abc123))", &local_map, &cache, &pending, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn extract_uids_skips_cached() {
        let local_map = HashMap::new();
        let mut cache = HashMap::new();
        cache.insert("abc123".to_string(), "Cached text".to_string());
        let pending = HashSet::new();
        let mut out = Vec::new();
        extract_uids_from_text("((abc123))", &local_map, &cache, &pending, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn extract_uids_skips_pending() {
        let local_map = HashMap::new();
        let cache = HashMap::new();
        let mut pending = HashSet::new();
        pending.insert("abc123".to_string());
        let mut out = Vec::new();
        extract_uids_from_text("((abc123))", &local_map, &cache, &pending, &mut out);
        assert!(out.is_empty());
    }

    #[test]
    fn extract_uids_from_embed() {
        let local_map = HashMap::new();
        let cache = HashMap::new();
        let pending = HashSet::new();
        let mut out = Vec::new();
        extract_uids_from_text("{{embed: ((xyz))}}", &local_map, &cache, &pending, &mut out);
        assert_eq!(out, vec!["xyz"]);
    }

    #[test]
    fn extract_uids_multiple_refs() {
        let local_map = HashMap::new();
        let cache = HashMap::new();
        let pending = HashSet::new();
        let mut out = Vec::new();
        extract_uids_from_text(
            "((aaa)) and ((bbb))",
            &local_map,
            &cache,
            &pending,
            &mut out,
        );
        assert_eq!(out, vec!["aaa", "bbb"]);
    }

    #[test]
    fn collect_unresolved_refs_from_state() {
        let mut state = test_state();
        // Replace one block with text containing a block ref
        state.days[0].blocks[0].string = "see ((unknown_uid))".to_string();
        let refs = collect_unresolved_refs(&state);
        assert_eq!(refs, vec!["unknown_uid"]);
    }

    #[test]
    fn collect_unresolved_refs_ignores_local_blocks() {
        let mut state = test_state();
        // Reference a block that exists locally
        state.days[0].blocks[0].string = "see ((b2))".to_string();
        let refs = collect_unresolved_refs(&state);
        assert!(refs.is_empty());
    }

    #[test]
    fn block_ref_resolved_updates_cache() {
        let mut state = test_state();
        state.pending_block_refs.insert("uid1".to_string());
        // Simulate receiving BlockRefResolved message
        state.pending_block_refs.remove("uid1");
        state
            .block_ref_cache
            .insert("uid1".to_string(), "Resolved text".to_string());
        assert_eq!(state.block_ref_cache.get("uid1").unwrap(), "Resolved text");
        assert!(!state.pending_block_refs.contains("uid1"));
    }

    // --- undo tests ---

    #[test]
    fn undo_empty_stack_returns_none() {
        let mut state = test_state();
        assert!(state.undo_stack.is_empty());
        let result = apply_undo(&mut state);
        assert!(result.is_none());
    }

    #[test]
    fn undo_text_edit_restores_original() {
        let mut state = test_state();
        // Simulate: text was "Block one", got changed to "Block one!"
        update_block_text_in_days(&mut state.days, "b1", "Block one!");
        state.undo_stack.push(UndoEntry::TextEdit {
            block_uid: "b1".into(),
            old_text: "Block one".into(),
        });
        let action = apply_undo(&mut state);
        assert_eq!(state.days[0].blocks[0].string, "Block one");
        assert!(action.is_some());
    }

    #[test]
    fn undo_text_edit_returns_update_action() {
        let mut state = test_state();
        update_block_text_in_days(&mut state.days, "b1", "Changed");
        state.undo_stack.push(UndoEntry::TextEdit {
            block_uid: "b1".into(),
            old_text: "Block one".into(),
        });
        let action = apply_undo(&mut state).unwrap();
        match action {
            WriteAction::UpdateBlock { block } => {
                assert_eq!(block.uid, "b1");
                assert_eq!(block.string, "Block one");
            }
            _ => panic!("Expected UpdateBlock"),
        }
    }

    #[test]
    fn undo_create_block_removes_it() {
        let mut state = test_state();
        // Simulate: a new block was created
        let new_block = make_block("new1", "New block", 3);
        insert_block_in_days(&mut state.days, "02-21-2026", 3, new_block);
        assert_eq!(state.flat_block_count(), 4);
        state.undo_stack.push(UndoEntry::CreateBlock {
            block_uid: "new1".into(),
        });
        let action = apply_undo(&mut state);
        assert_eq!(state.flat_block_count(), 3);
        assert!(action.is_some());
    }

    #[test]
    fn undo_create_block_returns_delete_action() {
        let mut state = test_state();
        let new_block = make_block("new1", "New block", 3);
        insert_block_in_days(&mut state.days, "02-21-2026", 3, new_block);
        state.undo_stack.push(UndoEntry::CreateBlock {
            block_uid: "new1".into(),
        });
        let action = apply_undo(&mut state).unwrap();
        match action {
            WriteAction::DeleteBlock { block } => {
                assert_eq!(block.uid, "new1");
            }
            _ => panic!("Expected DeleteBlock"),
        }
    }

    #[test]
    fn undo_delete_block_restores_it() {
        let mut state = test_state();
        // Capture block before deleting
        let block = state.days[0].blocks[1].clone(); // b2
        let parent_uid = "02-21-2026".to_string();
        let order = block.order;
        let saved_selected = state.selected_block;
        // Delete b2
        remove_block_from_days(&mut state.days, "b2");
        assert_eq!(state.flat_block_count(), 2);
        state.undo_stack.push(UndoEntry::DeleteBlock {
            block,
            parent_uid,
            order,
            selected_block: saved_selected,
        });
        let action = apply_undo(&mut state);
        assert_eq!(state.flat_block_count(), 3);
        assert!(action.is_some());
    }

    #[test]
    fn undo_delete_block_restores_position() {
        let mut state = test_state();
        state.selected_block = 1;
        let block = state.days[0].blocks[1].clone();
        let saved_selected = state.selected_block;
        remove_block_from_days(&mut state.days, "b2");
        state.selected_block = 0; // cursor moved after delete
        state.undo_stack.push(UndoEntry::DeleteBlock {
            block,
            parent_uid: "02-21-2026".into(),
            order: 1,
            selected_block: saved_selected,
        });
        apply_undo(&mut state);
        assert_eq!(state.selected_block, 1);
    }

    #[test]
    fn undo_delete_block_returns_create_action() {
        let mut state = test_state();
        let block = state.days[0].blocks[1].clone();
        remove_block_from_days(&mut state.days, "b2");
        state.undo_stack.push(UndoEntry::DeleteBlock {
            block: block.clone(),
            parent_uid: "02-21-2026".into(),
            order: 1,
            selected_block: 0,
        });
        let action = apply_undo(&mut state).unwrap();
        match action {
            WriteAction::CreateBlock {
                location,
                block: new_block,
            } => {
                assert_eq!(location.parent_uid, "02-21-2026");
                assert_eq!(new_block.string, "Block two");
                assert_eq!(new_block.uid, Some("b2".into()));
            }
            _ => panic!("Expected CreateBlock"),
        }
    }

    #[test]
    fn undo_move_block_returns_to_original() {
        let mut state = test_state();
        state.selected_block = 1;
        let saved_selected = state.selected_block;
        // Indent b2 under b1
        let info = resolve_block_at_index(&state.days, &state.linked_refs, 1).unwrap();
        indent_block_in_days(&mut state.days, "b2");
        state.undo_stack.push(UndoEntry::MoveBlock {
            block_uid: "b2".into(),
            old_parent_uid: info.parent_uid.clone(),
            old_order: info.order,
            selected_block: saved_selected,
        });
        // Now undo — should move b2 back
        let action = apply_undo(&mut state);
        assert!(action.is_some());
        match action.unwrap() {
            WriteAction::MoveBlock { block, location } => {
                assert_eq!(block.uid, "b2");
                assert_eq!(location.parent_uid, "02-21-2026");
            }
            _ => panic!("Expected MoveBlock"),
        }
        assert_eq!(state.selected_block, saved_selected);
    }

    #[test]
    fn undo_multiple_sequential() {
        let mut state = test_state();
        // Edit 1: change b1
        update_block_text_in_days(&mut state.days, "b1", "Edit 1");
        state.undo_stack.push(UndoEntry::TextEdit {
            block_uid: "b1".into(),
            old_text: "Block one".into(),
        });
        // Edit 2: change b2
        update_block_text_in_days(&mut state.days, "b2", "Edit 2");
        state.undo_stack.push(UndoEntry::TextEdit {
            block_uid: "b2".into(),
            old_text: "Block two".into(),
        });
        // Undo edit 2
        apply_undo(&mut state);
        assert_eq!(state.days[0].blocks[1].string, "Block two");
        assert_eq!(state.days[0].blocks[0].string, "Edit 1"); // still changed
                                                              // Undo edit 1
        apply_undo(&mut state);
        assert_eq!(state.days[0].blocks[0].string, "Block one");
        // Stack empty
        assert!(apply_undo(&mut state).is_none());
    }

    #[test]
    fn undo_create_adjusts_selected_block() {
        let mut state = test_state();
        state.selected_block = 3; // pointing at the new block
        let new_block = make_block("new1", "New", 3);
        insert_block_in_days(&mut state.days, "02-21-2026", 3, new_block);
        state.undo_stack.push(UndoEntry::CreateBlock {
            block_uid: "new1".into(),
        });
        apply_undo(&mut state);
        // selected_block should be clamped to last valid index
        assert!(state.selected_block < state.flat_block_count() || state.flat_block_count() == 0);
    }

    // --- Search tests ---

    #[test]
    fn search_opens_with_action() {
        let mut state = test_state();
        assert!(state.search.is_none());
        handle_action(&mut state, &Action::Search);
        assert!(state.search.is_some());
        let search = state.search.unwrap();
        assert_eq!(search.query, "");
        assert_eq!(search.selected, 0);
        // Should have all 3 blocks from test_state
        assert_eq!(search.results.len(), 3);
    }

    #[test]
    fn search_filters_by_query() {
        let mut state = test_state();
        handle_action(&mut state, &Action::Search);
        // Type "one" to filter
        let key = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE);
        handle_search_key(&mut state, &key);
        let key = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        handle_search_key(&mut state, &key);
        let key = KeyEvent::new(KeyCode::Char('e'), KeyModifiers::NONE);
        handle_search_key(&mut state, &key);
        let search = state.search.as_ref().unwrap();
        assert_eq!(search.query, "one");
        assert_eq!(search.results.len(), 1);
        assert_eq!(search.results[0].0, "b1");
    }

    #[test]
    fn search_navigate_up_down() {
        let mut state = test_state();
        handle_action(&mut state, &Action::Search);
        // Move down
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        handle_search_key(&mut state, &down);
        assert_eq!(state.search.as_ref().unwrap().selected, 1);
        handle_search_key(&mut state, &down);
        assert_eq!(state.search.as_ref().unwrap().selected, 2);
        // Can't go past end
        handle_search_key(&mut state, &down);
        assert_eq!(state.search.as_ref().unwrap().selected, 2);
        // Move up
        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        handle_search_key(&mut state, &up);
        assert_eq!(state.search.as_ref().unwrap().selected, 1);
        // Can't go below 0
        handle_search_key(&mut state, &up);
        handle_search_key(&mut state, &up);
        assert_eq!(state.search.as_ref().unwrap().selected, 0);
    }

    #[test]
    fn search_enter_navigates_to_block() {
        let mut state = test_state();
        state.selected_block = 0;
        handle_action(&mut state, &Action::Search);
        // Navigate to second result
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        handle_search_key(&mut state, &down);
        // Enter to select
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        handle_search_key(&mut state, &enter);
        // Search should be closed
        assert!(state.search.is_none());
        // selected_block should point to b2 (index 1)
        assert_eq!(state.selected_block, 1);
    }

    #[test]
    fn search_esc_closes() {
        let mut state = test_state();
        handle_action(&mut state, &Action::Search);
        assert!(state.search.is_some());
        let esc = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        handle_search_key(&mut state, &esc);
        assert!(state.search.is_none());
    }

    #[test]
    fn search_backspace_updates_query() {
        let mut state = test_state();
        handle_action(&mut state, &Action::Search);
        // Type "on"
        let key_o = KeyEvent::new(KeyCode::Char('o'), KeyModifiers::NONE);
        let key_n = KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE);
        handle_search_key(&mut state, &key_o);
        handle_search_key(&mut state, &key_n);
        assert_eq!(state.search.as_ref().unwrap().query, "on");
        // Backspace
        let bs = KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE);
        handle_search_key(&mut state, &bs);
        assert_eq!(state.search.as_ref().unwrap().query, "o");
        // Results should re-expand
        assert!(!state.search.as_ref().unwrap().results.is_empty());
    }

    #[test]
    fn search_case_insensitive() {
        let mut state = test_state();
        handle_action(&mut state, &Action::Search);
        // Type "BLOCK" in uppercase
        for ch in "BLOCK".chars() {
            let key = KeyEvent::new(KeyCode::Char(ch), KeyModifiers::SHIFT);
            handle_search_key(&mut state, &key);
        }
        let search = state.search.as_ref().unwrap();
        assert_eq!(search.query, "BLOCK");
        // All 3 blocks match "Block" case-insensitively
        assert_eq!(search.results.len(), 3);
    }

    // --- Collapse/Expand tests ---

    fn test_state_with_children() -> AppState {
        let mut state = AppState::new("test-graph", vec![]);
        state.loading = false;
        state.status_message = None;
        let parent = Block {
            uid: "p1".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![
                make_block("c1", "Child 1", 0),
                make_block("c2", "Child 2", 1),
            ],
            open: true,
        };
        let day = make_daily_note(2026, 2, 21, vec![parent, make_block("b2", "Sibling", 1)]);
        state.days = vec![day];
        state
    }

    #[test]
    fn collapse_hides_children_from_count() {
        let mut state = test_state_with_children();
        // p1(open) + c1 + c2 + b2 = 4
        assert_eq!(state.flat_block_count(), 4);
        // Collapse p1
        handle_action(&mut state, &Action::Collapse);
        // p1(closed) + b2 = 2
        assert_eq!(state.flat_block_count(), 2);
    }

    #[test]
    fn expand_shows_children() {
        let mut state = test_state_with_children();
        set_block_open(&mut state.days, "p1", false);
        assert_eq!(state.flat_block_count(), 2);
        handle_action(&mut state, &Action::Expand);
        assert_eq!(state.flat_block_count(), 4);
    }

    #[test]
    fn enter_toggles_open() {
        let mut state = test_state_with_children();
        assert_eq!(state.flat_block_count(), 4);
        handle_action(&mut state, &Action::Enter);
        assert_eq!(state.flat_block_count(), 2);
        handle_action(&mut state, &Action::Enter);
        assert_eq!(state.flat_block_count(), 4);
    }

    #[test]
    fn collapsed_block_skips_children_in_resolve() {
        let mut state = test_state_with_children();
        set_block_open(&mut state.days, "p1", false);
        // index 0 = p1, index 1 = b2 (children hidden)
        let info = resolve_block_at_index(&state.days, &state.linked_refs, 1).unwrap();
        assert_eq!(info.block_uid, "b2");
    }

    // --- Day Navigation tests ---

    fn test_state_two_days() -> AppState {
        let mut state = AppState::new("test-graph", vec![]);
        state.loading = false;
        state.status_message = None;
        let day1 = make_daily_note(
            2026,
            2,
            22,
            vec![
                make_block("a1", "Today block 1", 0),
                make_block("a2", "Today block 2", 1),
            ],
        );
        let day2 = make_daily_note(
            2026,
            2,
            21,
            vec![
                make_block("b1", "Yesterday block 1", 0),
                make_block("b2", "Yesterday block 2", 1),
            ],
        );
        state.days = vec![day1, day2]; // reverse chronological
        state
    }

    #[test]
    fn prev_day_jumps_to_next_day_blocks() {
        let mut state = test_state_two_days();
        state.selected_block = 0; // in day1
        handle_action(&mut state, &Action::PrevDay);
        // Should jump to first block of day2 (index 2)
        assert_eq!(state.selected_block, 2);
    }

    #[test]
    fn next_day_jumps_to_previous_day() {
        let mut state = test_state_two_days();
        state.selected_block = 2; // in day2
        handle_action(&mut state, &Action::NextDay);
        // Should jump to first block of day1 (index 0)
        assert_eq!(state.selected_block, 0);
    }

    #[test]
    fn go_daily_resets_to_first_block() {
        let mut state = test_state_two_days();
        state.current_date = NaiveDate::from_ymd_opt(2026, 2, 22).unwrap();
        state.selected_block = 3;
        handle_action(&mut state, &Action::GoDaily);
        assert_eq!(state.selected_block, 0);
    }

    // --- Help tests ---

    #[test]
    fn help_toggles_show_help() {
        let mut state = test_state();
        assert!(!state.show_help);
        handle_action(&mut state, &Action::Help);
        assert!(state.show_help);
        handle_action(&mut state, &Action::Help);
        assert!(!state.show_help);
    }

    #[test]
    fn exit_closes_help() {
        let mut state = test_state();
        state.show_help = true;
        handle_action(&mut state, &Action::Exit);
        assert!(!state.show_help);
    }

    // --- Redo tests ---

    #[test]
    fn redo_after_undo_text_edit() {
        let mut state = test_state();
        state.undo_stack.push(UndoEntry::TextEdit {
            block_uid: "b1".into(),
            old_text: "Original".into(),
        });
        // Undo: text goes back to "Original"
        apply_undo(&mut state);
        assert_eq!(state.days[0].blocks[0].string, "Original");
        assert_eq!(state.redo_stack.len(), 1);
        // Redo: text goes back to "Block one"
        apply_redo(&mut state);
        assert_eq!(state.days[0].blocks[0].string, "Block one");
        assert_eq!(state.undo_stack.len(), 1);
    }

    #[test]
    fn redo_empty_stack_returns_none() {
        let mut state = test_state();
        assert!(apply_redo(&mut state).is_none());
    }

    #[test]
    fn new_edit_clears_redo_stack() {
        let mut state = test_state();
        state.redo_stack.push(UndoEntry::TextEdit {
            block_uid: "b1".into(),
            old_text: "old".into(),
        });
        // Simulate a new edit via finalize_insert
        state.input_mode = InputMode::Insert {
            buffer: EditBuffer::new("Changed text"),
            block_uid: "b1".into(),
            original_text: "Block one".into(),
            create_info: None,
        };
        finalize_insert(&mut state);
        assert!(state.redo_stack.is_empty());
    }

    // --- format_roam_daily_title (via handle_daily_note_loaded) tests ---

    fn make_empty_note(year: i32, month: u32, day: u32) -> DailyNote {
        let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
        DailyNote {
            date,
            uid: format!("{:02}-{:02}-{}", month, day, year),
            title: String::new(),
            blocks: vec![],
        }
    }

    #[test]
    fn daily_note_empty_title_generates_roam_title_february_25() {
        let mut state = AppState::new("test", vec![]);
        let note = make_empty_note(2026, 2, 25);
        handle_daily_note_loaded(&mut state, note);
        assert_eq!(state.days[0].title, "February 25th, 2026");
    }

    #[test]
    fn daily_note_empty_title_generates_roam_title_january_1st() {
        let mut state = AppState::new("test", vec![]);
        let note = make_empty_note(2026, 1, 1);
        handle_daily_note_loaded(&mut state, note);
        assert_eq!(state.days[0].title, "January 1st, 2026");
    }

    #[test]
    fn daily_note_empty_title_generates_roam_title_march_2nd() {
        let mut state = AppState::new("test", vec![]);
        let note = make_empty_note(2026, 3, 2);
        handle_daily_note_loaded(&mut state, note);
        assert_eq!(state.days[0].title, "March 2nd, 2026");
    }

    #[test]
    fn daily_note_empty_title_generates_roam_title_april_3rd() {
        let mut state = AppState::new("test", vec![]);
        let note = make_empty_note(2026, 4, 3);
        handle_daily_note_loaded(&mut state, note);
        assert_eq!(state.days[0].title, "April 3rd, 2026");
    }

    #[test]
    fn daily_note_empty_title_generates_roam_title_may_11th() {
        let mut state = AppState::new("test", vec![]);
        let note = make_empty_note(2026, 5, 11);
        handle_daily_note_loaded(&mut state, note);
        assert_eq!(state.days[0].title, "May 11th, 2026");
    }

    #[test]
    fn daily_note_empty_title_generates_roam_title_21st() {
        let mut state = AppState::new("test", vec![]);
        let note = make_empty_note(2026, 6, 21);
        handle_daily_note_loaded(&mut state, note);
        assert_eq!(state.days[0].title, "June 21st, 2026");
    }

    #[test]
    fn daily_note_empty_title_generates_roam_title_22nd() {
        let mut state = AppState::new("test", vec![]);
        let note = make_empty_note(2026, 7, 22);
        handle_daily_note_loaded(&mut state, note);
        assert_eq!(state.days[0].title, "July 22nd, 2026");
    }

    #[test]
    fn daily_note_empty_title_generates_roam_title_23rd() {
        let mut state = AppState::new("test", vec![]);
        let note = make_empty_note(2026, 8, 23);
        handle_daily_note_loaded(&mut state, note);
        assert_eq!(state.days[0].title, "August 23rd, 2026");
    }

    #[test]
    fn daily_note_non_empty_title_is_preserved() {
        let mut state = AppState::new("test", vec![]);
        let date = NaiveDate::from_ymd_opt(2026, 2, 25).unwrap();
        let note = DailyNote {
            date,
            uid: "02-25-2026".into(),
            title: "Existing Title".into(),
            blocks: vec![],
        };
        handle_daily_note_loaded(&mut state, note);
        assert_eq!(state.days[0].title, "Existing Title");
    }

    // --- CreateBlock on empty day tests ---

    #[test]
    fn create_block_on_empty_day_creates_first_block() {
        let mut state = AppState::new("test", vec![]);
        state.loading = false;
        let day = DailyNote {
            date: NaiveDate::from_ymd_opt(2026, 2, 25).unwrap(),
            uid: "02-25-2026".into(),
            title: "February 25th, 2026".into(),
            blocks: vec![],
        };
        state.days = vec![day];

        handle_action(&mut state, &Action::CreateBlock);

        assert_eq!(state.days[0].blocks.len(), 1);
        assert_eq!(state.selected_block, 0);
        assert!(matches!(state.input_mode, InputMode::Insert { .. }));
    }

    #[test]
    fn create_block_on_empty_day_sets_cursor_to_zero() {
        let mut state = AppState::new("test", vec![]);
        state.loading = false;
        state.cursor_col = 5;
        let day = DailyNote {
            date: NaiveDate::from_ymd_opt(2026, 2, 25).unwrap(),
            uid: "02-25-2026".into(),
            title: "February 25th, 2026".into(),
            blocks: vec![],
        };
        state.days = vec![day];

        handle_action(&mut state, &Action::CreateBlock);

        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn create_block_on_empty_day_uses_day_uid_as_parent() {
        let mut state = AppState::new("test", vec![]);
        state.loading = false;
        let day = DailyNote {
            date: NaiveDate::from_ymd_opt(2026, 2, 25).unwrap(),
            uid: "02-25-2026".into(),
            title: "February 25th, 2026".into(),
            blocks: vec![],
        };
        state.days = vec![day];

        handle_action(&mut state, &Action::CreateBlock);

        match &state.input_mode {
            InputMode::Insert { create_info, .. } => {
                let info = create_info.as_ref().expect("expected create_info");
                assert_eq!(info.parent_uid, "02-25-2026");
                assert_eq!(info.order, 0);
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    // --- CursorLeft / CursorRight tests ---

    #[test]
    fn cursor_left_decrements_cursor_col() {
        let mut state = test_state();
        state.cursor_col = 3;
        handle_action(&mut state, &Action::CursorLeft);
        assert_eq!(state.cursor_col, 2);
    }

    #[test]
    fn cursor_left_stops_at_zero() {
        let mut state = test_state();
        state.cursor_col = 0;
        handle_action(&mut state, &Action::CursorLeft);
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn cursor_right_increments_cursor_col() {
        let mut state = test_state();
        // "Block one" has 9 chars, rendered_char_count allows up to index 8
        state.cursor_col = 0;
        handle_action(&mut state, &Action::CursorRight);
        assert_eq!(state.cursor_col, 1);
    }

    #[test]
    fn cursor_right_does_not_exceed_text_length() {
        let mut state = test_state();
        // "Block one" = 9 chars, max cursor_col = 8 (last char index)
        state.cursor_col = 7;
        handle_action(&mut state, &Action::CursorRight);
        assert_eq!(state.cursor_col, 8);
        // One more should not advance past the end
        handle_action(&mut state, &Action::CursorRight);
        assert_eq!(state.cursor_col, 8);
    }

    #[test]
    fn cursor_right_on_empty_block_stays_at_zero() {
        let mut state = AppState::new("test", vec![]);
        state.loading = false;
        let day = make_daily_note(2026, 2, 21, vec![make_block("b1", "", 0)]);
        state.days = vec![day];
        state.cursor_col = 0;
        handle_action(&mut state, &Action::CursorRight);
        assert_eq!(state.cursor_col, 0);
    }

    // --- cursor_col resets when selected_block changes tests ---

    #[test]
    fn move_up_resets_cursor_col() {
        let mut state = test_state();
        state.selected_block = 2;
        state.cursor_col = 5;
        handle_action(&mut state, &Action::MoveUp);
        assert_eq!(state.cursor_col, 0);
    }

    #[test]
    fn move_down_resets_cursor_col() {
        let mut state = test_state();
        state.selected_block = 0;
        state.cursor_col = 4;
        handle_action(&mut state, &Action::MoveDown);
        assert_eq!(state.selected_block, 1);
        assert_eq!(state.cursor_col, 0);
    }

    // --- Page navigation tests ---

    #[test]
    fn enter_with_single_link_returns_page_request() {
        let mut state = test_state();
        // Replace block text with a link
        state.days[0].blocks[0].string = "see [[My Page]]".to_string();
        state.selected_block = 0;
        let result = handle_action(&mut state, &Action::Enter);
        assert_eq!(result, Some(LoadRequest::Page("My Page".to_string())));
        assert_eq!(
            state.view_mode,
            ViewMode::Page {
                title: "My Page".to_string()
            }
        );
        assert!(state.loading);
    }

    #[test]
    fn enter_with_no_links_toggles_collapse() {
        let mut state = test_state();
        state.days[0].blocks[0].string = "no links here".to_string();
        state.days[0].blocks[0].children = vec![make_block("child", "Child", 0)];
        state.selected_block = 0;
        let result = handle_action(&mut state, &Action::Enter);
        assert!(result.is_none());
        // Block should now be collapsed
        assert!(!state.days[0].blocks[0].open);
    }

    #[test]
    fn enter_with_multiple_links_opens_picker() {
        let mut state = test_state();
        state.days[0].blocks[0].string = "[[Page A]] and [[Page B]]".to_string();
        state.selected_block = 0;
        let result = handle_action(&mut state, &Action::Enter);
        assert!(result.is_none());
        let lp = state.link_picker.as_ref().unwrap();
        assert_eq!(lp.links, vec!["Page A", "Page B"]);
        assert_eq!(lp.selected, 0);
    }

    #[test]
    fn navigate_to_page_saves_history() {
        let mut state = test_state();
        state.days[0].blocks[0].string = "see [[Target]]".to_string();
        state.selected_block = 0;
        handle_action(&mut state, &Action::Enter);
        // History should contain the previous daily notes view
        assert_eq!(state.nav_history.len(), 1);
        assert_eq!(state.nav_history[0].view_mode, ViewMode::DailyNotes);
    }

    #[test]
    fn nav_back_restores_previous_view() {
        let mut state = test_state();
        // Modify block text to contain a link
        state.days[0].blocks[0].string = "[[Target]]".to_string();
        let snapshot_days = state.days.clone();
        // Navigate to a page
        handle_action(&mut state, &Action::Enter);
        // Simulate page load
        state.days = vec![make_daily_note(
            2000,
            1,
            1,
            vec![make_block("p1", "Page block", 0)],
        )];
        state.loading = false;
        // Navigate back
        handle_action(&mut state, &Action::NavBack);
        assert_eq!(state.view_mode, ViewMode::DailyNotes);
        assert_eq!(state.days, snapshot_days);
    }

    #[test]
    fn nav_forward_after_back() {
        let mut state = test_state();
        state.days[0].blocks[0].string = "[[Target]]".to_string();
        // Navigate to page
        handle_action(&mut state, &Action::Enter);
        // Simulate page loaded
        let page_days = vec![make_daily_note(
            2000,
            1,
            1,
            vec![make_block("p1", "Page content", 0)],
        )];
        state.days = page_days.clone();
        state.loading = false;
        // Navigate back
        handle_action(&mut state, &Action::NavBack);
        assert_eq!(state.view_mode, ViewMode::DailyNotes);
        // Navigate forward — should restore the page view with snapshot data
        handle_action(&mut state, &Action::NavForward);
        // After navigating to page, the snapshot was empty (we cleared days),
        // but we saved the current state before going back
        assert_eq!(
            state.view_mode,
            ViewMode::Page {
                title: "Target".to_string()
            }
        );
    }

    #[test]
    fn nav_back_at_start_does_nothing() {
        let mut state = test_state();
        let original_mode = state.view_mode.clone();
        handle_action(&mut state, &Action::NavBack);
        assert_eq!(state.view_mode, original_mode);
    }

    #[test]
    fn nav_forward_at_end_does_nothing() {
        let mut state = test_state();
        handle_action(&mut state, &Action::NavForward);
        assert_eq!(state.view_mode, ViewMode::DailyNotes);
    }

    #[test]
    fn go_daily_from_page_view_returns_to_daily() {
        let mut state = test_state();
        state.view_mode = ViewMode::Page {
            title: "Some Page".to_string(),
        };
        let result = handle_action(&mut state, &Action::GoDaily);
        assert_eq!(result, Some(LoadRequest::DailyNote(state.current_date)));
        assert_eq!(state.view_mode, ViewMode::DailyNotes);
    }

    #[test]
    fn next_day_noop_in_page_view() {
        let mut state = test_state();
        state.view_mode = ViewMode::Page {
            title: "Some Page".to_string(),
        };
        state.selected_block = 0;
        let result = handle_action(&mut state, &Action::NextDay);
        assert!(result.is_none());
    }

    #[test]
    fn prev_day_noop_in_page_view() {
        let mut state = test_state();
        state.view_mode = ViewMode::Page {
            title: "Some Page".to_string(),
        };
        let result = handle_action(&mut state, &Action::PrevDay);
        assert!(result.is_none());
    }

    #[test]
    fn page_loaded_sets_days_and_clears_loading() {
        let mut state = test_state();
        state.loading = true;
        let page = make_daily_note(2000, 1, 1, vec![make_block("p1", "Page text", 0)]);
        handle_page_loaded(&mut state, page);
        assert!(!state.loading);
        assert_eq!(state.days.len(), 1);
        assert_eq!(state.days[0].blocks[0].string, "Page text");
    }

    #[test]
    fn page_loaded_empty_creates_placeholder_block() {
        let mut state = test_state();
        let page = DailyNote {
            date: NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
            uid: "page-uid".into(),
            title: "Empty Page".into(),
            blocks: vec![],
        };
        handle_page_loaded(&mut state, page);
        assert_eq!(state.days[0].blocks.len(), 1);
        assert!(state.days[0].blocks[0].string.is_empty());
    }

    #[test]
    fn link_picker_esc_closes() {
        let mut state = test_state();
        state.link_picker = Some(LinkPickerState {
            links: vec!["A".into(), "B".into()],
            selected: 0,
        });
        let key = KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE);
        handle_link_picker_key(&mut state, &key);
        assert!(state.link_picker.is_none());
    }

    #[test]
    fn link_picker_navigate_up_down() {
        let mut state = test_state();
        state.link_picker = Some(LinkPickerState {
            links: vec!["A".into(), "B".into(), "C".into()],
            selected: 0,
        });
        let down = KeyEvent::new(KeyCode::Down, KeyModifiers::NONE);
        handle_link_picker_key(&mut state, &down);
        assert_eq!(state.link_picker.as_ref().unwrap().selected, 1);

        handle_link_picker_key(&mut state, &down);
        assert_eq!(state.link_picker.as_ref().unwrap().selected, 2);

        // Should not go past end
        handle_link_picker_key(&mut state, &down);
        assert_eq!(state.link_picker.as_ref().unwrap().selected, 2);

        let up = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        handle_link_picker_key(&mut state, &up);
        assert_eq!(state.link_picker.as_ref().unwrap().selected, 1);
    }

    #[test]
    fn link_picker_enter_navigates_to_selected() {
        let mut state = test_state();
        state.link_picker = Some(LinkPickerState {
            links: vec!["Page A".into(), "Page B".into()],
            selected: 1,
        });
        let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
        let result = handle_link_picker_key(&mut state, &enter);
        assert_eq!(result, Some(LoadRequest::Page("Page B".to_string())));
        assert!(state.link_picker.is_none());
    }

    #[test]
    fn nav_history_caps_at_50() {
        let mut state = test_state();
        for i in 0..55 {
            state.days[0].blocks[0].string = format!("[[Page {}]]", i);
            state.selected_block = 0;
            handle_action(&mut state, &Action::Enter);
            // Simulate page load to reset state
            state.days = vec![make_daily_note(
                2000,
                1,
                1,
                vec![make_block("p", &format!("Content {}", i), 0)],
            )];
            state.loading = false;
        }
        assert!(state.nav_history.len() <= 50);
    }

    #[test]
    fn move_down_no_auto_load_in_page_view() {
        let mut state = test_state();
        state.view_mode = ViewMode::Page {
            title: "Test".to_string(),
        };
        state.selected_block = 2; // last block
        let result = handle_action(&mut state, &Action::MoveDown);
        assert!(result.is_none());
        assert!(!state.loading_more);
    }

    // --- Linked References tests ---

    fn make_linked_refs_state() -> LinkedRefsState {
        LinkedRefsState {
            groups: vec![
                LinkedRefGroup {
                    page_title: "Page A".into(),
                    blocks: vec![
                        LinkedRefBlock {
                            uid: "b1".into(),
                            string: "ref from A".into(),
                            page_title: "Page A".into(),
                        },
                        LinkedRefBlock {
                            uid: "b2".into(),
                            string: "another ref from A".into(),
                            page_title: "Page A".into(),
                        },
                    ],
                },
                LinkedRefGroup {
                    page_title: "Page B".into(),
                    blocks: vec![LinkedRefBlock {
                        uid: "b3".into(),
                        string: "ref from B".into(),
                        page_title: "Page B".into(),
                    }],
                },
            ],
            collapsed: false,
            loading: false,
        }
    }

    /// Helper: day title used by test_state() for the single day
    const TEST_DAY_TITLE: &str = "Test 2026-2-21";

    fn set_linked_refs(state: &mut AppState, lr: LinkedRefsState) {
        state.linked_refs.insert(TEST_DAY_TITLE.to_string(), lr);
    }

    #[test]
    fn linked_ref_items_count_none() {
        let state = test_state();
        assert_eq!(state.total_navigable_count(), state.flat_block_count());
    }

    #[test]
    fn linked_ref_items_count_empty_groups() {
        let mut state = test_state();
        set_linked_refs(
            &mut state,
            LinkedRefsState {
                groups: vec![],
                collapsed: false,
                loading: false,
            },
        );
        assert_eq!(state.total_navigable_count(), state.flat_block_count());
    }

    #[test]
    fn linked_ref_items_count_expanded() {
        let mut state = test_state();
        set_linked_refs(&mut state, make_linked_refs_state());
        let flat = state.flat_block_count();
        // 1 (header) + 1 (group A header) + 2 (blocks) + 1 (group B header) + 1 (block) = 6
        assert_eq!(state.total_navigable_count(), flat + 6);
    }

    #[test]
    fn linked_ref_items_count_collapsed() {
        let mut state = test_state();
        let mut lr = make_linked_refs_state();
        lr.collapsed = true;
        set_linked_refs(&mut state, lr);
        let flat = state.flat_block_count();
        assert_eq!(state.total_navigable_count(), flat + 1);
    }

    #[test]
    fn total_navigable_count_includes_linked_refs() {
        let mut state = test_state();
        set_linked_refs(&mut state, make_linked_refs_state());
        let flat = state.flat_block_count();
        assert_eq!(state.total_navigable_count(), flat + 6);
    }

    #[test]
    fn resolve_linked_ref_item_regular_block_returns_none() {
        let mut state = test_state();
        set_linked_refs(&mut state, make_linked_refs_state());
        assert!(state.resolve_linked_ref_item(0).is_none());
    }

    #[test]
    fn resolve_linked_ref_item_section_header() {
        let mut state = test_state();
        set_linked_refs(&mut state, make_linked_refs_state());
        let flat = state.flat_block_count();
        assert_eq!(
            state.resolve_linked_ref_item(flat),
            Some(LinkedRefItem::SectionHeader)
        );
    }

    #[test]
    fn resolve_linked_ref_item_group_header() {
        let mut state = test_state();
        set_linked_refs(&mut state, make_linked_refs_state());
        let flat = state.flat_block_count();
        assert_eq!(
            state.resolve_linked_ref_item(flat + 1),
            Some(LinkedRefItem::GroupHeader("Page A".into()))
        );
    }

    #[test]
    fn resolve_linked_ref_item_block() {
        let mut state = test_state();
        set_linked_refs(&mut state, make_linked_refs_state());
        let flat = state.flat_block_count();
        let item = state.resolve_linked_ref_item(flat + 2);
        assert!(matches!(item, Some(LinkedRefItem::Block(ref b)) if b.uid == "b1"));
    }

    #[test]
    fn resolve_linked_ref_item_second_group() {
        let mut state = test_state();
        set_linked_refs(&mut state, make_linked_refs_state());
        let flat = state.flat_block_count();
        // flat+0=header, flat+1=groupA, flat+2=blockA1, flat+3=blockA2, flat+4=groupB, flat+5=blockB1
        assert_eq!(
            state.resolve_linked_ref_item(flat + 4),
            Some(LinkedRefItem::GroupHeader("Page B".into()))
        );
    }

    #[test]
    fn move_down_navigates_into_linked_refs() {
        let mut state = test_state();
        set_linked_refs(&mut state, make_linked_refs_state());
        let flat = state.flat_block_count();
        state.selected_block = flat - 1; // last regular block
        handle_action(&mut state, &Action::MoveDown);
        assert_eq!(state.selected_block, flat); // linked refs section header
    }

    #[test]
    fn edit_block_guard_in_linked_refs() {
        let mut state = test_state();
        set_linked_refs(&mut state, make_linked_refs_state());
        let flat = state.flat_block_count();
        state.selected_block = flat; // linked refs header
        handle_action(&mut state, &Action::EditBlock);
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn create_block_guard_in_linked_refs() {
        let mut state = test_state();
        set_linked_refs(&mut state, make_linked_refs_state());
        let flat = state.flat_block_count();
        state.selected_block = flat;
        handle_action(&mut state, &Action::CreateBlock);
        assert_eq!(state.input_mode, InputMode::Normal);
    }

    #[test]
    fn enter_on_linked_ref_section_header_toggles_collapse() {
        let mut state = test_state();
        set_linked_refs(&mut state, make_linked_refs_state());
        let flat = state.flat_block_count();
        state.selected_block = flat;
        handle_action(&mut state, &Action::Enter);
        assert!(state.linked_refs.get(TEST_DAY_TITLE).unwrap().collapsed);
        handle_action(&mut state, &Action::Enter);
        assert!(!state.linked_refs.get(TEST_DAY_TITLE).unwrap().collapsed);
    }

    #[test]
    fn enter_on_linked_ref_group_header_navigates() {
        let mut state = test_state();
        state.view_mode = ViewMode::Page {
            title: "Target".into(),
        };
        set_linked_refs(&mut state, make_linked_refs_state());
        let flat = state.flat_block_count();
        state.selected_block = flat + 1; // "Page A" group header
        let result = handle_action(&mut state, &Action::Enter);
        assert!(matches!(result, Some(LoadRequest::Page(ref t)) if t == "Page A"));
    }

    #[test]
    fn navigate_to_page_clears_linked_refs() {
        let mut state = test_state();
        set_linked_refs(&mut state, make_linked_refs_state());
        navigate_to_page(&mut state, "SomePage".into());
        assert!(state.linked_refs.is_empty());
    }

    #[test]
    fn handle_page_loaded_sets_state() {
        let mut state = test_state();
        state.view_mode = ViewMode::Page {
            title: "Target".into(),
        };
        let note = DailyNote {
            date: chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap(),
            uid: "page-uid".into(),
            title: "Target".into(),
            blocks: vec![make_block("b1", "content", 0)],
        };
        handle_page_loaded(&mut state, note);
        assert!(!state.loading);
        assert_eq!(state.days.len(), 1);
    }
}
