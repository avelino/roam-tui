use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

use crate::api::types::{Block, DailyNote};
use crate::edit_buffer::EditBuffer;
use crate::highlight::CodeHighlighter;
use crate::markdown;

pub struct MainArea<'a> {
    pub days: &'a [DailyNote],
    pub selected_block: usize,
    pub loading: bool,
    pub loading_more: bool,
    pub edit_info: Option<EditInfo<'a>>,
    pub block_ref_cache: &'a std::collections::HashMap<String, String>,
}

pub struct EditInfo<'a> {
    pub buffer: &'a EditBuffer,
    pub block_index: usize,
}

#[derive(Debug, Clone)]
enum VisibleLine {
    DayHeading(String),
    DaySeparator,
    Block {
        depth: usize,
        text: String,
        block_index: usize,
        collapsed_children: usize,
    },
    CodeLabel {
        depth: usize,
        spans: Vec<Span<'static>>,
        block_index: usize,
    },
    CodeLine {
        depth: usize,
        spans: Vec<Span<'static>>,
        block_index: usize,
        line_number: usize,
    },
    Blockquote {
        depth: usize,
        text: String,
        block_index: usize,
    },
    LoadingMore,
}

/// Check if a block's text represents a code block (starts with ```)
fn is_code_block(text: &str) -> bool {
    text.starts_with("```")
}

/// Check if a block's text represents a blockquote (starts with `> `)
fn is_blockquote(text: &str) -> bool {
    text.starts_with("> ")
}

/// Markdown lang code blocks should be rendered with inline formatting, not as code
fn is_markdown_lang(lang: &str) -> bool {
    matches!(lang, "md" | "markdown" | "")
}

/// Parse a code block string into (language, code_lines).
/// Roam stores code blocks as: ```\nlang\ncode...```
fn parse_code_block(text: &str) -> (&str, &str) {
    let content = text.strip_prefix("```").unwrap_or(text);
    let content = content.strip_suffix("```").unwrap_or(content);
    let content = content.strip_prefix('\n').unwrap_or(content);

    // First line is the language
    if let Some(newline_pos) = content.find('\n') {
        let lang = content[..newline_pos].trim();
        let code = &content[newline_pos + 1..];
        (lang, code)
    } else {
        // No newline — just a language tag with no code
        (content.trim(), "")
    }
}

fn build_visible_lines(
    days: &[DailyNote],
    loading_more: bool,
    highlighter: &mut CodeHighlighter,
) -> Vec<VisibleLine> {
    let mut lines = Vec::new();
    let mut block_index = 0;

    for (i, day) in days.iter().enumerate() {
        if i > 0 {
            lines.push(VisibleLine::DaySeparator);
        }
        lines.push(VisibleLine::DayHeading(day.title.clone()));
        flatten_blocks(&day.blocks, 0, &mut lines, &mut block_index, highlighter);
    }

    if loading_more {
        lines.push(VisibleLine::DaySeparator);
        lines.push(VisibleLine::LoadingMore);
    }

    lines
}

fn flatten_blocks(
    blocks: &[Block],
    depth: usize,
    lines: &mut Vec<VisibleLine>,
    block_index: &mut usize,
    highlighter: &mut CodeHighlighter,
) {
    for block in blocks {
        if is_code_block(&block.string) {
            let (lang, code) = parse_code_block(&block.string);
            let bi = *block_index;

            let base_style = Style::default().fg(Color::White).bg(Color::DarkGray);

            // Language label line
            if !lang.is_empty() {
                lines.push(VisibleLine::CodeLabel {
                    depth,
                    spans: vec![Span::styled(
                        format!(" {} ", lang),
                        Style::default().fg(Color::DarkGray),
                    )],
                    block_index: bi,
                });
            }

            // Code lines
            if !code.is_empty() {
                let code_lines: Vec<Vec<Span<'static>>> = if is_markdown_lang(lang) {
                    code.lines()
                        .map(|line_text| markdown::render_spans(line_text, base_style))
                        .collect()
                } else {
                    highlighter.highlight_code(lang, code, base_style)
                };

                for (idx, spans) in code_lines.into_iter().enumerate() {
                    lines.push(VisibleLine::CodeLine {
                        depth,
                        spans,
                        block_index: bi,
                        line_number: idx + 1,
                    });
                }
            }
        } else if is_blockquote(&block.string) {
            lines.push(VisibleLine::Blockquote {
                depth,
                text: block.string[2..].to_string(),
                block_index: *block_index,
            });
        } else {
            let collapsed_children = if !block.open && !block.children.is_empty() {
                block.children.len()
            } else {
                0
            };
            lines.push(VisibleLine::Block {
                depth,
                text: block.string.clone(),
                block_index: *block_index,
                collapsed_children,
            });
        }

        *block_index += 1;
        if block.open {
            flatten_blocks(&block.children, depth + 1, lines, block_index, highlighter);
        }
    }
}

fn render_centered_message(msg: &str, area: Rect, buf: &mut Buffer) {
    if area.height > 0 {
        let line = Line::styled(msg, Style::default().fg(Color::DarkGray));
        let y = area.y + area.height / 2;
        let render_area = Rect::new(area.x, y, area.width, 1);
        line.render(render_area, buf);
    }
}

