use crate::api::types::{BlockLocation, BlockRef, BlockUpdate, NewBlock, OrderValue, WriteAction};

use super::blocks::{
    find_block_in_days, find_block_parent_info, insert_block_in_days, move_block_in_days,
    remove_block_from_days, resolve_block_at_index, update_block_text_in_days,
};
use super::state::{AppState, UndoEntry};

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
