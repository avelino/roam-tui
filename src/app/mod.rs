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
use input::{
    handle_delete_block, handle_insert_key, handle_link_picker_key, handle_quick_switcher_key,
    handle_search_key,
};
use tasks::{
    collect_unresolved_refs, spawn_fetch_daily_note, spawn_fetch_linked_refs, spawn_fetch_page,
    spawn_fetch_page_titles, spawn_refresh_daily_note, spawn_resolve_block_refs, spawn_write,
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
use crate::api::types::{
    Block, BlockLocation, BlockRef as ApiBlockRef, DailyNote, OrderValue, WriteAction,
};
use crate::config::AppConfig;
use crate::error::{ErrorInfo, ErrorPopup, Result};
use crate::keys::preset::Action;
use crate::keys::KeybindingMap;

use state::UndoEntry;

fn handle_batch_indent(state: &mut AppState) -> Option<WriteAction> {
    let (lo, hi) = state.selection.indices();
    state.redo_stack.clear();
    let mut undo_entries = Vec::new();
    let mut write_actions = Vec::new();

    for idx in lo..=hi {
        if let Some(info) = blocks::resolve_block_at_index(&state.days, &state.linked_refs, idx) {
            let saved_selected = state.selected_block;
            if let Some((new_parent, new_order)) =
                blocks::indent_block_in_days(&mut state.days, &info.block_uid)
            {
                undo_entries.push(UndoEntry::MoveBlock {
                    block_uid: info.block_uid.clone(),
                    old_parent_uid: info.parent_uid,
                    old_order: info.order,
                    selected_block: saved_selected,
                });
                write_actions.push(WriteAction::MoveBlock {
                    block: ApiBlockRef {
                        uid: info.block_uid,
                    },
                    location: BlockLocation {
                        parent_uid: new_parent,
                        order: OrderValue::Index(new_order),
                    },
                });
            }
        }
    }

    if write_actions.is_empty() {
        return None;
    }

    state.undo_stack.push(UndoEntry::Batch(undo_entries));
    Some(WriteAction::BatchActions {
        actions: write_actions,
    })
}

fn handle_batch_dedent(state: &mut AppState) -> Option<WriteAction> {
    let (lo, hi) = state.selection.indices();
    state.redo_stack.clear();
    let mut undo_entries = Vec::new();
    let mut write_actions = Vec::new();

    // Dedent in reverse to keep indices more stable
    for idx in (lo..=hi).rev() {
        if let Some(info) = blocks::resolve_block_at_index(&state.days, &state.linked_refs, idx) {
            let saved_selected = state.selected_block;
            if let Some((new_parent, new_order)) =
                blocks::dedent_block_in_days(&mut state.days, &info.block_uid)
            {
                undo_entries.push(UndoEntry::MoveBlock {
                    block_uid: info.block_uid.clone(),
                    old_parent_uid: info.parent_uid,
                    old_order: info.order,
                    selected_block: saved_selected,
                });
                write_actions.push(WriteAction::MoveBlock {
                    block: ApiBlockRef {
                        uid: info.block_uid,
                    },
                    location: BlockLocation {
                        parent_uid: new_parent,
                        order: OrderValue::Index(new_order),
                    },
                });
            }
        }
    }

    if write_actions.is_empty() {
        return None;
    }

    undo_entries.reverse();
    state.undo_stack.push(UndoEntry::Batch(undo_entries));
    Some(WriteAction::BatchActions {
        actions: write_actions,
    })
}

fn handle_move_block_up(state: &mut AppState) -> Option<WriteAction> {
    if state
        .resolve_linked_ref_item(state.selected_block)
        .is_some()
    {
        return None;
    }
    let info =
        blocks::resolve_block_at_index(&state.days, &state.linked_refs, state.selected_block)?;
    let saved_selected = state.selected_block;
    let block_old_order = info.order;
    let (parent_uid, new_order, sibling_uid) =
        blocks::move_block_up_in_days(&mut state.days, &info.block_uid)?;

    state.redo_stack.clear();
    if let Some(new_idx) =
        blocks::find_block_index_by_uid(&state.days, &state.linked_refs, &info.block_uid)
    {
        state.selected_block = new_idx;
        state.cursor_col = 0;
    }
    state.undo_stack.push(UndoEntry::SwapSiblings {
        block_uid: info.block_uid.clone(),
        sibling_uid,
        parent_uid: parent_uid.clone(),
        block_old_order,
        selected_block: saved_selected,
    });

    Some(WriteAction::MoveBlock {
        block: ApiBlockRef {
            uid: info.block_uid,
        },
        location: BlockLocation {
            parent_uid,
            order: OrderValue::Index(new_order),
        },
    })
}

fn handle_move_block_down(state: &mut AppState) -> Option<WriteAction> {
    if state
        .resolve_linked_ref_item(state.selected_block)
        .is_some()
    {
        return None;
    }
    let info =
        blocks::resolve_block_at_index(&state.days, &state.linked_refs, state.selected_block)?;
    let saved_selected = state.selected_block;
    let block_old_order = info.order;
    let (parent_uid, new_order, sibling_uid) =
        blocks::move_block_down_in_days(&mut state.days, &info.block_uid)?;

    state.redo_stack.clear();
    if let Some(new_idx) =
        blocks::find_block_index_by_uid(&state.days, &state.linked_refs, &info.block_uid)
    {
        state.selected_block = new_idx;
        state.cursor_col = 0;
    }
    state.undo_stack.push(UndoEntry::SwapSiblings {
        block_uid: info.block_uid.clone(),
        sibling_uid,
        parent_uid: parent_uid.clone(),
        block_old_order,
        selected_block: saved_selected,
    });

    Some(WriteAction::MoveBlock {
        block: ApiBlockRef {
            uid: info.block_uid,
        },
        location: BlockLocation {
            parent_uid,
            order: OrderValue::Index(new_order),
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
        } else if action == &Action::Indent && state.selection.is_multi() {
            if let Some(write_action) = handle_batch_indent(state) {
                spawn_write(client, write_action, tx);
            }
        } else if action == &Action::Unindent && state.selection.is_multi() {
            if let Some(write_action) = handle_batch_dedent(state) {
                spawn_write(client, write_action, tx);
            }
        } else if action == &Action::MoveBlockUp {
            if let Some(write_action) = handle_move_block_up(state) {
                spawn_write(client, write_action, tx);
            }
        } else if action == &Action::MoveBlockDown {
            if let Some(write_action) = handle_move_block_down(state) {
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
        let placeholder_uid = generate_uid();
        state.placeholder_uids.insert(placeholder_uid.clone());
        note.blocks.push(Block {
            uid: placeholder_uid,
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
        let placeholder_uid = generate_uid();
        state.placeholder_uids.insert(placeholder_uid.clone());
        note.blocks.push(Block {
            uid: placeholder_uid,
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

    let mut state = AppState::new(
        &config.graph.name,
        keybindings.hints(),
        keybindings.all_hints(),
    );

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
                    } else if state.quick_switcher.is_some() {
                        if let Some(req) = handle_quick_switcher_key(&mut state, &key) {
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
                        if state.needs_linked_refs_refresh {
                            state.needs_linked_refs_refresh = false;
                            for day in &state.days {
                                let title = day.title.clone();
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
                    let page_title = note.title.clone();
                    let page_uid = note.uid.clone();
                    handle_page_loaded(&mut state, note);
                    let unresolved = collect_unresolved_refs(&state);
                    spawn_resolve_block_refs(&client, unresolved, &mut state, &tx);

                    // Add newly created page to cache for future Quick Switcher searches
                    if !page_title.is_empty()
                        && !state.page_title_cache.iter().any(|(t, _)| t == &page_title)
                    {
                        state.page_title_cache.push((page_title.clone(), page_uid));
                        state
                            .page_title_cache
                            .sort_by(|a, b| a.0.to_lowercase().cmp(&b.0.to_lowercase()));
                    }

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
                AppMessage::PageTitlesLoaded(titles) => {
                    state.page_title_cache = titles;
                    if let Some(qs) = &mut state.quick_switcher {
                        qs.fetching = false;
                        let query = qs.query.clone();
                        qs.filtered = search::filter_page_titles(
                            &state.page_title_cache,
                            &query,
                            search::QUICK_SWITCHER_LIMIT,
                        );
                        qs.selected = qs.selected.min(qs.filtered.len().saturating_sub(1));
                    }
                }
                AppMessage::Tick => {
                    // Quick Switcher debounce
                    if let Some(qs) = &mut state.quick_switcher {
                        if qs.debounce_ticks > 0 {
                            qs.debounce_ticks -= 1;
                            if qs.debounce_ticks == 0 && !qs.fetching && !qs.query.is_empty() {
                                qs.fetching = true;
                                spawn_fetch_page_titles(&client, &tx);
                            }
                        }
                    }

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
mod tests;
