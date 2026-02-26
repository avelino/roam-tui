use std::collections::{HashMap, HashSet};

use chrono::{Local, NaiveDate};

use crate::api::types::{Block, DailyNote, LinkedRefBlock, LinkedRefGroup};
use crate::edit_buffer::EditBuffer;
use crate::error::ErrorPopup;

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
pub(super) struct ViewSnapshot {
    pub(super) view_mode: ViewMode,
    pub(super) days: Vec<DailyNote>,
    pub(super) selected_block: usize,
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
    Key(crossterm::event::KeyEvent),
    DailyNoteLoaded(DailyNote),
    PageLoaded(DailyNote),
    RefreshLoaded(DailyNote),
    BlockRefResolved(String, String),              // (uid, text)
    LinkedRefsLoaded(String, Vec<LinkedRefGroup>), // (page_title, groups)
    ApiError(crate::error::ErrorInfo),
    Tick,
}

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
    pub(super) pending_block_refs: HashSet<String>,
    pub autocomplete: Option<AutocompleteState>,
    pub search: Option<SearchState>,
    pub undo_stack: Vec<UndoEntry>,
    pub redo_stack: Vec<UndoEntry>,
    pub show_help: bool,
    pub view_mode: ViewMode,
    pub(super) nav_history: Vec<ViewSnapshot>,
    pub(super) nav_index: usize,
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

pub(crate) fn linked_ref_section_count(lr: &LinkedRefsState) -> usize {
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

pub(crate) fn count_blocks_recursive(blocks: &[Block]) -> usize {
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
