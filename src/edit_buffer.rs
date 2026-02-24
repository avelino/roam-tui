#[derive(Debug, Clone, PartialEq)]
pub struct EditBuffer {
    pub chars: Vec<char>,
    pub cursor: usize,
}

impl EditBuffer {
    pub fn new(text: &str) -> Self {
        let chars: Vec<char> = text.chars().collect();
        let cursor = chars.len();
        Self { chars, cursor }
    }

    pub fn new_empty() -> Self {
        Self {
            chars: Vec::new(),
            cursor: 0,
        }
    }

    pub fn insert_char(&mut self, ch: char) {
        self.chars.insert(self.cursor, ch);
        self.cursor += 1;
    }

    pub fn insert_pair(&mut self, open: char, close: char) {
        self.chars.insert(self.cursor, open);
        self.chars.insert(self.cursor + 1, close);
        self.cursor += 1; // cursor between open and close
    }

    pub fn delete_back(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }

    pub fn delete_forward(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
        }
    }

    pub fn move_right(&mut self) {
        if self.cursor < self.chars.len() {
            self.cursor += 1;
        }
    }

    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor = self.chars.len();
    }

    pub fn move_word_left(&mut self) {
        while self.cursor > 0 && self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }
        while self.cursor > 0 && !self.chars[self.cursor - 1].is_whitespace() {
            self.cursor -= 1;
        }
    }

    pub fn move_word_right(&mut self) {
        let len = self.chars.len();
        while self.cursor < len && !self.chars[self.cursor].is_whitespace() {
            self.cursor += 1;
        }
        while self.cursor < len && self.chars[self.cursor].is_whitespace() {
            self.cursor += 1;
        }
    }

    pub fn move_up(&mut self) {
        let current_line_start = self.chars[..self.cursor]
            .iter()
            .rposition(|&c| c == '\n')
            .map(|p| p + 1)
            .unwrap_or(0);

        if current_line_start == 0 {
            self.cursor = 0;
            return;
        }

        let col = self.cursor - current_line_start;
        let prev_line_end = current_line_start - 1; // the \n before current line
        let prev_line_start = self.chars[..prev_line_end]
            .iter()
            .rposition(|&c| c == '\n')
            .map(|p| p + 1)
            .unwrap_or(0);
        let prev_line_len = prev_line_end - prev_line_start;
        self.cursor = prev_line_start + col.min(prev_line_len);
    }

    pub fn move_down(&mut self) {
        let current_line_start = self.chars[..self.cursor]
            .iter()
            .rposition(|&c| c == '\n')
            .map(|p| p + 1)
            .unwrap_or(0);

        let current_line_end = self.chars[self.cursor..]
            .iter()
            .position(|&c| c == '\n')
            .map(|p| self.cursor + p)
            .unwrap_or(self.chars.len());

        if current_line_end >= self.chars.len() {
            self.cursor = self.chars.len();
            return;
        }

        let col = self.cursor - current_line_start;
        let next_line_start = current_line_end + 1;
        let next_line_end = self.chars[next_line_start..]
            .iter()
            .position(|&c| c == '\n')
            .map(|p| next_line_start + p)
            .unwrap_or(self.chars.len());
        let next_line_len = next_line_end - next_line_start;
        self.cursor = next_line_start + col.min(next_line_len);
    }

    pub fn to_string(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn replace_range(&mut self, start: usize, end: usize, replacement: &str) {
        let new_chars: Vec<char> = replacement.chars().collect();
        let new_len = new_chars.len();
        self.chars.splice(start..end, new_chars);
        self.cursor = start + new_len;
    }

    pub fn toggle_todo(&mut self) {
        const TODO: &str = "{{[[TODO]]}} ";
        const DONE: &str = "{{[[DONE]]}} ";

        let text = self.to_string();
        if text.starts_with(DONE) {
            let prefix_len = DONE.chars().count();
            self.chars.drain(..prefix_len);
            self.cursor = self.cursor.saturating_sub(prefix_len);
        } else if text.starts_with(TODO) {
            let new_prefix: Vec<char> = DONE.chars().collect();
            for (i, ch) in new_prefix.iter().enumerate() {
                self.chars[i] = *ch;
            }
        } else {
            let prefix: Vec<char> = TODO.chars().collect();
            let prefix_len = prefix.len();
            for (i, ch) in prefix.into_iter().enumerate() {
                self.chars.insert(i, ch);
            }
            self.cursor += prefix_len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_cursor_at_end() {
        let buf = EditBuffer::new("hello");
        assert_eq!(buf.to_string(), "hello");
        assert_eq!(buf.cursor, 5);
    }

    #[test]
    fn new_empty() {
        let buf = EditBuffer::new_empty();
        assert_eq!(buf.to_string(), "");
        assert_eq!(buf.cursor, 0);
    }

    #[test]
    fn insert_char() {
        let mut buf = EditBuffer::new("hllo");
        buf.cursor = 1;
        buf.insert_char('e');
        assert_eq!(buf.to_string(), "hello");
        assert_eq!(buf.cursor, 2);
    }

    #[test]
    fn insert_at_end() {
        let mut buf = EditBuffer::new("hell");
        buf.insert_char('o');
        assert_eq!(buf.to_string(), "hello");
        assert_eq!(buf.cursor, 5);
    }

    #[test]
    fn delete_back() {
        let mut buf = EditBuffer::new("hello");
        buf.delete_back();
        assert_eq!(buf.to_string(), "hell");
        assert_eq!(buf.cursor, 4);
    }

    #[test]
    fn delete_back_at_start() {
        let mut buf = EditBuffer::new("hello");
        buf.cursor = 0;
        buf.delete_back();
        assert_eq!(buf.to_string(), "hello");
        assert_eq!(buf.cursor, 0);
    }

    #[test]
    fn delete_forward() {
        let mut buf = EditBuffer::new("hello");
        buf.cursor = 0;
        buf.delete_forward();
        assert_eq!(buf.to_string(), "ello");
        assert_eq!(buf.cursor, 0);
    }

    #[test]
    fn delete_forward_at_end() {
        let mut buf = EditBuffer::new("hello");
        buf.delete_forward();
        assert_eq!(buf.to_string(), "hello");
    }

    #[test]
    fn move_left_right() {
        let mut buf = EditBuffer::new("abc");
        assert_eq!(buf.cursor, 3);
        buf.move_left();
        assert_eq!(buf.cursor, 2);
        buf.move_left();
        assert_eq!(buf.cursor, 1);
        buf.move_right();
        assert_eq!(buf.cursor, 2);
    }

    #[test]
    fn move_left_stops_at_zero() {
        let mut buf = EditBuffer::new("a");
        buf.cursor = 0;
        buf.move_left();
        assert_eq!(buf.cursor, 0);
    }

    #[test]
    fn move_right_stops_at_end() {
        let mut buf = EditBuffer::new("a");
        buf.move_right();
        assert_eq!(buf.cursor, 1);
    }

    #[test]
    fn home_end() {
        let mut buf = EditBuffer::new("hello world");
        buf.move_home();
        assert_eq!(buf.cursor, 0);
        buf.move_end();
        assert_eq!(buf.cursor, 11);
    }

    #[test]
    fn word_jump_right() {
        let mut buf = EditBuffer::new("hello world foo");
        buf.cursor = 0;
        buf.move_word_right();
        assert_eq!(buf.cursor, 6);
        buf.move_word_right();
        assert_eq!(buf.cursor, 12);
        buf.move_word_right();
        assert_eq!(buf.cursor, 15);
    }

    #[test]
    fn word_jump_left() {
        let mut buf = EditBuffer::new("hello world foo");
        buf.move_word_left();
        assert_eq!(buf.cursor, 12);
        buf.move_word_left();
        assert_eq!(buf.cursor, 6);
        buf.move_word_left();
        assert_eq!(buf.cursor, 0);
    }

    #[test]
    fn unicode() {
        let mut buf = EditBuffer::new("café");
        assert_eq!(buf.chars.len(), 4);
        assert_eq!(buf.cursor, 4);
        buf.delete_back();
        assert_eq!(buf.to_string(), "caf");
        buf.insert_char('é');
        assert_eq!(buf.to_string(), "café");
    }

    #[test]
    fn toggle_todo_adds_prefix() {
        let mut buf = EditBuffer::new("buy milk");
        buf.toggle_todo();
        assert_eq!(buf.to_string(), "{{[[TODO]]}} buy milk");
    }

    #[test]
    fn toggle_todo_to_done() {
        let mut buf = EditBuffer::new("{{[[TODO]]}} buy milk");
        buf.toggle_todo();
        assert_eq!(buf.to_string(), "{{[[DONE]]}} buy milk");
    }

    #[test]
    fn toggle_done_removes_prefix() {
        let mut buf = EditBuffer::new("{{[[DONE]]}} buy milk");
        buf.toggle_todo();
        assert_eq!(buf.to_string(), "buy milk");
    }

    #[test]
    fn toggle_todo_full_cycle() {
        let mut buf = EditBuffer::new("task");
        buf.toggle_todo();
        assert_eq!(buf.to_string(), "{{[[TODO]]}} task");
        buf.toggle_todo();
        assert_eq!(buf.to_string(), "{{[[DONE]]}} task");
        buf.toggle_todo();
        assert_eq!(buf.to_string(), "task");
    }

    #[test]
    fn toggle_todo_cursor_adjusts() {
        let mut buf = EditBuffer::new("task");
        assert_eq!(buf.cursor, 4);
        let prefix_len = "{{[[TODO]]}} ".chars().count();
        buf.toggle_todo();
        assert_eq!(buf.cursor, 4 + prefix_len);
        buf.toggle_todo();
        assert_eq!(buf.cursor, 4 + prefix_len);
        buf.toggle_todo();
        assert_eq!(buf.cursor, 4);
    }

    #[test]
    fn insert_pair_parentheses() {
        let mut buf = EditBuffer::new("hello");
        buf.insert_pair('(', ')');
        assert_eq!(buf.to_string(), "hello()");
        assert_eq!(buf.cursor, 6); // between ( and )
    }

    #[test]
    fn insert_pair_brackets() {
        let mut buf = EditBuffer::new("hello");
        buf.insert_pair('[', ']');
        assert_eq!(buf.to_string(), "hello[]");
        assert_eq!(buf.cursor, 6);
    }

    #[test]
    fn insert_pair_braces() {
        let mut buf = EditBuffer::new("hello");
        buf.insert_pair('{', '}');
        assert_eq!(buf.to_string(), "hello{}");
        assert_eq!(buf.cursor, 6);
    }

    #[test]
    fn insert_pair_mid_text() {
        let mut buf = EditBuffer::new("ab");
        buf.cursor = 1;
        buf.insert_pair('(', ')');
        assert_eq!(buf.to_string(), "a()b");
        assert_eq!(buf.cursor, 2); // between ( and )
    }

    #[test]
    fn insert_pair_empty_buffer() {
        let mut buf = EditBuffer::new_empty();
        buf.insert_pair('(', ')');
        assert_eq!(buf.to_string(), "()");
        assert_eq!(buf.cursor, 1);
    }

    #[test]
    fn replace_range_basic() {
        let mut buf = EditBuffer::new("hello(())world");
        // cursor at 8 = between inner parens: hello((|))world
        buf.cursor = 8;
        buf.replace_range(5, 9, "((abc123))");
        assert_eq!(buf.to_string(), "hello((abc123))world");
        assert_eq!(buf.cursor, 15); // after the closing ))
    }

    #[test]
    fn replace_range_at_end() {
        let mut buf = EditBuffer::new("test(())");
        buf.cursor = 6;
        buf.replace_range(4, 8, "((xyz))");
        assert_eq!(buf.to_string(), "test((xyz))");
        assert_eq!(buf.cursor, 11);
    }

    #[test]
    fn replace_range_at_start() {
        let mut buf = EditBuffer::new("(())rest");
        buf.cursor = 2;
        buf.replace_range(0, 4, "((uid))");
        assert_eq!(buf.to_string(), "((uid))rest");
        assert_eq!(buf.cursor, 7);
    }

    #[test]
    fn empty_operations() {
        let mut buf = EditBuffer::new_empty();
        buf.delete_back();
        buf.delete_forward();
        buf.move_left();
        buf.move_right();
        buf.move_word_left();
        buf.move_word_right();
        buf.move_up();
        buf.move_down();
        assert_eq!(buf.cursor, 0);
        assert_eq!(buf.to_string(), "");
    }

    // --- move_up / move_down tests ---

    #[test]
    fn move_down_to_same_column() {
        let mut buf = EditBuffer::new("hello\nworld");
        buf.cursor = 2; // 'l' in "hello"
        buf.move_down();
        assert_eq!(buf.cursor, 8); // 'r' in "world"
    }

    #[test]
    fn move_up_to_same_column() {
        let mut buf = EditBuffer::new("hello\nworld");
        buf.cursor = 8; // 'r' in "world"
        buf.move_up();
        assert_eq!(buf.cursor, 2); // 'l' in "hello"
    }

    #[test]
    fn move_down_clamps_to_shorter_line() {
        let mut buf = EditBuffer::new("hello\nab");
        buf.cursor = 4; // 'o' in "hello"
        buf.move_down();
        assert_eq!(buf.cursor, 8); // end of "ab"
    }

    #[test]
    fn move_up_clamps_to_shorter_line() {
        let mut buf = EditBuffer::new("ab\nhello");
        buf.cursor = 7; // 'l' in "hello" (col 4)
        buf.move_up();
        assert_eq!(buf.cursor, 2); // end of "ab"
    }

    #[test]
    fn move_down_on_last_line_goes_to_end() {
        let mut buf = EditBuffer::new("hello\nworld");
        buf.cursor = 8; // 'r' in "world"
        buf.move_down();
        assert_eq!(buf.cursor, 11); // end of text
    }

    #[test]
    fn move_up_on_first_line_goes_to_start() {
        let mut buf = EditBuffer::new("hello\nworld");
        buf.cursor = 3; // second 'l' in "hello"
        buf.move_up();
        assert_eq!(buf.cursor, 0);
    }

    #[test]
    fn move_down_single_line_goes_to_end() {
        let mut buf = EditBuffer::new("hello");
        buf.cursor = 2;
        buf.move_down();
        assert_eq!(buf.cursor, 5);
    }

    #[test]
    fn move_up_single_line_goes_to_start() {
        let mut buf = EditBuffer::new("hello");
        buf.cursor = 3;
        buf.move_up();
        assert_eq!(buf.cursor, 0);
    }

    #[test]
    fn move_down_three_lines() {
        let mut buf = EditBuffer::new("aaa\nbbb\nccc");
        buf.cursor = 1; // col 1 in "aaa"
        buf.move_down();
        assert_eq!(buf.cursor, 5); // col 1 in "bbb"
        buf.move_down();
        assert_eq!(buf.cursor, 9); // col 1 in "ccc"
    }

    #[test]
    fn move_up_three_lines() {
        let mut buf = EditBuffer::new("aaa\nbbb\nccc");
        buf.cursor = 9; // col 1 in "ccc"
        buf.move_up();
        assert_eq!(buf.cursor, 5); // col 1 in "bbb"
        buf.move_up();
        assert_eq!(buf.cursor, 1); // col 1 in "aaa"
    }

    #[test]
    fn move_down_to_empty_line() {
        let mut buf = EditBuffer::new("hello\n\nworld");
        buf.cursor = 3; // col 3 in "hello"
        buf.move_down();
        assert_eq!(buf.cursor, 6); // on empty line (end = start)
    }

    #[test]
    fn move_up_from_empty_line() {
        let mut buf = EditBuffer::new("hello\n\nworld");
        buf.cursor = 6; // on empty line
        buf.move_up();
        assert_eq!(buf.cursor, 0); // col 0 in "hello"
    }
}