/// Reconstruct `Vec<Span<'static>>` from a slice of (char, Style) pairs,
/// merging consecutive chars that share the same style into a single Span.
fn chars_to_spans(chars: &[(char, Style)]) -> Vec<Span<'static>> {
    if chars.is_empty() {
        return vec![];
    }
    let mut spans = Vec::new();
    let mut current_text = String::new();
    let mut current_style = chars[0].1;

    for &(ch, style) in chars {
        if style == current_style {
            current_text.push(ch);
        } else {
            spans.push(Span::styled(current_text.clone(), current_style));
            current_text.clear();
            current_text.push(ch);
            current_style = style;
        }
    }
    if !current_text.is_empty() {
        spans.push(Span::styled(current_text, current_style));
    }
    spans
}

/// Word-wrap a sequence of styled spans into multiple lines.
///
/// - `first_width`: max chars available on the first line (after prefix like "  • ")
/// - `cont_width`: max chars available on continuation lines (after prefix like "    ")
///
/// Breaks at word boundaries (spaces) when possible, hard-wraps otherwise.
/// Returns `Vec<Vec<Span>>` where each inner Vec is one visual line.
fn wrap_spans(
    spans: Vec<Span<'static>>,
    first_width: usize,
    cont_width: usize,
) -> Vec<Vec<Span<'static>>> {
    let first_width = first_width.max(1);
    let cont_width = cont_width.max(1);

    // Flatten spans into (char, style) pairs
    let chars: Vec<(char, Style)> = spans
        .iter()
        .flat_map(|s| s.content.chars().map(move |c| (c, s.style)))
        .collect();

    let total_chars = chars.len();

    // Fast path: fits on first line
    if total_chars <= first_width {
        return vec![spans];
    }

    let mut result = Vec::new();
    let mut pos = 0;
    let mut is_first = true;

    while pos < total_chars {
        let width = if is_first { first_width } else { cont_width };
        let remaining = total_chars - pos;

        if remaining <= width {
            result.push(chars_to_spans(&chars[pos..]));
            break;
        }

        // Find break point: last space within [pos, pos+width)
        let end = pos + width;
        let break_at = chars[pos..end]
            .iter()
            .rposition(|&(c, _)| c == ' ')
            .map(|offset| pos + offset + 1) // break after space
            .unwrap_or(end); // hard break if no space

        result.push(chars_to_spans(&chars[pos..break_at]));
        pos = break_at;
        is_first = false;
    }

    result
}

