use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::error::{Result, RoamError};

pub fn parse_key(input: &str) -> Result<KeyEvent> {
    let parts: Vec<&str> = input.split('+').collect();
    let mut modifiers = KeyModifiers::NONE;
    let mut key_part = None;

    for (i, part) in parts.iter().enumerate() {
        let normalized = part.trim();
        match normalized.to_lowercase().as_str() {
            "ctrl" => modifiers |= KeyModifiers::CONTROL,
            "shift" => modifiers |= KeyModifiers::SHIFT,
            "alt" => modifiers |= KeyModifiers::ALT,
            _ => {
                if i == parts.len() - 1 {
                    key_part = Some(normalized);
                } else {
                    return Err(RoamError::Config(format!(
                        "Unknown modifier '{}' in key '{}'",
                        normalized, input
                    )));
                }
            }
        }
    }

    let key_str =
        key_part.ok_or_else(|| RoamError::Config(format!("No key code found in '{}'", input)))?;

    let code = parse_key_code(key_str)?;

    Ok(KeyEvent::new(code, modifiers))
}

fn parse_key_code(s: &str) -> Result<KeyCode> {
    match s.to_lowercase().as_str() {
        "enter" | "return" => Ok(KeyCode::Enter),
        "esc" | "escape" => Ok(KeyCode::Esc),
        "tab" => Ok(KeyCode::Tab),
        "backspace" | "bs" => Ok(KeyCode::Backspace),
        "delete" | "del" => Ok(KeyCode::Delete),
        "insert" | "ins" => Ok(KeyCode::Insert),
        "home" => Ok(KeyCode::Home),
        "end" => Ok(KeyCode::End),
        "pageup" | "pgup" => Ok(KeyCode::PageUp),
        "pagedown" | "pgdn" => Ok(KeyCode::PageDown),
        "up" | "↑" => Ok(KeyCode::Up),
        "down" | "↓" => Ok(KeyCode::Down),
        "left" | "←" => Ok(KeyCode::Left),
        "right" | "→" => Ok(KeyCode::Right),
        "space" => Ok(KeyCode::Char(' ')),
        s if s.starts_with('f') && s.len() > 1 => {
            let num: u8 = s[1..]
                .parse()
                .map_err(|_| RoamError::Config(format!("Invalid function key: {}", s)))?;
            if !(1..=12).contains(&num) {
                return Err(RoamError::Config(format!(
                    "Function key out of range: F{}",
                    num
                )));
            }
            Ok(KeyCode::F(num))
        }
        s if s.chars().count() == 1 => {
            let ch = s.chars().next().unwrap();
            Ok(KeyCode::Char(ch))
        }
        _ => Err(RoamError::Config(format!("Unknown key: {}", s))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyModifiers};

    #[test]
    fn parse_simple_char() {
        let key = parse_key("q").unwrap();
        assert_eq!(key.code, KeyCode::Char('q'));
        assert_eq!(key.modifiers, KeyModifiers::NONE);
    }

    #[test]
    fn parse_ctrl_modifier() {
        let key = parse_key("Ctrl+c").unwrap();
        assert_eq!(key.code, KeyCode::Char('c'));
        assert_eq!(key.modifiers, KeyModifiers::CONTROL);
    }

    #[test]
    fn parse_shift_modifier() {
        let key = parse_key("Shift+Tab").unwrap();
        assert_eq!(key.code, KeyCode::Tab);
        assert_eq!(key.modifiers, KeyModifiers::SHIFT);
    }

    #[test]
    fn parse_ctrl_shift_combo() {
        let key = parse_key("Ctrl+Shift+k").unwrap();
        assert_eq!(key.code, KeyCode::Char('k'));
        assert_eq!(key.modifiers, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    }

    #[test]
    fn parse_alt_modifier() {
        let key = parse_key("Alt+Enter").unwrap();
        assert_eq!(key.code, KeyCode::Enter);
        assert_eq!(key.modifiers, KeyModifiers::ALT);
    }

    #[test]
    fn parse_special_keys() {
        assert_eq!(parse_key("Enter").unwrap().code, KeyCode::Enter);
        assert_eq!(parse_key("Esc").unwrap().code, KeyCode::Esc);
        assert_eq!(parse_key("Tab").unwrap().code, KeyCode::Tab);
        assert_eq!(parse_key("Backspace").unwrap().code, KeyCode::Backspace);
        assert_eq!(parse_key("Delete").unwrap().code, KeyCode::Delete);
        assert_eq!(parse_key("Home").unwrap().code, KeyCode::Home);
        assert_eq!(parse_key("End").unwrap().code, KeyCode::End);
        assert_eq!(parse_key("PageUp").unwrap().code, KeyCode::PageUp);
        assert_eq!(parse_key("PageDown").unwrap().code, KeyCode::PageDown);
        assert_eq!(parse_key("Space").unwrap().code, KeyCode::Char(' '));
    }

    #[test]
    fn parse_arrow_keys() {
        assert_eq!(parse_key("Up").unwrap().code, KeyCode::Up);
        assert_eq!(parse_key("Down").unwrap().code, KeyCode::Down);
        assert_eq!(parse_key("Left").unwrap().code, KeyCode::Left);
        assert_eq!(parse_key("Right").unwrap().code, KeyCode::Right);
    }

    #[test]
    fn parse_arrow_unicode() {
        assert_eq!(parse_key("↑").unwrap().code, KeyCode::Up);
        assert_eq!(parse_key("↓").unwrap().code, KeyCode::Down);
        assert_eq!(parse_key("←").unwrap().code, KeyCode::Left);
        assert_eq!(parse_key("→").unwrap().code, KeyCode::Right);
    }

    #[test]
    fn parse_function_keys() {
        assert_eq!(parse_key("F1").unwrap().code, KeyCode::F(1));
        assert_eq!(parse_key("F12").unwrap().code, KeyCode::F(12));
    }

    #[test]
    fn parse_ctrl_arrow() {
        let key = parse_key("Ctrl+Up").unwrap();
        assert_eq!(key.code, KeyCode::Up);
        assert_eq!(key.modifiers, KeyModifiers::CONTROL);
    }

    #[test]
    fn parse_invalid_key_returns_error() {
        assert!(parse_key("").is_err());
        assert!(parse_key("F13").is_err());
        assert!(parse_key("Unknown+x").is_err());
    }

    #[test]
    fn parse_case_insensitive_modifiers() {
        let key = parse_key("ctrl+shift+k").unwrap();
        assert_eq!(key.code, KeyCode::Char('k'));
        assert_eq!(key.modifiers, KeyModifiers::CONTROL | KeyModifiers::SHIFT);
    }
}
