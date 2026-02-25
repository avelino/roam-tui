use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Action {
    MoveUp,
    MoveDown,
    Collapse,
    Expand,
    Enter,
    Exit,
    Search,
    Quit,
    ToggleSidebar,
    Help,
    GoDaily,
    NextDay,
    PrevDay,
    QuickSwitcher,
    Indent,
    Unindent,
    EditBlock,
    CreateBlock,
    Undo,
    Redo,
    CursorLeft,
    CursorRight,
}

impl Action {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "move_up" => Some(Self::MoveUp),
            "move_down" => Some(Self::MoveDown),
            "collapse" => Some(Self::Collapse),
            "expand" => Some(Self::Expand),
            "enter" => Some(Self::Enter),
            "exit" => Some(Self::Exit),
            "search" => Some(Self::Search),
            "quit" => Some(Self::Quit),
            "toggle_sidebar" => Some(Self::ToggleSidebar),
            "help" => Some(Self::Help),
            "go_daily" => Some(Self::GoDaily),
            "next_day" => Some(Self::NextDay),
            "prev_day" => Some(Self::PrevDay),
            "quick_switcher" => Some(Self::QuickSwitcher),
            "indent" => Some(Self::Indent),
            "unindent" => Some(Self::Unindent),
            "edit_block" => Some(Self::EditBlock),
            "create_block" => Some(Self::CreateBlock),
            "undo" => Some(Self::Undo),
            "redo" => Some(Self::Redo),
            "cursor_left" => Some(Self::CursorLeft),
            "cursor_right" => Some(Self::CursorRight),
            _ => None,
        }
    }

    pub fn hint_text(&self) -> &'static str {
        match self {
            Self::MoveUp => "up",
            Self::MoveDown => "down",
            Self::Collapse => "collapse",
            Self::Expand => "expand",
            Self::Enter => "enter",
            Self::Exit => "exit",
            Self::Search => "search",
            Self::Quit => "quit",
            Self::ToggleSidebar => "sidebar",
            Self::Help => "help",
            Self::GoDaily => "daily",
            Self::NextDay => "next day",
            Self::PrevDay => "prev day",
            Self::QuickSwitcher => "switcher",
            Self::Indent => "indent",
            Self::Unindent => "unindent",
            Self::EditBlock => "edit",
            Self::CreateBlock => "new block",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::CursorLeft => "cursor ←",
            Self::CursorRight => "cursor →",
        }
    }
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn ctrl(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL)
}

fn alt(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::ALT)
}

fn shift(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::SHIFT)
}

fn ctrl_shift(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::CONTROL | KeyModifiers::SHIFT)
}

pub fn vim_preset() -> HashMap<KeyEvent, Action> {
    let mut m = HashMap::new();
    m.insert(key(KeyCode::Char('k')), Action::MoveUp);
    m.insert(key(KeyCode::Up), Action::MoveUp);
    m.insert(key(KeyCode::Char('j')), Action::MoveDown);
    m.insert(key(KeyCode::Down), Action::MoveDown);
    m.insert(key(KeyCode::Char('h')), Action::Collapse);
    m.insert(key(KeyCode::Char('l')), Action::Expand);
    m.insert(key(KeyCode::Enter), Action::Enter);
    m.insert(key(KeyCode::Esc), Action::Exit);
    m.insert(key(KeyCode::Char('/')), Action::Search);
    m.insert(key(KeyCode::Char('q')), Action::Quit);
    m.insert(key(KeyCode::Char('b')), Action::ToggleSidebar);
    m.insert(key(KeyCode::Char('?')), Action::Help);
    m.insert(ctrl(KeyCode::Char('o')), Action::QuickSwitcher);
    m.insert(key(KeyCode::Tab), Action::Indent);
    m.insert(shift(KeyCode::BackTab), Action::Unindent);
    m.insert(key(KeyCode::Char('i')), Action::EditBlock);
    m.insert(key(KeyCode::Char('o')), Action::CreateBlock);
    m.insert(key(KeyCode::Char('u')), Action::Undo);
    m.insert(ctrl(KeyCode::Char('r')), Action::Redo);
    m.insert(shift(KeyCode::Char('N')), Action::NextDay);
    m.insert(shift(KeyCode::Char('P')), Action::PrevDay);
    m.insert(shift(KeyCode::Char('G')), Action::GoDaily);
    m.insert(key(KeyCode::PageDown), Action::NextDay);
    m.insert(key(KeyCode::PageUp), Action::PrevDay);
    m.insert(key(KeyCode::Left), Action::CursorLeft);
    m.insert(key(KeyCode::Right), Action::CursorRight);
    m
}

