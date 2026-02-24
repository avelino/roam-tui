pub mod header;
pub mod main_area;
pub mod status_bar;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block as WidgetBlock, BorderType, Borders, Clear};
use ratatui::Frame;

use crate::app::{AppState, AutocompleteState, InputMode, SearchState};

use header::Header;
use main_area::{EditInfo, MainArea};
use status_bar::StatusBar;

pub fn render(frame: &mut Frame, state: &AppState) {
    let chunks = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .split(frame.area());

    let header = Header {
        graph_name: &state.graph_name,
        date: &state.date_display,
    };
    frame.render_widget(header, chunks[0]);

    let edit_info = match &state.input_mode {
        InputMode::Insert { buffer, .. } => Some(EditInfo {
            buffer,
            block_index: state.selected_block,
        }),
        InputMode::Normal => None,
    };

    let main = MainArea {
        days: &state.days,
        selected_block: state.selected_block,
        loading: state.loading,
        loading_more: state.loading_more,
        edit_info,
        block_ref_cache: &state.block_ref_cache,
    };
    frame.render_widget(main, chunks[1]);

    if let Some(ac) = &state.autocomplete {
        render_autocomplete_popup(frame, ac, chunks[1]);
    }

    if let Some(search) = &state.search {
        render_search_popup(frame, search, chunks[1]);
    }

    if state.show_help {
        render_help_popup(frame, &state.hints, chunks[1]);
    }

    let insert_mode = !matches!(state.input_mode, InputMode::Normal);
    let status = StatusBar {
        hints: &state.hints,
        message: state.status_message.as_deref(),
        insert_mode,
    };
    frame.render_widget(status, chunks[2]);
}

fn render_autocomplete_popup(frame: &mut Frame, ac: &AutocompleteState, area: Rect) {
    let max_items = if ac.results.is_empty() {
        1 // room for "No results" message
    } else {
        10.min(ac.results.len())
    };
    let popup_height = (max_items + 2) as u16; // +2 for borders
    let popup_width = (area.width * 60 / 100).max(30).min(area.width);
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.y + area.height / 2).min(area.y + area.height.saturating_sub(popup_height));

    let popup_area = Rect::new(x, y, popup_width, popup_height);

    frame.render_widget(Clear, popup_area);

    let title = if ac.query.is_empty() {
        " Block ref ".to_string()
    } else {
        format!(" Search: {} ", ac.query)
    };

    let block = WidgetBlock::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Gray))
        .title(title);

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if ac.results.is_empty() {
        let style = Style::default().fg(Color::DarkGray);
        let line = Line::from(vec![Span::styled("No results", style)]);
        let line_area = Rect::new(inner.x, inner.y, inner.width, 1);
        frame.render_widget(line, line_area);
        return;
    }

    for (i, (_, text)) in ac.results.iter().take(max_items).enumerate() {
        if i as u16 >= inner.height {
            break;
        }
        let is_selected = i == ac.selected;
        let style = if is_selected {
            Style::default().fg(Color::White).bg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Gray)
        };

        let max_text_width = inner.width as usize;
        let display: String = text.chars().take(max_text_width).collect();
        let padding = max_text_width.saturating_sub(display.chars().count());
        let padded = format!("{}{}", display, " ".repeat(padding));

        let line = Line::from(vec![Span::styled(padded, style)]);
        let line_area = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        frame.render_widget(line, line_area);
    }
}

fn render_help_popup(frame: &mut Frame, hints: &[(String, &str)], area: Rect) {
    let line_count = hints.len();
    let popup_height = (line_count + 3).min(area.height as usize) as u16; // +2 borders +1 footer
    let popup_width = (area.width * 60 / 100).max(30).min(area.width);
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + (area.height.saturating_sub(popup_height)) / 2;

    let popup_area = Rect::new(x, y, popup_width, popup_height);
    frame.render_widget(Clear, popup_area);

    let block = WidgetBlock::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" Help ");

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    for (i, (key_str, action_name)) in hints.iter().enumerate() {
        if i as u16 >= inner.height.saturating_sub(1) {
            break;
        }
        let key_span = Span::styled(
            format!("{:>12}", key_str),
            Style::default().fg(Color::Yellow),
        );
        let sep = Span::styled("  ", Style::default());
        let action_span = Span::styled(*action_name, Style::default().fg(Color::White));
        let line = Line::from(vec![key_span, sep, action_span]);
        let line_area = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        frame.render_widget(line, line_area);
    }

    // Footer
    if inner.height > 0 {
        let footer_y = inner.y + inner.height - 1;
        let footer = Line::styled(
            "Press any key to close",
            Style::default().fg(Color::DarkGray),
        );
        let footer_area = Rect::new(inner.x, footer_y, inner.width, 1);
        frame.render_widget(footer, footer_area);
    }
}

fn render_search_popup(frame: &mut Frame, search: &SearchState, area: Rect) {
    let max_visible = 10;
    let visible_count = if search.results.is_empty() {
        1
    } else {
        max_visible.min(search.results.len())
    };
    let popup_height = (visible_count + 2) as u16;
    let popup_width = (area.width * 80 / 100).max(30).min(area.width);
    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = (area.y + area.height / 2).min(area.y + area.height.saturating_sub(popup_height));

    let popup_area = Rect::new(x, y, popup_width, popup_height);
    frame.render_widget(Clear, popup_area);

    let title = if search.query.is_empty() {
        " Search ".to_string()
    } else {
        format!(" Search: {} ", search.query)
    };

    let block = WidgetBlock::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Yellow))
        .title(title);

    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    if search.results.is_empty() {
        let style = Style::default().fg(Color::DarkGray);
        let line = Line::from(vec![Span::styled("No results", style)]);
        let line_area = Rect::new(inner.x, inner.y, inner.width, 1);
        frame.render_widget(line, line_area);
        return;
    }

    // Scroll window: keep selected item visible
    let scroll_offset = if search.selected >= max_visible {
        search.selected - max_visible + 1
    } else {
        0
    };

    for (i, (_, text)) in search
        .results
        .iter()
        .skip(scroll_offset)
        .take(visible_count)
        .enumerate()
    {
        if i as u16 >= inner.height {
            break;
        }
        let is_selected = (i + scroll_offset) == search.selected;
        let style = if is_selected {
            Style::default().fg(Color::White).bg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Gray)
        };

        let max_text_width = inner.width as usize;
        let display: String = text.chars().take(max_text_width).collect();
        let padding = max_text_width.saturating_sub(display.chars().count());
        let padded = format!("{}{}", display, " ".repeat(padding));

        let line = Line::from(vec![Span::styled(padded, style)]);
        let line_area = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        frame.render_widget(line, line_area);
    }
}
