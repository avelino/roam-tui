use std::collections::HashMap;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::app::Selection;
use crate::markdown;

use super::text_wrap::{inject_cursor, truncate, wrap_spans};
use super::view::EditInfo;
use super::visible_lines::VisibleLine;

pub(crate) struct RowBuilder<'a> {
    selection: &'a Selection,
    edit_info: Option<&'a EditInfo<'a>>,
    cursor_col: usize,
    block_map: &'a HashMap<String, String>,
    max_width: usize,
    rows: Vec<Line<'static>>,
    selected_row: usize,
    found_selected: bool,
}

impl<'a> RowBuilder<'a> {
    pub fn new(
        selection: &'a Selection,
        edit_info: Option<&'a EditInfo<'a>>,
        cursor_col: usize,
        block_map: &'a HashMap<String, String>,
        max_width: usize,
    ) -> Self {
        Self {
            selection,
            edit_info,
            cursor_col,
            block_map,
            max_width,
            rows: Vec::new(),
            selected_row: 0,
            found_selected: false,
        }
    }

    /// Consume builder, return (rows, selected_row).
    pub fn build(mut self, visible_lines: &[VisibleLine]) -> (Vec<Line<'static>>, usize) {
        for vline in visible_lines {
            match vline {
                VisibleLine::DayHeading(title) => self.push_heading(title),
                VisibleLine::DaySeparator | VisibleLine::LinkedRefsSeparator => {
                    self.push_separator()
                }
                VisibleLine::Block {
                    depth,
                    text,
                    block_index,
                    collapsed_children,
                } => self.push_block(*depth, text, *block_index, *collapsed_children),
                VisibleLine::Blockquote {
                    depth,
                    text,
                    block_index,
                } => self.push_blockquote(*depth, text, *block_index),
                VisibleLine::CodeLabel {
                    depth,
                    spans,
                    block_index,
                } => self.push_code(*depth, spans, *block_index, true, None),
                VisibleLine::CodeLine {
                    depth,
                    spans,
                    block_index,
                    line_number,
                } => self.push_code(*depth, spans, *block_index, false, Some(*line_number)),
                VisibleLine::LoadingMore => {
                    self.rows.push(Line::styled(
                        "  Loading more...".to_string(),
                        Style::default().fg(Color::DarkGray),
                    ));
                }
                VisibleLine::LinkedRefsHeader {
                    count,
                    collapsed,
                    block_index,
                } => self.push_linked_refs_header(*count, *collapsed, *block_index),
                VisibleLine::LinkedRefsGroupHeader {
                    page_title,
                    block_index,
                } => self.push_linked_refs_group(page_title, *block_index),
                VisibleLine::LinkedRefsBlock {
                    text, block_index, ..
                } => self.push_linked_refs_block(text, *block_index),
            }
        }
        (self.rows, self.selected_row)
    }

    fn track_selection(&mut self, block_index: usize) -> bool {
        let is_selected = self.selection.contains(block_index);
        if !self.found_selected && is_selected {
            self.selected_row = self.rows.len();
            self.found_selected = true;
        }
        is_selected
    }

    fn selected_style() -> Style {
        Style::default().fg(Color::White).bg(Color::DarkGray)
    }

    // -- Heading & separator --

    fn push_heading(&mut self, title: &str) {
        let text = format!("  {}", title);
        let style = Style::default()
            .fg(Color::White)
            .add_modifier(Modifier::BOLD);
        self.rows
            .push(Line::styled(truncate(&text, self.max_width), style));
    }

    fn push_separator(&mut self) {
        let sep = "─".repeat(self.max_width);
        self.rows.push(Line::styled(
            sep,
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        ));
    }

    // -- Block (the big one, split into edit vs normal) --

    fn push_block(&mut self, depth: usize, text: &str, block_index: usize, collapsed: usize) {
        let indent = "  ".repeat(depth + 1);
        let is_selected = self.track_selection(block_index);

        let is_editing = is_selected
            && self
                .edit_info
                .as_ref()
                .is_some_and(|e| e.block_index == block_index);

        if is_editing {
            self.push_block_editing(&indent);
        } else {
            self.push_block_normal(text, depth, &indent, is_selected, collapsed);
        }
    }

    fn push_block_editing(&mut self, indent: &str) {
        let edit = self.edit_info.as_ref().unwrap();
        let style = Self::selected_style();
        let prefix = format!("{}• ", indent);
        let cont_prefix = format!("{}  ", indent);
        let buf_text = edit.buffer.to_string();
        let cursor_pos = edit.buffer.cursor;

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
            line_start = line_end + 1;
        }

        let edit_start_row = self.rows.len();
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
                let cursor_char = line_chars.get(cursor_col).copied().unwrap_or(' ');
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