pub fn emacs_preset() -> HashMap<KeyEvent, Action> {
    let mut m = HashMap::new();
    m.insert(ctrl(KeyCode::Char('p')), Action::MoveUp);
    m.insert(key(KeyCode::Up), Action::MoveUp);
    m.insert(ctrl(KeyCode::Char('n')), Action::MoveDown);
    m.insert(key(KeyCode::Down), Action::MoveDown);
    m.insert(ctrl(KeyCode::Char('b')), Action::Collapse);
    m.insert(ctrl(KeyCode::Char('f')), Action::Expand);
    m.insert(key(KeyCode::Enter), Action::Enter);
    m.insert(ctrl(KeyCode::Char('g')), Action::Exit);
    m.insert(ctrl(KeyCode::Char('s')), Action::Search);
    m.insert(ctrl(KeyCode::Char('q')), Action::Quit);
    m.insert(ctrl(KeyCode::Char('h')), Action::Help);
    m.insert(key(KeyCode::Tab), Action::Indent);
    m.insert(shift(KeyCode::BackTab), Action::Unindent);
    m.insert(key(KeyCode::Enter), Action::EditBlock);
    m.insert(alt(KeyCode::Enter), Action::CreateBlock);
    m.insert(ctrl(KeyCode::Char('/')), Action::Undo);
    m.insert(ctrl_shift(KeyCode::Char('/')), Action::Redo);
    m.insert(alt(KeyCode::Char('n')), Action::NextDay);
    m.insert(alt(KeyCode::Char('p')), Action::PrevDay);
    m.insert(ctrl(KeyCode::Char('d')), Action::GoDaily);
    m.insert(key(KeyCode::PageDown), Action::NextDay);
    m.insert(key(KeyCode::PageUp), Action::PrevDay);
    m.insert(key(KeyCode::Left), Action::CursorLeft);
    m.insert(key(KeyCode::Right), Action::CursorRight);
    m
}

pub fn vscode_preset() -> HashMap<KeyEvent, Action> {
    let mut m = HashMap::new();
    m.insert(key(KeyCode::Up), Action::MoveUp);
    m.insert(key(KeyCode::Down), Action::MoveDown);
    m.insert(key(KeyCode::Left), Action::CursorLeft);
    m.insert(key(KeyCode::Right), Action::CursorRight);
    m.insert(ctrl(KeyCode::Left), Action::Collapse);
    m.insert(ctrl(KeyCode::Right), Action::Expand);
    m.insert(key(KeyCode::Enter), Action::Enter);
    m.insert(key(KeyCode::Esc), Action::Exit);
    m.insert(ctrl_shift(KeyCode::Char('f')), Action::Search);
    m.insert(ctrl(KeyCode::Char('q')), Action::Quit);
    m.insert(ctrl(KeyCode::Char('b')), Action::ToggleSidebar);
    m.insert(key(KeyCode::F(1)), Action::Help);
    m.insert(ctrl(KeyCode::Char('p')), Action::QuickSwitcher);
    m.insert(ctrl(KeyCode::Char('d')), Action::GoDaily);
    m.insert(key(KeyCode::Tab), Action::Indent);
    m.insert(shift(KeyCode::BackTab), Action::Unindent);
    m.insert(key(KeyCode::Enter), Action::EditBlock);
    m.insert(ctrl(KeyCode::Enter), Action::CreateBlock);
    m.insert(ctrl(KeyCode::Char('z')), Action::Undo);
    m.insert(ctrl_shift(KeyCode::Char('z')), Action::Redo);
    m.insert(alt(KeyCode::Up), Action::NextDay);
    m.insert(alt(KeyCode::Down), Action::PrevDay);
    m.insert(key(KeyCode::PageDown), Action::NextDay);
    m.insert(key(KeyCode::PageUp), Action::PrevDay);
    m
}

