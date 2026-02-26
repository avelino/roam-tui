use chrono::{Days, Local};

use crate::app::blocks::format_roam_daily_title;
use crate::edit_buffer::EditBuffer;

use super::types::{DateOffset, SlashAction};

/// Execute a slash command action on the edit buffer.
/// `slash_pos` is the position of the '/' character in the buffer.
/// `query_len` is the length of the typed query after '/'.
pub fn execute(action: &SlashAction, buffer: &mut EditBuffer, slash_pos: usize, query_len: usize) {
    let replace_end = slash_pos + 1 + query_len; // '/' + query chars

    match action {
        SlashAction::PrependText(prefix) => {
            // Remove the slash and query
            buffer.replace_range(slash_pos, replace_end, "");
            // Strip existing TODO/DONE prefix if present
            let text: String = buffer.chars.iter().collect();
            let stripped = text
                .strip_prefix("{{[[TODO]]}} ")
                .or_else(|| text.strip_prefix("{{[[DONE]]}} "));
            if let Some(rest) = stripped {
                let prefix_len = text.len() - rest.len();
                buffer.chars.drain(..prefix_len);
                buffer.cursor = buffer.cursor.saturating_sub(prefix_len);
            }
            // Insert new prefix at start
            let prefix_chars: Vec<char> = prefix.chars().collect();
            let prefix_len = prefix_chars.len();
            for (i, ch) in prefix_chars.into_iter().enumerate() {
                buffer.chars.insert(i, ch);
            }
            buffer.cursor += prefix_len;
        }
        SlashAction::InsertText(text) => {
            buffer.replace_range(slash_pos, replace_end, text);
        }
        SlashAction::InsertPair { open, close } => {
            let replacement = format!("{}{}", open, close);
            buffer.replace_range(slash_pos, replace_end, &replacement);
            // Place cursor between open and close
            buffer.cursor = slash_pos + open.chars().count();
        }
        SlashAction::InsertDate(offset) => {
            let today = Local::now().date_naive();
            let date = match offset {
                DateOffset::Today => today,
                DateOffset::Yesterday => today.checked_sub_days(Days::new(1)).unwrap_or(today),
                DateOffset::Tomorrow => today.checked_add_days(Days::new(1)).unwrap_or(today),
            };
            let title = format_roam_daily_title(date);
            let link = format!("[[{}]]", title);
            buffer.replace_range(slash_pos, replace_end, &link);
        }
        SlashAction::InsertTime => {
            let time_str = Local::now().format("%H:%M").to_string();
            buffer.replace_range(slash_pos, replace_end, &time_str);
        }
        SlashAction::InsertCodeBlock => {
            let code = "```\n\n```";
            buffer.replace_range(slash_pos, replace_end, code);
            // Place cursor on the empty line between fences
            buffer.cursor = slash_pos + 4; // after "```\n"
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit_buffer::EditBuffer;

    #[test]
    fn execute_insert_text_hr() {
        let mut buf = EditBuffer::new("hello /");
        buf.cursor = 7;
        execute(&SlashAction::InsertText("---"), &mut buf, 6, 0);
        assert_eq!(buf.to_string(), "hello ---");
    }

    #[test]
    fn execute_insert_text_with_query() {
        let mut buf = EditBuffer::new("text /hr");
        buf.cursor = 8;
        execute(&SlashAction::InsertText("---"), &mut buf, 5, 2);
        assert_eq!(buf.to_string(), "text ---");
    }

    #[test]
    fn execute_insert_pair_bold() {
        let mut buf = EditBuffer::new("/");
        buf.cursor = 1;
        execute(
            &SlashAction::InsertPair {
                open: "**",
                close: "**",
            },
            &mut buf,
            0,
            0,
        );
        assert_eq!(buf.to_string(), "****");
        assert_eq!(buf.cursor, 2); // between the pairs
    }

    #[test]
    fn execute_insert_pair_embed() {
        let mut buf = EditBuffer::new("test /");
        buf.cursor = 6;
        execute(
            &SlashAction::InsertPair {
                open: "{{[[embed]]: ",
                close: "}}",
            },
            &mut buf,
            5,
            0,
        );
        assert_eq!(buf.to_string(), "test {{[[embed]]: }}");
        assert_eq!(buf.cursor, 18); // after "{{[[embed]]: "
    }

    #[test]
    fn execute_prepend_todo() {
        let mut buf = EditBuffer::new("buy milk /");
        buf.cursor = 10;
        execute(&SlashAction::PrependText("{{[[TODO]]}} "), &mut buf, 9, 0);
        assert_eq!(buf.to_string(), "{{[[TODO]]}} buy milk ");
    }

    #[test]
    fn execute_prepend_replaces_existing_todo() {
        let mut buf = EditBuffer::new("{{[[TODO]]}} task /");
        buf.cursor = 19;
        execute(&SlashAction::PrependText("{{[[DONE]]}} "), &mut buf, 18, 0);
        assert_eq!(buf.to_string(), "{{[[DONE]]}} task ");
    }

    #[test]
    fn execute_prepend_h1() {
        let mut buf = EditBuffer::new("title /");
        buf.cursor = 7;
        execute(&SlashAction::PrependText("# "), &mut buf, 6, 0);
        assert_eq!(buf.to_string(), "# title ");
    }

    #[test]
    fn execute_prepend_replaces_existing_done() {
        let mut buf = EditBuffer::new("{{[[DONE]]}} task /");
        buf.cursor = 19;
        execute(&SlashAction::PrependText("{{[[TODO]]}} "), &mut buf, 18, 0);
        assert_eq!(buf.to_string(), "{{[[TODO]]}} task ");
    }

    #[test]
    fn execute_insert_date_today() {
        let mut buf = EditBuffer::new("/");
        buf.cursor = 1;
        execute(&SlashAction::InsertDate(DateOffset::Today), &mut buf, 0, 0);
        let result = buf.to_string();
        assert!(result.starts_with("[["));
        assert!(result.ends_with("]]"));
        // Should contain a year
        assert!(result.contains("2026") || result.contains("2025") || result.contains("2027"));
    }

    #[test]
    fn execute_insert_time() {
        let mut buf = EditBuffer::new("/");
        buf.cursor = 1;
        execute(&SlashAction::InsertTime, &mut buf, 0, 0);
        let result = buf.to_string();
        assert_eq!(result.len(), 5); // "HH:MM"
        assert_eq!(result.chars().nth(2), Some(':'));
    }

    #[test]
    fn execute_insert_code_block() {
        let mut buf = EditBuffer::new("text /");
        buf.cursor = 6;
        execute(&SlashAction::InsertCodeBlock, &mut buf, 5, 0);
        assert_eq!(buf.to_string(), "text ```\n\n```");
        assert_eq!(buf.cursor, 9); // on the empty line
    }

    #[test]
    fn execute_with_query_in_middle() {
        let mut buf = EditBuffer::new("start /bo end");
        buf.cursor = 10; // after "bo"
        execute(
            &SlashAction::InsertPair {
                open: "**",
                close: "**",
            },
            &mut buf,
            6,
            2,
        );
        assert_eq!(buf.to_string(), "start **** end");
        assert_eq!(buf.cursor, 8); // between ** and **
    }
}
