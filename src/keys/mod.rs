pub mod parser;
pub mod preset;

use std::collections::HashMap;

use crossterm::event::KeyEvent;

use crate::error::{Result, RoamError};
use preset::{get_preset, Action};

pub struct KeybindingMap {
    bindings: HashMap<KeyEvent, Action>,
}

impl KeybindingMap {
    pub fn from_preset(name: &str, overrides: &HashMap<String, String>) -> Result<Self> {
        let mut bindings = get_preset(name)
            .ok_or_else(|| RoamError::Config(format!("Unknown keybinding preset: {}", name)))?;

        for (action_name, key_str) in overrides {
            let action = Action::from_str(action_name)
                .ok_or_else(|| RoamError::Config(format!("Unknown action: {}", action_name)))?;
            let key_event = parser::parse_key(key_str)?;

            bindings.retain(|_, v| v != &action);
            bindings.insert(key_event, action);
        }

        Ok(Self { bindings })
    }

    pub fn resolve(&self, key: &KeyEvent) -> Option<&Action> {
        self.bindings.get(key)
    }

    pub fn hints(&self) -> Vec<(String, &'static str)> {
        let important = [
            Action::Quit,
            Action::Search,
            Action::Help,
            Action::MoveUp,
            Action::MoveDown,
        ];

        let mut hints = Vec::new();
        for action in &important {
            if let Some((key_event, _)) = self.bindings.iter().find(|(_, a)| *a == action) {
                hints.push((format_key_event(key_event), action.hint_text()));
            }
        }
        hints
    }
}

fn format_key_event(key: &KeyEvent) -> String {
    use crossterm::event::{KeyCode, KeyModifiers};

    let mut parts = Vec::new();

    if key.modifiers.contains(KeyModifiers::CONTROL) {
        parts.push("Ctrl".to_string());
    }
    if key.modifiers.contains(KeyModifiers::ALT) {
        parts.push("Alt".to_string());
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        parts.push("Shift".to_string());
    }

    let key_str = match key.code {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "BackTab".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Delete => "Delete".to_string(),
        KeyCode::Up => "↑".to_string(),
        KeyCode::Down => "↓".to_string(),
        KeyCode::Left => "←".to_string(),
        KeyCode::Right => "→".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::F(n) => format!("F{}", n),
        _ => "?".to_string(),
    };
    parts.push(key_str);

    parts.join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    #[test]
    fn from_preset_vim_resolves_j_to_move_down() {
        let map = KeybindingMap::from_preset("vim", &HashMap::new()).unwrap();
        let key = KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE);
        assert_eq!(map.resolve(&key), Some(&Action::MoveDown));
    }

    #[test]
    fn from_preset_vim_resolves_q_to_quit() {
        let map = KeybindingMap::from_preset("vim", &HashMap::new()).unwrap();
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(map.resolve(&key), Some(&Action::Quit));
    }

    #[test]
    fn override_replaces_existing_binding() {
        let mut overrides = HashMap::new();
        overrides.insert("quit".into(), "Ctrl+q".into());

        let map = KeybindingMap::from_preset("vim", &overrides).unwrap();

        let old_key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(map.resolve(&old_key), None);

        let new_key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert_eq!(map.resolve(&new_key), Some(&Action::Quit));
    }

    #[test]
    fn unknown_preset_returns_error() {
        let result = KeybindingMap::from_preset("unknown", &HashMap::new());
        assert!(result.is_err());
    }

    #[test]
    fn unknown_action_in_override_returns_error() {
        let mut overrides = HashMap::new();
        overrides.insert("nonexistent".into(), "q".into());

        let result = KeybindingMap::from_preset("vim", &overrides);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_returns_none_for_unbound_key() {
        let map = KeybindingMap::from_preset("vim", &HashMap::new()).unwrap();
        let key = KeyEvent::new(KeyCode::Char('z'), KeyModifiers::NONE);
        assert_eq!(map.resolve(&key), None);
    }

    #[test]
    fn hints_returns_important_actions() {
        let map = KeybindingMap::from_preset("vim", &HashMap::new()).unwrap();
        let hints = map.hints();
        let hint_labels: Vec<&str> = hints.iter().map(|(_, label)| *label).collect();
        assert!(hint_labels.contains(&"quit"));
        assert!(hint_labels.contains(&"search"));
    }

    #[test]
    fn format_key_event_simple_char() {
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);
        assert_eq!(format_key_event(&key), "q");
    }

    #[test]
    fn format_key_event_with_ctrl() {
        let key = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(format_key_event(&key), "Ctrl+c");
    }

    #[test]
    fn format_key_event_arrow() {
        let key = KeyEvent::new(KeyCode::Up, KeyModifiers::NONE);
        assert_eq!(format_key_event(&key), "↑");
    }
}
