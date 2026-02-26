pub mod execute;
mod types;

mod blockquote;
mod bold;
mod code;
mod date;
mod done;
mod embed;
mod h1;
mod h2;
mod h3;
mod highlight;
mod hr;
mod italic;
mod latex;
mod strikethrough;
mod time;
mod todo;
mod tomorrow;
mod yesterday;

pub use types::*;

use crate::edit_buffer::EditBuffer;

pub fn all_commands() -> Vec<SlashCommand> {
    vec![
        todo::CMD,
        done::CMD,
        date::CMD,
        yesterday::CMD,
        tomorrow::CMD,
        time::CMD,
        code::CMD,
        hr::CMD,
        bold::CMD,
        italic::CMD,
        highlight::CMD,
        strikethrough::CMD,
        h1::CMD,
        h2::CMD,
        h3::CMD,
        blockquote::CMD,
        embed::CMD,
        latex::CMD,
    ]
}

pub fn filter(query: &str) -> Vec<SlashCommand> {
    let q = query.to_lowercase();
    all_commands()
        .into_iter()
        .filter(|c| c.name.contains(&q))
        .collect()
}

/// Detects if the user just typed '/' at a position that should open the slash menu.
/// Returns `Some(slash_pos)` if triggered, `None` otherwise.
///
/// Triggers when:
/// - '/' at position 0 (start of buffer)
/// - '/' preceded by whitespace
///
/// Does NOT trigger:
/// - mid-word (e.g. "http:/")
/// - after other non-whitespace
pub fn detect_trigger(buffer: &EditBuffer) -> Option<usize> {
    let c = buffer.cursor;
    if c == 0 {
        return None;
    }
    let slash_pos = c - 1;
    if buffer.chars[slash_pos] != '/' {
        return None;
    }
    if slash_pos == 0 || buffer.chars[slash_pos - 1].is_whitespace() {
        Some(slash_pos)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- detect_trigger tests ---

    #[test]
    fn trigger_at_start() {
        let mut buf = EditBuffer::new("/");
        buf.cursor = 1;
        assert_eq!(detect_trigger(&buf), Some(0));
    }

    #[test]
    fn trigger_after_space() {
        let mut buf = EditBuffer::new("text /");
        buf.cursor = 6;
        assert_eq!(detect_trigger(&buf), Some(5));
    }

    #[test]
    fn trigger_after_newline() {
        let mut buf = EditBuffer::new("line\n/");
        buf.cursor = 6;
        assert_eq!(detect_trigger(&buf), Some(5));
    }

    #[test]
    fn no_trigger_mid_word() {
        let mut buf = EditBuffer::new("http:/");
        buf.cursor = 6;
        assert_eq!(detect_trigger(&buf), None);
    }

    #[test]
    fn no_trigger_in_path() {
        let mut buf = EditBuffer::new("path/to");
        buf.cursor = 5; // just after '/'
        assert_eq!(detect_trigger(&buf), None);
    }

    #[test]
    fn no_trigger_empty_buffer() {
        let buf = EditBuffer::new_empty();
        assert_eq!(detect_trigger(&buf), None);
    }

    #[test]
    fn no_trigger_cursor_at_start() {
        let mut buf = EditBuffer::new("/abc");
        buf.cursor = 0;
        assert_eq!(detect_trigger(&buf), None);
    }

    // --- filter tests ---

    #[test]
    fn filter_empty_returns_all() {
        let results = filter("");
        assert_eq!(results.len(), 18);
    }

    #[test]
    fn filter_by_prefix() {
        let results = filter("to");
        let names: Vec<&str> = results.iter().map(|c| c.name).collect();
        assert!(names.contains(&"todo"));
        assert!(names.contains(&"tomorrow"));
    }

    #[test]
    fn filter_case_insensitive() {
        let results = filter("TODO");
        assert!(results.iter().any(|c| c.name == "todo"));
    }

    #[test]
    fn filter_no_match() {
        let results = filter("zzz");
        assert!(results.is_empty());
    }

    #[test]
    fn filter_partial_match() {
        let results = filter("ode");
        assert!(results.iter().any(|c| c.name == "code"));
    }

    // --- all_commands tests ---

    #[test]
    fn all_commands_has_18() {
        assert_eq!(all_commands().len(), 18);
    }

    #[test]
    fn all_commands_unique_names() {
        let cmds = all_commands();
        let names: Vec<&str> = cmds.iter().map(|c| c.name).collect();
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(names.len(), unique.len());
    }
}
