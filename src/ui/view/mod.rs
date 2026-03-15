use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use crate::api::types::DailyNote;
use crate::app::{LinkedRefsState, Selection};
use crate::edit_buffer::EditBuffer;
use crate::highlight::CodeHighlighter;
use crate::markdown;

use super::row_builder::RowBuilder;
use super::text_wrap::render_centered_message;
use super::visible_lines::build_visible_lines;

pub struct MainArea<'a> {
    pub days: &'a [DailyNote],
    pub selection: &'a Selection,
    pub cursor_col: usize,
    pub loading: bool,
    pub loading_more: bool,
    pub edit_info: Option<EditInfo<'a>>,
    pub block_ref_cache: &'a std::collections::HashMap<String, String>,
    pub linked_refs: &'a std::collections::HashMap<String, LinkedRefsState>,
}

pub struct EditInfo<'a> {
    pub buffer: &'a EditBuffer,
    pub block_index: usize,
}

impl<'a> Widget for MainArea<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.loading {
            render_centered_message(" Loading today's notes...", area, buf);
            return;
        }

        if self.days.is_empty() {
            render_centered_message(" No notes for today", area, buf);
            return;
        }

        // Build data
        let mut highlighter = CodeHighlighter::new();
        let mut block_map = markdown::build_block_text_map(self.days);
        block_map.extend(
            self.block_ref_cache
                .iter()
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        let visible_lines = build_visible_lines(
            self.days,
            self.loading_more,
            &mut highlighter,
            self.linked_refs,
        );

        // Build rows
        let max_width = area.width as usize;
        let builder = RowBuilder::new(
            self.selection,
            self.edit_info.as_ref(),
            self.cursor_col,
            &block_map,
            max_width,
        );
        let (rows, selected_row) = builder.build(&visible_lines);

        // Scroll (half-page centering on selected row)
        let viewport_height = area.height as usize;
        let half = viewport_height / 2;
        let scroll_offset = if selected_row > half {
            (selected_row - half).min(rows.len().saturating_sub(viewport_height))
        } else {
            0
        };

        // Render
        for (i, row) in rows.into_iter().skip(scroll_offset).enumerate() {
            if i >= viewport_height {
                break;
            }
            let y = area.y + i as u16;
            let render_area = Rect::new(area.x, y, area.width, 1);
            Widget::render(row, render_area, buf);
        }
    }
}

#[cfg(test)]
mod tests;
