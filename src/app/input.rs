use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::api::types::{
    BlockLocation, BlockRef, BlockUpdate, DailyNote, NewBlock, OrderValue, WriteAction,
};

use super::blocks::{
    dedent_block_in_days, find_block_in_days, find_block_index_by_uid, indent_block_in_days,
    remove_block_from_days, resolve_block_at_index, update_block_text_in_days,
};
use super::nav::navigate_to_page;
use super::search::{filter_blocks, AUTOCOMPLETE_LIMIT, SEARCH_LIMIT};
use super::state::{AppState, AutocompleteState, CreateInfo, InputMode, LoadRequest, UndoEntry};

// --- Link picker key handling ---

pub(super) fn handle_link_picker_key(state: &mut AppState, key: &KeyEvent) -> Option<LoadRequest> {
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
    if super::search::detect_block_ref_trigger(buffer) {
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

pub(super) fn finalize_insert(state: &mut AppState) -> Option<WriteAction> {
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
