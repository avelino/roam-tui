use crate::api::types::Block;
use crate::edit_buffer::EditBuffer;
use crate::keys::preset::Action;
use crate::markdown;

use super::blocks::{
    find_block_in_days, find_block_index_by_uid, generate_uid, insert_block_in_days,
    resolve_block_at_index, set_block_open,
};
use super::nav::{
    navigate_to_page, push_nav_snapshot, restore_nav_snapshot, save_nav_snapshot_at_index,
};
use super::search::{filter_blocks, SEARCH_LIMIT};
use super::search::{filter_page_titles, QUICK_SWITCHER_LIMIT};
use super::state::{
    count_blocks_recursive, AppState, CreateInfo, InputMode, LinkPickerState, LinkedRefItem,
    LoadRequest, QuickSwitcherState, SearchState, ViewMode, ViewSnapshot,
};

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
                    refs: vec![],
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
                    refs: vec![],
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
        Action::QuickSwitcher => {
            let filtered = if !state.page_title_cache.is_empty() {
                filter_page_titles(&state.page_title_cache, "", QUICK_SWITCHER_LIMIT)
            } else {
                Vec::new()
            };
            state.quick_switcher = Some(QuickSwitcherState {
                query: String::new(),
                filtered,
                selected: 0,
                debounce_ticks: 0,
                fetching: false,
            });
            None
        }
        _ => None,
    }
}
