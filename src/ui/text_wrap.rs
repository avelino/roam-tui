use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

/// Inject a block cursor (inverted color) at the given character position within spans.
pub(crate) fn inject_cursor(spans: Vec<Span<'static>>, cursor_pos: usize) -> Vec<Span<'static>> {
    let cursor_style = Style::default().fg(Color::Black).bg(Color::White);
    let chars: Vec<(char, Style)> = spans
        .iter()
        .flat_map(|s| s.content.chars().map(move |c| (c, s.style)))
        .collect();

    if chars.is_empty() {
        return vec![Span::styled(" ", cursor_style)];
    }

    let pos = cursor_pos.min(chars.len().saturating_sub(1));
    let mut modified = chars;
    modified[pos].1 = cursor_style;
    chars_to_spans(&modified)
}

pub(crate) fn render_centered_message(msg: &str, area: Rect, buf: &mut Buffer) {
    if area.height > 0 {
        let line = Line::styled(msg, Style::default().fg(Color::DarkGray));
        let y = area.y + area.height / 2;
        let render_area = Rect::new(area.x, y, area.width, 1);
        line.render(render_area, buf);
    }
}

/// Reconstruct `Vec<Span<'static>>` from a slice of (char, Style) pairs,
/// merging consecutive chars that share the same style into a single Span.
pub(crate) fn chars_to_spans(chars: &[(char, Style)]) -> Vec<Span<'static>> {
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
pub(crate) fn wrap_spans(
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

pub(crate) fn truncate(s: &str, max_len: usize) -> String {
    s.chars().take(max_len).collect()
}
