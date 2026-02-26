use chrono::NaiveDate;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::api::types::{Block, DailyNote, LinkedRefBlock, LinkedRefGroup};

use super::{AppState, LinkedRefsState};

pub fn make_block(uid: &str, text: &str, order: i64) -> Block {
    Block {
        uid: uid.into(),
        string: text.into(),
        order,
        children: vec![],
        open: true,
        refs: vec![],
    }
}

pub fn make_daily_note(year: i32, month: u32, day: u32, blocks: Vec<Block>) -> DailyNote {
    let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
    DailyNote {
        date,
        uid: format!("{:02}-{:02}-{}", month, day, year),
        title: format!("Test {}-{}-{}", year, month, day),
        blocks,
    }
}

pub fn test_state() -> AppState {
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

pub fn test_state_with_blocks() -> AppState {
    test_state()
}

pub fn test_state_with_children() -> AppState {
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
        refs: vec![],
    };
    let day = make_daily_note(2026, 2, 21, vec![parent, make_block("b2", "Sibling", 1)]);
    state.days = vec![day];
    state
}

pub fn test_state_two_days() -> AppState {
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

pub fn make_empty_note(year: i32, month: u32, day: u32) -> DailyNote {
    let date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
    DailyNote {
        date,
        uid: format!("{:02}-{:02}-{}", month, day, year),
        title: String::new(),
        blocks: vec![],
    }
}

pub fn key_event(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

pub fn ctrl_key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

pub fn enter_insert_mode(state: &mut AppState) {
    use crate::keys::preset::Action;
    super::handle_action(state, &Action::EditBlock);
}

pub fn make_linked_refs_state() -> LinkedRefsState {
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

/// Day title used by test_state() for the single day
pub const TEST_DAY_TITLE: &str = "Test 2026-2-21";

pub fn set_linked_refs(state: &mut AppState, lr: LinkedRefsState) {
    state.linked_refs.insert(TEST_DAY_TITLE.to_string(), lr);
}