            self.rows.push(Line::from(spans));
        }
        self.selected_row = edit_start_row + cursor_line;
    }

    fn push_block_normal(
        &mut self,
        text: &str,
        depth: usize,
        indent: &str,
        is_selected: bool,
        collapsed: usize,
    ) {
        let mut style = if is_selected {
            Self::selected_style()
        } else {
            Style::default().fg(Color::Gray)
        };

        if !is_selected && depth >= 3 {
            style = style.add_modifier(Modifier::DIM);
        }

        let bullet = if collapsed > 0 { "▸" } else { "•" };
        let bullet_style = if collapsed > 0 {
            Style::default().fg(Color::Cyan).bg(if is_selected {
                Color::DarkGray
            } else {
                Color::Reset
            })
        } else {
            style
        };

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
            indent.to_string()
        };

        let cont_prefix = format!("{}  ", indent);
        let prefix_width = indent.chars().count() + 2;
        let cont_prefix_width = cont_prefix.chars().count();
        let first_w = self.max_width.saturating_sub(prefix_width);
        let cont_w = self.max_width.saturating_sub(cont_prefix_width);
        let mut is_first_row = true;

        let mut rendered_char_offset = 0;
        for text_line in text.split('\n') {
            let mut line_spans =
                markdown::render_spans_with_refs(text_line, style, Some(self.block_map));

            if is_selected && self.edit_info.is_none() {
                let line_rendered_len: usize =
                    line_spans.iter().map(|s| s.content.chars().count()).sum();
                if self.cursor_col >= rendered_char_offset
                    && self.cursor_col - rendered_char_offset <= line_rendered_len
                {
                    let cursor_in_line = self.cursor_col - rendered_char_offset;
                    line_spans = inject_cursor(line_spans, cursor_in_line);
                }
                rendered_char_offset += line_rendered_len + 1;
            }

            let w = if is_first_row { first_w } else { cont_w };
            let wrapped = wrap_spans(line_spans, w, cont_w);
            for (wrap_idx, wline) in wrapped.into_iter().enumerate() {
                let mut full_spans: Vec<Span<'static>> = if is_first_row && wrap_idx == 0 {
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

                if is_first_row && wrap_idx == 0 && collapsed > 0 {
                    full_spans.push(Span::styled(
                        format!(" [{}]", collapsed),
                        Style::default().fg(Color::DarkGray),
                    ));
                }

                self.rows.push(Line::from(full_spans));
                is_first_row = false;
            }
        }
    }

    // -- Blockquote --

    fn push_blockquote(&mut self, depth: usize, text: &str, block_index: usize) {
        let indent = "  ".repeat(depth + 1);
        let is_selected = self.track_selection(block_index);

        let border_style = if is_selected {
            Style::default().fg(Color::DarkGray).bg(Color::DarkGray)
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let text_style = if is_selected {
            Self::selected_style()
        } else {
            Style::default().fg(Color::Gray)
        };

        let prefix_str = format!("{}│ ", indent);
        let prefix_width = prefix_str.chars().count();
        let text_w = self.max_width.saturating_sub(prefix_width);

        for text_line in text.split('\n') {
            let line_spans =
                markdown::render_spans_with_refs(text_line, text_style, Some(self.block_map));
            let wrapped = wrap_spans(line_spans, text_w, text_w);
            for wline in wrapped {
                let mut full_spans = vec![
                    Span::styled(indent.clone(), text_style),
                    Span::styled("│ ".to_string(), border_style),
                ];
                full_spans.extend(wline);
                self.rows.push(Line::from(full_spans));
            }
        }
    }

    // -- Code blocks --

    fn push_code(
        &mut self,
        depth: usize,
        spans: &[Span<'static>],
        block_index: usize,
        is_label: bool,
        line_number: Option<usize>,
    ) {
        let indent = "  ".repeat(depth + 1);
        let is_selected = self.selection.contains(block_index);

        if !self.found_selected && is_selected && is_label {
            self.selected_row = self.rows.len();
            self.found_selected = true;
        }

        let indent_style = if is_selected {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        };

        let mut line_spans = vec![Span::styled(format!("{}  ", indent), indent_style)];

        if let Some(num) = line_number {
            let gutter_style = Style::default().fg(Color::Gray).bg(Color::DarkGray);
            line_spans.push(Span::styled(format!("{:>3} ", num), gutter_style));
        }

        line_spans.extend(spans.iter().cloned());

        let used: usize = line_spans.iter().map(|s| s.content.chars().count()).sum();
        let remaining = self.max_width.saturating_sub(used);
        if remaining > 0 {
            line_spans.push(Span::styled(
                " ".repeat(remaining),
                Style::default().bg(Color::DarkGray),
            ));
        }

        self.rows.push(Line::from(line_spans));
    }

    // -- Linked references --

    fn push_linked_refs_header(&mut self, count: usize, collapsed: bool, block_index: usize) {
        let is_selected = self.track_selection(block_index);
        let arrow = if collapsed { "▸" } else { "▾" };
        let label = if count == 0 {
            format!("  {} Linked References (loading...)", arrow)
        } else {
            format!("  {} Linked References ({})", arrow, count)
        };
        let style = if is_selected {
            Style::default()
                .fg(Color::Cyan)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD)
        };
        self.rows
            .push(Line::styled(truncate(&label, self.max_width), style));
    }

    fn push_linked_refs_group(&mut self, page_title: &str, block_index: usize) {
        let is_selected = self.track_selection(block_index);
        let style = if is_selected {
            Style::default().fg(Color::Yellow).bg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Yellow)
        };
        let label = format!("    {}", page_title);
        self.rows
            .push(Line::styled(truncate(&label, self.max_width), style));
    }

    fn push_linked_refs_block(&mut self, text: &str, block_index: usize) {
        let is_selected = self.track_selection(block_index);
        let style = if is_selected {
            Self::selected_style()
        } else {
            Style::default().fg(Color::DarkGray)
        };
        let prefix = "      • ";
        let prefix_width = prefix.chars().count();
        let text_w = self.max_width.saturating_sub(prefix_width);
        let line_spans = markdown::render_spans_with_refs(text, style, Some(self.block_map));
        let wrapped = wrap_spans(line_spans, text_w, text_w);
        for (i, wline) in wrapped.into_iter().enumerate() {
            let pfx = if i == 0 {
                prefix.to_string()
            } else {
                " ".repeat(prefix_width)
            };
            let mut full_spans = vec![Span::styled(pfx, style)];
            full_spans.extend(wline);
            self.rows.push(Line::from(full_spans));
        }
    }
}