impl<'a> Widget for MainArea<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.loading {
            render_centered_message(" Loading today's notes...", area, buf);
            return;
        }

        if self.days.is_empty() || self.days.iter().all(|d| d.blocks.is_empty()) {
            render_centered_message(" No notes for today", area, buf);
            return;
        }

        let mut highlighter = CodeHighlighter::new();
        let mut block_map = markdown::build_block_text_map(self.days);
        block_map.extend(
            self.block_ref_cache
                .iter()
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        let visible_lines = build_visible_lines(self.days, self.loading_more, &mut highlighter);
        let max_width = area.width as usize;

        // Phase 1: Build all visual rows (blocks may expand to multiple rows)
        let mut rows: Vec<Line<'static>> = Vec::new();
        let mut selected_row: usize = 0;
        let mut found_selected = false;

        for vline in &visible_lines {
            match vline {
                VisibleLine::DayHeading(title) => {
                    let text = format!("  {}", title);
                    let style = Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD);
                    let truncated = truncate(&text, max_width);
                    rows.push(Line::styled(truncated, style));
                }
                VisibleLine::DaySeparator => {
                    let sep = "─".repeat(max_width);
                    rows.push(Line::styled(
                        sep,
                        Style::default()
                            .fg(Color::DarkGray)
                            .add_modifier(Modifier::DIM),
                    ));
                }
                VisibleLine::Block {
                    depth,
                    text,
                    block_index,
                    collapsed_children,
                } => {
                    let indent = "  ".repeat(depth + 1);
                    let is_selected = *block_index == self.selected_block;

                    if !found_selected && is_selected {
                        selected_row = rows.len();
                        found_selected = true;
                    }

                    let is_editing = is_selected
                        && self
                            .edit_info
                            .as_ref()
                            .is_some_and(|e| e.block_index == *block_index);

                    if is_editing {
                        let edit = self.edit_info.as_ref().unwrap();
                        let style = Style::default().fg(Color::White).bg(Color::DarkGray);
                        let prefix = format!("{}• ", indent);
                        let cont_prefix = format!("{}  ", indent);
                        let buf_text = edit.buffer.to_string();
                        let cursor_pos = edit.buffer.cursor;

                        // Map flat cursor position to (line, col)
                        let text_lines: Vec<&str> = buf_text.split('\n').collect();
                        let mut line_start = 0;
                        let mut cursor_line = text_lines.len().saturating_sub(1);
                        let mut cursor_col = 0;

                        for (i, tl) in text_lines.iter().enumerate() {
                            let line_end = line_start + tl.chars().count();
                            if cursor_pos <= line_end {
                                cursor_line = i;
                                cursor_col = cursor_pos - line_start;
                                break;
                            }
                            line_start = line_end + 1; // skip \n
                        }

                        let edit_start_row = rows.len();
                        for (line_idx, text_line) in text_lines.iter().enumerate() {
                            let lp = if line_idx == 0 {
                                prefix.clone()
                            } else {
                                cont_prefix.clone()
                            };
                            let mut spans = vec![Span::styled(lp, style)];

                            if line_idx == cursor_line {
                                let line_chars: Vec<char> = text_line.chars().collect();
                                let before: String = line_chars[..cursor_col].iter().collect();
                                let cursor_char =
                                    line_chars.get(cursor_col).copied().unwrap_or(' ');
                                let after: String = if cursor_col < line_chars.len() {
                                    line_chars[cursor_col + 1..].iter().collect()
                                } else {
                                    String::new()
                                };

                                spans.push(Span::styled(before, style));
                                spans.push(Span::styled(
                                    cursor_char.to_string(),
                                    Style::default().fg(Color::Black).bg(Color::White),
                                ));
                                if !after.is_empty() {
                                    spans.push(Span::styled(after, style));
                                }
                            } else {
                                spans.push(Span::styled(text_line.to_string(), style));
                            }

                            rows.push(Line::from(spans));
                        }
                        // Scroll to the cursor row, not the first row of the block
                        selected_row = edit_start_row + cursor_line;
                    } else {
                        let mut style = if is_selected {
                            Style::default().fg(Color::White).bg(Color::DarkGray)
                        } else {
                            Style::default().fg(Color::Gray)
                        };

                        // Dim for deep nesting
                        if !is_selected && *depth >= 3 {
                            style = style.add_modifier(Modifier::DIM);
                        }

                        let bullet = if *collapsed_children > 0 {
                            "▸"
                        } else {
                            "•"
                        };
                        let bullet_style = if *collapsed_children > 0 {
                            Style::default().fg(Color::Cyan).bg(if is_selected {
                                Color::DarkGray
                            } else {
                                Color::Reset
                            })
                        } else {
                            style
                        };

                        // Selection indicator: replace first 2 chars of indent with "▎ "
                        let selection_indicator = if is_selected && indent.len() >= 2 {
                            Some(Span::styled(
                                "▎",
                                Style::default().fg(Color::Cyan).bg(Color::DarkGray),
                            ))
                        } else {
                            None
                        };

                        let prefix_indent = if selection_indicator.is_some() {
                            " ".repeat(indent.chars().count().saturating_sub(1))
                        } else {
                            indent.clone()
                        };

                        let cont_prefix = format!("{}  ", indent);
                        let prefix_width = indent.chars().count() + 2; // bullet + space
                        let cont_prefix_width = cont_prefix.chars().count();

                        let first_w = max_width.saturating_sub(prefix_width);
                        let cont_w = max_width.saturating_sub(cont_prefix_width);
                        let mut is_first_row = true;

                        for text_line in text.split('\n') {
                            let line_spans = markdown::render_spans_with_refs(
                                text_line,
                                style,
                                Some(&block_map),
                            );
                            let w = if is_first_row { first_w } else { cont_w };
                            let wrapped = wrap_spans(line_spans, w, cont_w);
                            for (wrap_idx, wline) in wrapped.into_iter().enumerate() {
                                let mut full_spans: Vec<Span<'static>> =
                                    if is_first_row && wrap_idx == 0 {
                                        let mut v: Vec<Span<'static>> = Vec::new();
                                        if let Some(ref ind) = selection_indicator {
                                            v.push(ind.clone());
                                        }
                                        v.push(Span::styled(prefix_indent.clone(), style));
                                        v.push(Span::styled(format!("{} ", bullet), bullet_style));
                                        v
                                    } else {
                                        vec![Span::styled(cont_prefix.clone(), style)]
                                    };
                                full_spans.extend(wline);

                                // Append collapsed children count on first row
                                if is_first_row && wrap_idx == 0 && *collapsed_children > 0 {
                                    full_spans.push(Span::styled(
                                        format!(" [{}]", collapsed_children),
                                        Style::default().fg(Color::DarkGray),
                                    ));
                                }

                                rows.push(Line::from(full_spans));
                                is_first_row = false;
                            }
                        }
                    }
                }
                VisibleLine::Blockquote {
                    depth,
                    text,
                    block_index,
                } => {
                    let indent = "  ".repeat(depth + 1);
                    let is_selected = *block_index == self.selected_block;

                    if !found_selected && is_selected {
                        selected_row = rows.len();
                        found_selected = true;
                    }

                    let border_style = if is_selected {
                        Style::default().fg(Color::DarkGray).bg(Color::DarkGray)
                    } else {
                        Style::default().fg(Color::DarkGray)
                    };

                    let text_style = if is_selected {
                        Style::default().fg(Color::White).bg(Color::DarkGray)
                    } else {
                        Style::default().fg(Color::Gray)
                    };

                    let prefix_str = format!("{}│ ", indent);
                    let prefix_width = prefix_str.chars().count();

                    let text_w = max_width.saturating_sub(prefix_width);

                    for text_line in text.split('\n') {
                        let line_spans = markdown::render_spans_with_refs(
                            text_line,
                            text_style,
                            Some(&block_map),
                        );
                        let wrapped = wrap_spans(line_spans, text_w, text_w);
                        for wline in wrapped {
                            let mut full_spans = vec![
                                Span::styled(indent.clone(), text_style),
                                Span::styled("│ ".to_string(), border_style),
                            ];
                            full_spans.extend(wline);
                            rows.push(Line::from(full_spans));
                        }
                    }
                }
                VisibleLine::CodeLabel {
                    depth,
                    spans,
                    block_index,
                }
                | VisibleLine::CodeLine {
                    depth,
                    spans,
                    block_index,
                    ..
                } => {
                    let indent = "  ".repeat(depth + 1);
                    let is_selected = *block_index == self.selected_block;

                    if !found_selected
                        && is_selected
                        && matches!(vline, VisibleLine::CodeLabel { .. })
                    {
                        selected_row = rows.len();
                        found_selected = true;
                    }

                    let indent_style = if is_selected {
                        Style::default().bg(Color::DarkGray)
                    } else {
                        Style::default()
                    };

                    let mut line_spans = vec![Span::styled(format!("{}  ", indent), indent_style)];

                    if let VisibleLine::CodeLine { line_number, .. } = vline {
                        let gutter_style = Style::default().fg(Color::Gray).bg(Color::DarkGray);
                        line_spans.push(Span::styled(format!("{:>3} ", line_number), gutter_style));
                    }

                    line_spans.extend(spans.iter().cloned());

                    // Pad background to full width
                    let used: usize = line_spans.iter().map(|s| s.content.chars().count()).sum();
                    let remaining = max_width.saturating_sub(used);
                    if remaining > 0 {
                        line_spans.push(Span::styled(
                            " ".repeat(remaining),
                            Style::default().bg(Color::DarkGray),
                        ));
                    }

                    rows.push(Line::from(line_spans));
                }
                VisibleLine::LoadingMore => {
                    rows.push(Line::styled(
                        "  Loading more...".to_string(),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
            }
        }

        // Phase 2: Scroll (half-page centering on selected row)
        let viewport_height = area.height as usize;
        let half = viewport_height / 2;
        let scroll_offset = if selected_row > half {
            (selected_row - half).min(rows.len().saturating_sub(viewport_height))
        } else {
            0
        };

        // Phase 3: Render rows to terminal
        for (i, row) in rows.into_iter().skip(scroll_offset).enumerate() {
            if i >= viewport_height {
                break;
            }
            let y = area.y + i as u16;
            let render_area = Rect::new(area.x, y, area.width, 1);
            row.render(render_area, buf);
        }
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    s.chars().take(max_len).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;
    use std::collections::HashMap;

    fn read_line(buf: &Buffer, y: u16, width: u16) -> String {
        (0..width)
            .map(|x| {
                buf.cell((x, y))
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect()
    }

    fn make_block(uid: &str, text: &str, order: i64) -> Block {
        Block {
            uid: uid.into(),
            string: text.into(),
            order,
            children: vec![],
            open: true,
        }
    }

    fn make_daily_note(
        title: &str,
        year: i32,
        month: u32,
        day: u32,
        blocks: Vec<Block>,
    ) -> DailyNote {
        DailyNote {
            date: NaiveDate::from_ymd_opt(year, month, day).unwrap(),
            uid: format!("{:02}-{:02}-{}", month, day, year),
            title: title.into(),
            blocks,
        }
    }

    #[test]
    fn build_visible_lines_single_day() {
        let day = make_daily_note(
            "February 21, 2026",
            2026,
            2,
            21,
            vec![make_block("b1", "Hello", 0), make_block("b2", "World", 1)],
        );
        let mut hl = CodeHighlighter::new();
        let lines = build_visible_lines(&[day], false, &mut hl);

        assert_eq!(lines.len(), 3); // heading + 2 blocks
        assert!(matches!(&lines[0], VisibleLine::DayHeading(t) if t == "February 21, 2026"));
        assert!(
            matches!(&lines[1], VisibleLine::Block { block_index: 0, text, .. } if text == "Hello")
        );
        assert!(
            matches!(&lines[2], VisibleLine::Block { block_index: 1, text, .. } if text == "World")
        );
    }

    #[test]
    fn build_visible_lines_nested_blocks() {
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![make_block("c1", "Child", 0)],
            open: true,
        };
        let day = make_daily_note("Feb 21", 2026, 2, 21, vec![parent]);
        let mut hl = CodeHighlighter::new();
        let lines = build_visible_lines(&[day], false, &mut hl);

        assert_eq!(lines.len(), 3); // heading + parent + child
        assert!(matches!(
            &lines[1],
            VisibleLine::Block {
                depth: 0,
                block_index: 0,
                ..
            }
        ));
        assert!(matches!(
            &lines[2],
            VisibleLine::Block {
                depth: 1,
                block_index: 1,
                ..
            }
        ));
    }

    #[test]
    fn build_visible_lines_two_days_has_separator() {
        let day1 = make_daily_note("Day 1", 2026, 2, 21, vec![make_block("a", "A", 0)]);
        let day2 = make_daily_note("Day 2", 2026, 2, 20, vec![make_block("b", "B", 0)]);
        let mut hl = CodeHighlighter::new();
        let lines = build_visible_lines(&[day1, day2], false, &mut hl);

        assert_eq!(lines.len(), 5);
        assert!(matches!(&lines[2], VisibleLine::DaySeparator));
        assert!(matches!(&lines[3], VisibleLine::DayHeading(t) if t == "Day 2"));
    }

    #[test]
    fn build_visible_lines_loading_more() {
        let day = make_daily_note("Day 1", 2026, 2, 21, vec![make_block("a", "A", 0)]);
        let mut hl = CodeHighlighter::new();
        let lines = build_visible_lines(&[day], true, &mut hl);

        let last = lines.last().unwrap();
        assert!(matches!(last, VisibleLine::LoadingMore));
    }

    #[test]
    fn renders_loading_state() {
        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);

        let widget = MainArea {
            days: &[],
            selected_block: 0,
            loading: true,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        let found = (0..area.height).any(|y| read_line(&buf, y, area.width).contains("Loading"));
        assert!(found);
    }

    #[test]
    fn renders_empty_state() {
        let area = Rect::new(0, 0, 40, 5);
        let mut buf = Buffer::empty(area);

        let widget = MainArea {
            days: &[],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        let found = (0..area.height).any(|y| read_line(&buf, y, area.width).contains("No notes"));
        assert!(found);
    }

    #[test]
    fn renders_day_heading_and_blocks() {
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);

        let day = make_daily_note(
            "February 21, 2026",
            2026,
            2,
            21,
            vec![
                make_block("b1", "Block one", 0),
                make_block("b2", "Block two", 1),
            ],
        );

        let widget = MainArea {
            days: &[day],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        assert!(read_line(&buf, 0, area.width).contains("February 21, 2026"));
        assert!(read_line(&buf, 1, area.width).contains("Block one"));
    }

    #[test]
    fn renders_selected_block_with_bullet() {
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);

        let day = make_daily_note(
            "Feb 21",
            2026,
            2,
            21,
            vec![make_block("b1", "First", 0), make_block("b2", "Second", 1)],
        );

        let widget = MainArea {
            days: &[day],
            selected_block: 1,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        assert!(read_line(&buf, 2, area.width).contains("Second"));
    }

    #[test]
    fn renders_nested_block_with_indentation() {
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);

        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![make_block("c1", "Child", 0)],
            open: true,
        };
        let day = make_daily_note("Feb 21", 2026, 2, 21, vec![parent]);

        let widget = MainArea {
            days: &[day],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        let line1 = read_line(&buf, 1, area.width);
        let line2 = read_line(&buf, 2, area.width);

        assert!(line1.contains("Parent"));
        assert!(line2.contains("Child"));
        let parent_leading = line1.len() - line1.trim_start().len();
        let child_leading = line2.len() - line2.trim_start().len();
        assert!(child_leading > parent_leading);
    }

    #[test]
    fn code_block_expands_to_multiple_lines() {
        let code_text = "```\nrust\nfn main() {}\nlet x = 1;```";
        let day = make_daily_note(
            "Feb 21",
            2026,
            2,
            21,
            vec![make_block("c1", code_text, 0), make_block("b2", "After", 1)],
        );
        let mut hl = CodeHighlighter::new();
        let lines = build_visible_lines(&[day], false, &mut hl);

        // heading + lang label + 2 code lines + "After" block = 5
        let code_line_count = lines
            .iter()
            .filter(|l| matches!(l, VisibleLine::CodeLine { .. }))
            .count();
        assert!(
            code_line_count >= 2,
            "Expected at least 2 code lines, got {}",
            code_line_count
        );

        // "After" block should still be there
        let has_after = lines
            .iter()
            .any(|l| matches!(l, VisibleLine::Block { text, .. } if text == "After"));
        assert!(has_after, "Expected 'After' block in visible lines");
    }

    #[test]
    fn code_lines_have_line_numbers() {
        let code_text = "```\nrust\nfn main() {}\nlet x = 1;```";
        let day = make_daily_note("Feb 21", 2026, 2, 21, vec![make_block("c1", code_text, 0)]);
        let mut hl = CodeHighlighter::new();
        let lines = build_visible_lines(&[day], false, &mut hl);

        let code_lines: Vec<_> = lines
            .iter()
            .filter(|l| matches!(l, VisibleLine::CodeLine { .. }))
            .collect();

        assert!(code_lines.len() >= 2);
        if let VisibleLine::CodeLine { line_number, .. } = &code_lines[0] {
            assert_eq!(*line_number, 1);
        }
        if let VisibleLine::CodeLine { line_number, .. } = &code_lines[1] {
            assert_eq!(*line_number, 2);
        }
    }

    #[test]
    fn code_label_is_separate_variant() {
        let code_text = "```\nrust\nfn main() {}```";
        let day = make_daily_note("Feb 21", 2026, 2, 21, vec![make_block("c1", code_text, 0)]);
        let mut hl = CodeHighlighter::new();
        let lines = build_visible_lines(&[day], false, &mut hl);

        let label = lines
            .iter()
            .find(|l| matches!(l, VisibleLine::CodeLabel { .. }));
        assert!(
            label.is_some(),
            "Expected a CodeLabel variant for the language label"
        );
    }

    #[test]
    fn code_line_number_rendered_in_buffer() {
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);

        let code_text = "```\nrust\nfn main() {}\nlet x = 1;```";
        let day = make_daily_note("Feb 21", 2026, 2, 21, vec![make_block("c1", code_text, 0)]);

        let widget = MainArea {
            days: &[day],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        // Line 2 (after heading + label) should contain "1" (line number)
        // Line 3 should contain "2"
        let line2 = read_line(&buf, 2, area.width);
        let line3 = read_line(&buf, 3, area.width);
        assert!(
            line2.contains('1'),
            "Expected line number '1' in '{}'",
            line2
        );
        assert!(line2.contains("fn"), "Expected code content in '{}'", line2);
        assert!(
            line3.contains('2'),
            "Expected line number '2' in '{}'",
            line3
        );
    }

    #[test]
    fn renders_bold_text_with_styling() {
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);

        let day = make_daily_note(
            "Feb 21",
            2026,
            2,
            21,
            vec![make_block("b1", "**bold text**", 0)],
        );

        let widget = MainArea {
            days: &[day],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        // The rendered line should contain "bold text" (without **)
        let line1 = read_line(&buf, 1, area.width);
        assert!(
            line1.contains("bold text"),
            "Expected 'bold text', got: '{}'",
            line1
        );
        assert!(
            !line1.contains("**"),
            "Should not contain ** delimiters, got: '{}'",
            line1
        );
    }

    #[test]
    fn renders_page_link_without_brackets() {
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);

        let day = make_daily_note(
            "Feb 21",
            2026,
            2,
            21,
            vec![make_block("b1", "See [[my page]]", 0)],
        );

        let widget = MainArea {
            days: &[day],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        assert!(
            read_line(&buf, 1, area.width).contains("my page"),
            "Expected 'my page'"
        );
    }

    #[test]
    fn blockquote_detected_in_visible_lines() {
        let day = make_daily_note(
            "Feb 21",
            2026,
            2,
            21,
            vec![make_block("b1", "> quoted text", 0)],
        );
        let mut hl = CodeHighlighter::new();
        let lines = build_visible_lines(&[day], false, &mut hl);

        assert!(matches!(
            &lines[1],
            VisibleLine::Blockquote { text, block_index: 0, .. } if text == "quoted text"
        ));
    }

    #[test]
    fn blockquote_strips_prefix() {
        let day = make_daily_note(
            "Feb 21",
            2026,
            2,
            21,
            vec![make_block("b1", "> hello world", 0)],
        );
        let mut hl = CodeHighlighter::new();
        let lines = build_visible_lines(&[day], false, &mut hl);

        if let VisibleLine::Blockquote { text, .. } = &lines[1] {
            assert!(
                !text.starts_with("> "),
                "Text should not contain '> ' prefix, got: '{}'",
                text
            );
            assert_eq!(text, "hello world");
        } else {
            panic!("Expected Blockquote variant, got: {:?}", &lines[1]);
        }
    }

    #[test]
    fn blockquote_preserves_depth() {
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![make_block("c1", "> nested quote", 0)],
            open: true,
        };
        let day = make_daily_note("Feb 21", 2026, 2, 21, vec![parent]);
        let mut hl = CodeHighlighter::new();
        let lines = build_visible_lines(&[day], false, &mut hl);

        assert!(matches!(
            &lines[2],
            VisibleLine::Blockquote { depth: 1, text, block_index: 1, .. } if text == "nested quote"
        ));
    }

    #[test]
    fn blockquote_renders_with_border() {
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);

        let day = make_daily_note(
            "Feb 21",
            2026,
            2,
            21,
            vec![make_block("b1", "> quoted text", 0)],
        );

        let widget = MainArea {
            days: &[day],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        let line1 = read_line(&buf, 1, area.width);
        assert!(
            line1.contains('│'),
            "Expected '│' border in blockquote, got: '{}'",
            line1
        );
        assert!(
            line1.contains("quoted text"),
            "Expected 'quoted text', got: '{}'",
            line1
        );
    }

    #[test]
    fn blockquote_with_inline_formatting() {
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);

        let day = make_daily_note(
            "Feb 21",
            2026,
            2,
            21,
            vec![make_block("b1", "> **bold** and [[link]]", 0)],
        );

        let widget = MainArea {
            days: &[day],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        let line1 = read_line(&buf, 1, area.width);
        assert!(
            line1.contains("bold"),
            "Expected 'bold' in blockquote, got: '{}'",
            line1
        );
        assert!(
            line1.contains("link"),
            "Expected 'link' in blockquote, got: '{}'",
            line1
        );
        assert!(
            !line1.contains("**"),
            "Should not contain ** delimiters, got: '{}'",
            line1
        );
    }

    #[test]
    fn regular_block_not_affected_by_blockquote() {
        let day = make_daily_note(
            "Feb 21",
            2026,
            2,
            21,
            vec![
                make_block("b1", "normal text", 0),
                make_block("b2", "> quoted", 1),
                make_block("b3", "also normal", 2),
            ],
        );
        let mut hl = CodeHighlighter::new();
        let lines = build_visible_lines(&[day], false, &mut hl);

        assert!(matches!(&lines[1], VisibleLine::Block { text, .. } if text == "normal text"));
        assert!(matches!(&lines[2], VisibleLine::Blockquote { text, .. } if text == "quoted"));
        assert!(matches!(&lines[3], VisibleLine::Block { text, .. } if text == "also normal"));
    }

    #[test]
    fn renders_todo_as_checkbox() {
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);

        let day = make_daily_note(
            "Feb 21",
            2026,
            2,
            21,
            vec![make_block("b1", "{{TODO}} buy milk", 0)],
        );

        let widget = MainArea {
            days: &[day],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        assert!(
            read_line(&buf, 1, area.width).contains("buy milk"),
            "Expected 'buy milk'"
        );
    }

    // --- wrap_spans tests ---

    fn collect_line_text(spans: &[Span]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn wrap_spans_short_text_no_wrap() {
        let style = Style::default().fg(Color::White);
        let spans = vec![Span::styled("hello", style)];
        let result = wrap_spans(spans, 20, 20);
        assert_eq!(result.len(), 1);
        assert_eq!(collect_line_text(&result[0]), "hello");
    }

    #[test]
    fn wrap_spans_long_text_wraps_at_word() {
        let style = Style::default().fg(Color::White);
        let spans = vec![Span::styled("hello world foo", style)];
        let result = wrap_spans(spans, 10, 10);
        assert_eq!(result.len(), 2);
        assert_eq!(collect_line_text(&result[0]), "hello ");
        assert_eq!(collect_line_text(&result[1]), "world foo");
    }

    #[test]
    fn wrap_spans_no_space_hard_wraps() {
        let style = Style::default().fg(Color::White);
        let spans = vec![Span::styled("abcdefghijklmno", style)];
        let result = wrap_spans(spans, 10, 10);
        assert_eq!(result.len(), 2);
        assert_eq!(collect_line_text(&result[0]), "abcdefghij");
        assert_eq!(collect_line_text(&result[1]), "klmno");
    }

    #[test]
    fn wrap_spans_preserves_styles() {
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let normal = Style::default();
        let spans = vec![
            Span::styled("aaaa", bold),
            Span::styled(" bbbb cccc", normal),
        ];
        // "aaaa bbbb cccc" = 14 chars, width 10
        // Line 1: "aaaa bbbb " (break at last space within 10 chars)
        // Line 2: "cccc"
        let result = wrap_spans(spans, 10, 10);
        assert_eq!(result.len(), 2);

        // First line has mixed styles
        assert_eq!(result[0].len(), 2);
        assert_eq!(result[0][0].content, "aaaa");
        assert!(result[0][0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(result[0][1].content, " bbbb ");
        assert!(!result[0][1].style.add_modifier.contains(Modifier::BOLD));

        // Second line is normal style
        assert_eq!(collect_line_text(&result[1]), "cccc");
    }

    #[test]
    fn wrap_spans_different_first_cont_widths() {
        let style = Style::default();
        let spans = vec![Span::styled("aaa bbb ccc ddd eee", style)];
        // 19 chars, first_width=15, cont_width=8
        let result = wrap_spans(spans, 15, 8);
        assert_eq!(result.len(), 2);
        // First line fits within 15 chars
        let line1 = collect_line_text(&result[0]);
        assert!(
            line1.len() <= 15,
            "Line 1 '{}' exceeds first_width 15",
            line1
        );
        // Second line fits within 8 chars
        let line2 = collect_line_text(&result[1]);
        assert!(line2.len() <= 8, "Line 2 '{}' exceeds cont_width 8", line2);
    }

    #[test]
    fn wrap_spans_empty_returns_single_empty_line() {
        let result = wrap_spans(vec![], 10, 10);
        assert_eq!(result.len(), 1);
        assert!(result[0].is_empty());
    }

    #[test]
    fn multiline_block_respects_newlines() {
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);

        let text = "First paragraph\n\nSecond paragraph";
        let day = make_daily_note("Feb 21", 2026, 2, 21, vec![make_block("b1", text, 0)]);

        let widget = MainArea {
            days: &[day],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        // Row 0: heading
        // Row 1: "  • First paragraph"
        // Row 2: "    " (empty line from \n\n)
        // Row 3: "    Second paragraph"
        let line1 = read_line(&buf, 1, area.width);
        let line2 = read_line(&buf, 2, area.width);
        let line3 = read_line(&buf, 3, area.width);
        assert!(line1.contains("First paragraph"), "Row 1: '{}'", line1);
        assert!(line1.contains('•'), "Row 1 should have bullet: '{}'", line1);
        assert!(
            !line2.contains("Second"),
            "Row 2 should be blank separator: '{}'",
            line2
        );
        assert!(line3.contains("Second paragraph"), "Row 3: '{}'", line3);
        assert!(
            !line3.contains('•'),
            "Row 3 should not have bullet: '{}'",
            line3
        );
    }

    #[test]
    fn long_block_wraps_in_visible_output() {
        let area = Rect::new(0, 0, 30, 10);
        let mut buf = Buffer::empty(area);

        // "  • " prefix is 4 chars, leaving 26 chars for text
        // This text is ~40 chars, should wrap to 2+ lines
        let long_text = "this is a very long block text that should wrap";
        let day = make_daily_note("Feb 21", 2026, 2, 21, vec![make_block("b1", long_text, 0)]);

        let widget = MainArea {
            days: &[day],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        // Row 0: heading
        // Row 1: first line of wrapped block (with bullet)
        // Row 2: continuation line (with indent)
        let line1 = read_line(&buf, 1, area.width);
        let line2 = read_line(&buf, 2, area.width);
        assert!(
            line1.contains("this"),
            "Expected first part of text on row 1, got: '{}'",
            line1
        );
        assert!(
            !line2.trim().is_empty(),
            "Expected continuation on row 2, got: '{}'",
            line2
        );
        // Continuation should NOT have a bullet
        assert!(
            !line2.contains('•'),
            "Continuation line should not have bullet, got: '{}'",
            line2
        );
    }

    #[test]
    fn test_collapsed_block_shows_arrow_indicator() {
        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![make_block("c1", "Child", 0)],
            open: false,
        };
        let day = make_daily_note("Feb 21", 2026, 2, 21, vec![parent]);
        let mut hl = CodeHighlighter::new();
        let lines = build_visible_lines(&[day], false, &mut hl);

        assert!(matches!(
            &lines[1],
            VisibleLine::Block {
                collapsed_children: 1,
                ..
            }
        ));
    }

    #[test]
    fn test_collapsed_block_shows_children_count() {
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);

        let parent = Block {
            uid: "p".into(),
            string: "Parent".into(),
            order: 0,
            children: vec![
                make_block("c1", "Child1", 0),
                make_block("c2", "Child2", 1),
                make_block("c3", "Child3", 2),
            ],
            open: false,
        };
        let day = make_daily_note("Feb 21", 2026, 2, 21, vec![parent]);

        let widget = MainArea {
            days: &[day],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        let line1 = read_line(&buf, 1, area.width);
        assert!(
            line1.contains("▸"),
            "Expected '▸' for collapsed block, got: '{}'",
            line1
        );
        assert!(
            line1.contains("[3]"),
            "Expected '[3]' children count, got: '{}'",
            line1
        );
    }

    #[test]
    fn test_open_block_shows_bullet() {
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);

        let day = make_daily_note(
            "Feb 21",
            2026,
            2,
            21,
            vec![make_block("b1", "Normal block", 0)],
        );

        let widget = MainArea {
            days: &[day],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        let line1 = read_line(&buf, 1, area.width);
        assert!(
            line1.contains('•'),
            "Expected '•' for open block, got: '{}'",
            line1
        );
        assert!(
            !line1.contains('▸'),
            "Should not contain '▸' for open block, got: '{}'",
            line1
        );
    }

    #[test]
    fn test_day_separator_rendered() {
        let area = Rect::new(0, 0, 40, 10);
        let mut buf = Buffer::empty(area);

        let day1 = make_daily_note("Day 1", 2026, 2, 21, vec![make_block("a", "A", 0)]);
        let day2 = make_daily_note("Day 2", 2026, 2, 20, vec![make_block("b", "B", 0)]);

        let widget = MainArea {
            days: &[day1, day2],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        // Row 0: Day 1 heading
        // Row 1: block A
        // Row 2: separator (─ chars)
        // Row 3: Day 2 heading
        let sep_line = read_line(&buf, 2, area.width);
        assert!(
            sep_line.contains('─'),
            "Expected '─' separator between days, got: '{}'",
            sep_line
        );
    }

    #[test]
    fn test_selected_block_has_indicator() {
        let area = Rect::new(0, 0, 60, 10);
        let mut buf = Buffer::empty(area);

        let day = make_daily_note(
            "Feb 21",
            2026,
            2,
            21,
            vec![make_block("b1", "First", 0), make_block("b2", "Second", 1)],
        );

        let widget = MainArea {
            days: &[day],
            selected_block: 0,
            loading: false,
            loading_more: false,
            edit_info: None,
            block_ref_cache: &HashMap::new(),
        };
        widget.render(area, &mut buf);

        let line1 = read_line(&buf, 1, area.width);
        assert!(
            line1.contains('▎'),
            "Expected '▎' selection indicator, got: '{}'",
            line1
        );

        // Non-selected block should NOT have indicator
        let line2 = read_line(&buf, 2, area.width);
        assert!(
            !line2.contains('▎'),
            "Non-selected should not have '▎', got: '{}'",
            line2
        );
    }
}
