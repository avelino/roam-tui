use super::state::{AppState, LoadRequest, ViewMode, ViewSnapshot};

/// Save current view as a snapshot and prepare for navigating to a new page.
/// Returns a LoadRequest::Page for the given title.
pub(super) fn navigate_to_page(state: &mut AppState, title: String) -> LoadRequest {
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
pub(super) fn push_nav_snapshot(state: &mut AppState) {
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
pub(super) fn save_nav_snapshot_at_index(state: &mut AppState) {
    if state.nav_index < state.nav_history.len() {
        state.nav_history[state.nav_index] = ViewSnapshot {
            view_mode: state.view_mode.clone(),
            days: state.days.clone(),
            selected_block: state.selected_block,
        };
    }
}

/// Restore state from the snapshot at current nav_index.
pub(super) fn restore_nav_snapshot(state: &mut AppState) {
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
