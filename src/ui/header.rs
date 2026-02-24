use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Widget;

pub struct Header<'a> {
    pub graph_name: &'a str,
    pub date: &'a str,
}

impl<'a> Widget for Header<'a> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let title = Span::styled(
            " roam-tui ",
            Style::default()
                .fg(Color::White)
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

        let graph = Span::styled(
            format!(" [{}] ", self.graph_name),
            Style::default().fg(Color::Cyan).bg(Color::DarkGray),
        );

        let spacer_len = area.width.saturating_sub(
            title.width() as u16 + graph.width() as u16 + self.date.len() as u16 + 1,
        );
        let bg = Style::default().bg(Color::DarkGray);
        let spacer = Span::styled(" ".repeat(spacer_len as usize), bg);

        let date = Span::styled(
            format!("{} ", self.date),
            Style::default().fg(Color::Gray).bg(Color::DarkGray),
        );

        let line = Line::from(vec![title, graph, spacer, date]);
        line.render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_renders_graph_name_and_date() {
        let area = Rect::new(0, 0, 60, 1);
        let mut buf = Buffer::empty(area);

        let header = Header {
            graph_name: "my-graph",
            date: "Feb 21, 2026",
        };
        header.render(area, &mut buf);

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

        assert!(content.contains("roam-tui"));
        assert!(content.contains("my-graph"));
        assert!(content.contains("Feb 21, 2026"));
    }

    #[test]
    fn header_renders_in_single_line() {
        let area = Rect::new(0, 0, 80, 1);
        let mut buf = Buffer::empty(area);

        let header = Header {
            graph_name: "test",
            date: "Jan 01, 2026",
        };
        header.render(area, &mut buf);

        // Should not panic and fill the single line
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
        assert!(content.contains("roam-tui"));
    }
}
