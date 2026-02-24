use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

pub struct StatusBar<'a> {
    pub hints: &'a [(String, &'static str)],
    pub message: Option<&'a str>,
    pub insert_mode: bool,
}

impl<'a> Widget for StatusBar<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        if self.insert_mode {
            let line = Line::from(Span::styled(
                " -- INSERT -- (ESC para salvar) ",
                Style::default().fg(Color::Green),
            ));
            line.render(area, buf);
            return;
        }

        if let Some(msg) = self.message {
            let line = Line::from(Span::styled(
                format!(" {} ", msg),
                Style::default().fg(Color::Yellow),
            ));
            line.render(area, buf);
            return;
        }

        let mut spans = Vec::new();
        spans.push(Span::raw(" "));

        for (i, (key, action)) in self.hints.iter().enumerate() {
            if i > 0 {
                spans.push(Span::styled("  ", Style::default().fg(Color::DarkGray)));
            }
            spans.push(Span::styled(
                format!("[{}]", key),
                Style::default().fg(Color::Cyan),
            ));
            spans.push(Span::styled(
                action.to_string(),
                Style::default()
                    .fg(Color::DarkGray)
                    .add_modifier(Modifier::DIM),
            ));
        }

        let line = Line::from(spans);
        line.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_bar_renders_hints() {
        let area = Rect::new(0, 0, 60, 1);
        let mut buf = Buffer::empty(area);

        let hints = vec![("q".to_string(), "quit"), ("/".to_string(), "search")];
        let bar = StatusBar {
            hints: &hints,
            message: None,
            insert_mode: false,
        };
        bar.render(area, &mut buf);

        let content: String = (0..area.width)
            .map(|x| {
                buf.cell((x, 0))
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();

        assert!(content.contains("[q]"));
        assert!(content.contains("quit"));
        assert!(content.contains("[/]"));
        assert!(content.contains("search"));
    }

    #[test]
    fn status_bar_renders_message_when_present() {
        let area = Rect::new(0, 0, 60, 1);
        let mut buf = Buffer::empty(area);

        let hints = vec![("q".to_string(), "quit")];
        let bar = StatusBar {
            hints: &hints,
            message: Some("Loading pages..."),
            insert_mode: false,
        };
        bar.render(area, &mut buf);

        let content: String = (0..area.width)
            .map(|x| {
                buf.cell((x, 0))
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();

        assert!(content.contains("Loading pages..."));
        assert!(!content.contains("[q]"));
    }

    #[test]
    fn status_bar_shows_insert_mode() {
        let area = Rect::new(0, 0, 60, 1);
        let mut buf = Buffer::empty(area);

        let bar = StatusBar {
            hints: &[],
            message: None,
            insert_mode: true,
        };
        bar.render(area, &mut buf);

        let content: String = (0..area.width)
            .map(|x| {
                buf.cell((x, 0))
                    .unwrap()
                    .symbol()
                    .chars()
                    .next()
                    .unwrap_or(' ')
            })
            .collect();

        assert!(content.contains("INSERT"));
    }
}
