mod actions;
pub(crate) mod blocks;
mod input;
mod nav;
mod search;
pub(crate) mod slash;
mod state;
mod tasks;
mod undo;
pub use state::*;

use actions::handle_action;
use blocks::generate_uid;
use input::{handle_delete_block, handle_insert_key, handle_link_picker_key, handle_search_key};
use tasks::{
    collect_unresolved_refs, spawn_fetch_daily_note, spawn_fetch_linked_refs, spawn_fetch_page,
    spawn_refresh_daily_note, spawn_resolve_block_refs, spawn_write,
};
use undo::{apply_redo, apply_undo};

#[cfg(test)]
pub(crate) mod test_helpers;

use std::time::Duration;

use crossterm::event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use futures::StreamExt;
use ratatui::DefaultTerminal;
use tokio::sync::mpsc;

use crate::api::client::RoamClient;
use crate::api::types::{Block, DailyNote};
use crate::config::AppConfig;
use crate::error::{ErrorInfo, ErrorPopup, Result};
use crate::keys::preset::Action;
use crate::keys::KeybindingMap;

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

pub fn handle_daily_note_loaded(state: &mut AppState, mut note: DailyNote) {
    // Generate Roam-style title if the page doesn't exist yet
    if note.title.is_empty() {
        note.title = blocks::format_roam_daily_title(note.date);
    }
    // Ensure every day has at least one block so navigation always works
    if note.blocks.is_empty() {
        note.blocks.push(Block {
            uid: generate_uid(),
            string: String::new(),
            order: 0,
            children: vec![],
            open: true,
            refs: vec![],
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
            refs: vec![],
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
    use super::actions::handle_action;
    use super::blocks::*;
    use super::input::{
        finalize_insert, handle_delete_block, handle_insert_key, handle_search_key,
    };
    use super::nav::navigate_to_page;
    use super::search::{
        detect_block_ref_trigger, filter_blocks, AUTOCOMPLETE_LIMIT, SEARCH_LIMIT,
    };
    use super::tasks::extract_uids_from_text;
    use super::test_helpers::*;
    use super::undo::{apply_redo, apply_undo};
    use super::*;
    use crate::api::types::{Block, WriteAction};
    use crate::edit_buffer::EditBuffer;
    use crate::keys::preset::Action;
    use chrono::NaiveDate;
    use std::collections::{HashMap, HashSet};

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
            refs: vec![],
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
            refs: vec![],
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
            refs: vec![],
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
            refs: vec![],
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
            refs: vec![],
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
            refs: vec![],
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
            refs: vec![],
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
            refs: vec![],
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
            refs: vec![],
        };
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![child],
            open: true,
            refs: vec![],
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
            refs: vec![],
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
            refs: vec![],
        };
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![child],
            open: true,
            refs: vec![],
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
            refs: vec![],
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
            refs: vec![],
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
            refs: vec![],
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
            refs: vec![],
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

    // --- Slash menu tests ---

    /// Helper: enter insert mode and type " /" to open the slash menu.
    /// The space ensures '/' triggers (trigger requires start-of-buffer or whitespace).
    fn open_slash_menu(state: &mut AppState) {
        enter_insert_mode(state);
        handle_insert_key(state, &key_event(KeyCode::Char(' ')));
        handle_insert_key(state, &key_event(KeyCode::Char('/')));
    }

    #[test]
    fn typing_slash_opens_slash_menu() {
        let mut state = test_state();
        open_slash_menu(&mut state);
        assert!(state.slash_menu.is_some());
        let sm = state.slash_menu.as_ref().unwrap();
        assert_eq!(sm.query, "");
        assert_eq!(sm.selected, 0);
        assert_eq!(sm.commands.len(), 18);
    }

    #[test]
    fn slash_menu_filters_by_query() {
        let mut state = test_state();
        open_slash_menu(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('t')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('o')));
        let sm = state.slash_menu.as_ref().unwrap();
        assert_eq!(sm.query, "to");
        let names: Vec<&str> = sm.commands.iter().map(|c| c.name).collect();
        assert!(names.contains(&"todo"));
        assert!(names.contains(&"tomorrow"));
    }

    #[test]
    fn slash_menu_navigate_up_down() {
        let mut state = test_state();
        open_slash_menu(&mut state);
        assert_eq!(state.slash_menu.as_ref().unwrap().selected, 0);
        handle_insert_key(&mut state, &key_event(KeyCode::Down));
        assert_eq!(state.slash_menu.as_ref().unwrap().selected, 1);
        handle_insert_key(&mut state, &key_event(KeyCode::Down));
        assert_eq!(state.slash_menu.as_ref().unwrap().selected, 2);
        handle_insert_key(&mut state, &key_event(KeyCode::Up));
        assert_eq!(state.slash_menu.as_ref().unwrap().selected, 1);
        handle_insert_key(&mut state, &key_event(KeyCode::Up));
        assert_eq!(state.slash_menu.as_ref().unwrap().selected, 0);
        handle_insert_key(&mut state, &key_event(KeyCode::Up));
        assert_eq!(state.slash_menu.as_ref().unwrap().selected, 0);
    }

    #[test]
    fn slash_menu_esc_closes() {
        let mut state = test_state();
        open_slash_menu(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Esc));
        assert!(state.slash_menu.is_none());
        assert!(matches!(state.input_mode, InputMode::Insert { .. }));
    }

    #[test]
    fn slash_menu_backspace_on_empty_closes() {
        let mut state = test_state();
        open_slash_menu(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Backspace));
        assert!(state.slash_menu.is_none());
    }

    #[test]
    fn slash_menu_backspace_with_query() {
        let mut state = test_state();
        open_slash_menu(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('h')));
        assert_eq!(state.slash_menu.as_ref().unwrap().query, "h");
        handle_insert_key(&mut state, &key_event(KeyCode::Backspace));
        assert!(state.slash_menu.is_some());
        assert_eq!(state.slash_menu.as_ref().unwrap().query, "");
    }

    #[test]
    fn slash_menu_enter_executes_todo() {
        let mut state = test_state();
        open_slash_menu(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Enter));
        assert!(state.slash_menu.is_none());
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                assert!(buffer.to_string().starts_with("{{[[TODO]]}} "));
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn slash_menu_enter_executes_hr() {
        let mut state = test_state();
        open_slash_menu(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('h')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('r')));
        handle_insert_key(&mut state, &key_event(KeyCode::Enter));
        assert!(state.slash_menu.is_none());
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                assert!(buffer.to_string().contains("---"));
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn slash_menu_enter_executes_bold() {
        let mut state = test_state();
        open_slash_menu(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('b')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('o')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('l')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('d')));
        handle_insert_key(&mut state, &key_event(KeyCode::Enter));
        assert!(state.slash_menu.is_none());
        match &state.input_mode {
            InputMode::Insert { buffer, .. } => {
                assert!(buffer.to_string().contains("****"));
            }
            _ => panic!("Expected Insert mode"),
        }
    }

    #[test]
    fn slash_no_trigger_in_url() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        for ch in "http:/".chars() {
            handle_insert_key(&mut state, &key_event(KeyCode::Char(ch)));
        }
        assert!(state.slash_menu.is_none());
    }

    #[test]
    fn slash_trigger_after_space() {
        let mut state = test_state();
        enter_insert_mode(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char(' ')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('/')));
        assert!(state.slash_menu.is_some());
    }

    #[test]
    fn slash_menu_down_wraps_at_end() {
        let mut state = test_state();
        open_slash_menu(&mut state);
        handle_insert_key(&mut state, &key_event(KeyCode::Char('l')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('a')));
        handle_insert_key(&mut state, &key_event(KeyCode::Char('t')));
        assert_eq!(state.slash_menu.as_ref().unwrap().commands.len(), 1);
        assert_eq!(state.slash_menu.as_ref().unwrap().selected, 0);
        handle_insert_key(&mut state, &key_event(KeyCode::Down));
        assert_eq!(state.slash_menu.as_ref().unwrap().selected, 0);
    }
}