pub fn get_preset(name: &str) -> Option<HashMap<KeyEvent, Action>> {
    match name.to_lowercase().as_str() {
        "vim" => Some(vim_preset()),
        "emacs" => Some(emacs_preset()),
        "vscode" => Some(vscode_preset()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn has_essential_actions(preset: &HashMap<KeyEvent, Action>) {
        let actions: Vec<&Action> = preset.values().collect();
        assert!(actions.contains(&&Action::MoveUp), "Missing MoveUp");
        assert!(actions.contains(&&Action::MoveDown), "Missing MoveDown");
        assert!(actions.contains(&&Action::EditBlock), "Missing EditBlock");
        assert!(actions.contains(&&Action::Quit), "Missing Quit");
        assert!(actions.contains(&&Action::Search), "Missing Search");
    }

    #[test]
    fn vim_preset_has_essential_actions() {
        has_essential_actions(&vim_preset());
    }

    #[test]
    fn emacs_preset_has_essential_actions() {
        has_essential_actions(&emacs_preset());
    }

    #[test]
    fn vscode_preset_has_essential_actions() {
        has_essential_actions(&vscode_preset());
    }

    #[test]
    fn vim_j_maps_to_move_down() {
        let preset = vim_preset();
        assert_eq!(
            preset.get(&key(KeyCode::Char('j'))),
            Some(&Action::MoveDown)
        );
    }

    #[test]
    fn vim_k_maps_to_move_up() {
        let preset = vim_preset();
        assert_eq!(preset.get(&key(KeyCode::Char('k'))), Some(&Action::MoveUp));
    }

    #[test]
    fn vim_q_maps_to_quit() {
        let preset = vim_preset();
        assert_eq!(preset.get(&key(KeyCode::Char('q'))), Some(&Action::Quit));
    }

    #[test]
    fn emacs_ctrl_p_maps_to_move_up() {
        let preset = emacs_preset();
        assert_eq!(preset.get(&ctrl(KeyCode::Char('p'))), Some(&Action::MoveUp));
    }

    #[test]
    fn vscode_arrows_for_navigation() {
        let preset = vscode_preset();
        assert_eq!(preset.get(&key(KeyCode::Up)), Some(&Action::MoveUp));
        assert_eq!(preset.get(&key(KeyCode::Down)), Some(&Action::MoveDown));
    }

    #[test]
    fn get_preset_returns_none_for_unknown() {
        assert!(get_preset("unknown").is_none());
    }

    #[test]
    fn get_preset_case_insensitive() {
        assert!(get_preset("Vim").is_some());
        assert!(get_preset("VIM").is_some());
        assert!(get_preset("Emacs").is_some());
        assert!(get_preset("VSCode").is_some());
    }

    #[test]
    fn action_from_str_roundtrip() {
        assert_eq!(Action::from_str("move_up"), Some(Action::MoveUp));
        assert_eq!(Action::from_str("quit"), Some(Action::Quit));
        assert_eq!(Action::from_str("search"), Some(Action::Search));
        assert_eq!(Action::from_str("MOVE_DOWN"), Some(Action::MoveDown));
        assert_eq!(Action::from_str("nonexistent"), None);
    }

    #[test]
    fn vim_i_maps_to_edit_block() {
        let preset = vim_preset();
        assert_eq!(
            preset.get(&key(KeyCode::Char('i'))),
            Some(&Action::EditBlock)
        );
    }

    #[test]
    fn vim_o_maps_to_create_block() {
        let preset = vim_preset();
        assert_eq!(
            preset.get(&key(KeyCode::Char('o'))),
            Some(&Action::CreateBlock)
        );
    }

    #[test]
    fn action_from_str_edit_create() {
        assert_eq!(Action::from_str("edit_block"), Some(Action::EditBlock));
        assert_eq!(Action::from_str("create_block"), Some(Action::CreateBlock));
    }

    #[test]
    fn action_hint_text_returns_non_empty() {
        let actions = [
            Action::MoveUp,
            Action::MoveDown,
            Action::Quit,
            Action::Search,
            Action::Help,
        ];
        for action in &actions {
            assert!(!action.hint_text().is_empty());
        }
    }

    #[test]
    fn vim_u_maps_to_undo() {
        let preset = vim_preset();
        assert_eq!(preset.get(&key(KeyCode::Char('u'))), Some(&Action::Undo));
    }

    #[test]
    fn action_from_str_undo() {
        assert_eq!(Action::from_str("undo"), Some(Action::Undo));
    }

    #[test]
    fn vscode_ctrl_z_maps_to_undo() {
        let preset = vscode_preset();
        assert_eq!(preset.get(&ctrl(KeyCode::Char('z'))), Some(&Action::Undo));
    }

    #[test]
    fn emacs_ctrl_slash_maps_to_undo() {
        let preset = emacs_preset();
        assert_eq!(preset.get(&ctrl(KeyCode::Char('/'))), Some(&Action::Undo));
    }

    #[test]
    fn vim_pagedown_maps_to_next_day() {
        let preset = vim_preset();
        assert_eq!(preset.get(&key(KeyCode::PageDown)), Some(&Action::NextDay));
    }

    #[test]
    fn vim_pageup_maps_to_prev_day() {
        let preset = vim_preset();
        assert_eq!(preset.get(&key(KeyCode::PageUp)), Some(&Action::PrevDay));
    }

    #[test]
    fn emacs_pagedown_maps_to_next_day() {
        let preset = emacs_preset();
        assert_eq!(preset.get(&key(KeyCode::PageDown)), Some(&Action::NextDay));
    }

    #[test]
    fn emacs_pageup_maps_to_prev_day() {
        let preset = emacs_preset();
        assert_eq!(preset.get(&key(KeyCode::PageUp)), Some(&Action::PrevDay));
    }

    #[test]
    fn vscode_pagedown_maps_to_next_day() {
        let preset = vscode_preset();
        assert_eq!(preset.get(&key(KeyCode::PageDown)), Some(&Action::NextDay));
    }

    #[test]
    fn vscode_pageup_maps_to_prev_day() {
        let preset = vscode_preset();
        assert_eq!(preset.get(&key(KeyCode::PageUp)), Some(&Action::PrevDay));
    }

    // --- CursorLeft / CursorRight key mapping tests ---

    #[test]
    fn vim_left_maps_to_cursor_left() {
        let preset = vim_preset();
        assert_eq!(preset.get(&key(KeyCode::Left)), Some(&Action::CursorLeft));
    }

    #[test]
    fn vim_right_maps_to_cursor_right() {
        let preset = vim_preset();
        assert_eq!(preset.get(&key(KeyCode::Right)), Some(&Action::CursorRight));
    }

    #[test]
    fn vscode_left_maps_to_cursor_left_not_collapse() {
        let preset = vscode_preset();
        assert_eq!(preset.get(&key(KeyCode::Left)), Some(&Action::CursorLeft));
        assert_ne!(preset.get(&key(KeyCode::Left)), Some(&Action::Collapse));
    }

    #[test]
    fn vscode_right_maps_to_cursor_right_not_expand() {
        let preset = vscode_preset();
        assert_eq!(preset.get(&key(KeyCode::Right)), Some(&Action::CursorRight));
        assert_ne!(preset.get(&key(KeyCode::Right)), Some(&Action::Expand));
    }

    #[test]
    fn vscode_ctrl_left_maps_to_collapse() {
        let preset = vscode_preset();
        assert_eq!(preset.get(&ctrl(KeyCode::Left)), Some(&Action::Collapse));
    }

    #[test]
    fn vscode_ctrl_right_maps_to_expand() {
        let preset = vscode_preset();
        assert_eq!(preset.get(&ctrl(KeyCode::Right)), Some(&Action::Expand));
    }

    #[test]
    fn action_from_str_cursor_left_right() {
        assert_eq!(Action::from_str("cursor_left"), Some(Action::CursorLeft));
        assert_eq!(Action::from_str("cursor_right"), Some(Action::CursorRight));
    }

    #[test]
    fn action_hint_text_cursor_left_right() {
        assert!(!Action::CursorLeft.hint_text().is_empty());
        assert!(!Action::CursorRight.hint_text().is_empty());
    }
}
