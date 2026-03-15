use super::*;
use crate::api::types::{Block, LinkedRefBlock, LinkedRefGroup};
use crate::app::LinkedRefsState;
use crate::ui::text_wrap::{inject_cursor, wrap_spans};
use crate::ui::visible_lines::{build_visible_lines, VisibleLine};
use chrono::NaiveDate;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
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
        refs: vec![],
    }
}

fn make_daily_note(title: &str, year: i32, month: u32, day: u32, blocks: Vec<Block>) -> DailyNote {
    DailyNote {
        date: NaiveDate::from_ymd_opt(year, month, day).unwrap(),
        uid: format!("{:02}-{:02}-{}", month, day, year),
        title: title.into(),
        blocks,
    }
}

fn render_widget(days: &[DailyNote], selection: usize, width: u16, height: u16) -> Buffer {
    render_widget_with_refs(days, selection, width, height, &HashMap::new())
}

fn render_widget_with_refs(
    days: &[DailyNote],
    selection: usize,
    width: u16,
    height: u16,
    linked_refs: &HashMap<String, LinkedRefsState>,
) -> Buffer {
    let area = Rect::new(0, 0, width, height);
    let mut buf = Buffer::empty(area);
    let widget = MainArea {
        days,
        selection: &Selection::Single(selection),
        cursor_col: 0,
        loading: false,
        loading_more: false,
        edit_info: None,
        block_ref_cache: &HashMap::new(),
        linked_refs,
    };
    widget.render(area, &mut buf);
    buf
}

// -- visible_lines tests --

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
    let lines = build_visible_lines(&[day], false, &mut hl, &HashMap::new());

    assert_eq!(lines.len(), 3);
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
        refs: vec![],
    };
    let day = make_daily_note("Feb 21", 2026, 2, 21, vec![parent]);
    let mut hl = CodeHighlighter::new();
    let lines = build_visible_lines(&[day], false, &mut hl, &HashMap::new());

    assert_eq!(lines.len(), 3);
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
    let lines = build_visible_lines(&[day1, day2], false, &mut hl, &HashMap::new());

    assert_eq!(lines.len(), 5);
    assert!(matches!(&lines[2], VisibleLine::DaySeparator));
    assert!(matches!(&lines[3], VisibleLine::DayHeading(t) if t == "Day 2"));
}

#[test]
fn build_visible_lines_loading_more() {
    let day = make_daily_note("Day 1", 2026, 2, 21, vec![make_block("a", "A", 0)]);
    let mut hl = CodeHighlighter::new();
    let lines = build_visible_lines(&[day], true, &mut hl, &HashMap::new());
    assert!(matches!(lines.last().unwrap(), VisibleLine::LoadingMore));
}

// -- Widget rendering tests --

#[test]
fn renders_loading_state() {
    let area = Rect::new(0, 0, 40, 5);
    let mut buf = Buffer::empty(area);
    let widget = MainArea {
        days: &[],
        selection: &Selection::Single(0),
        cursor_col: 0,
        loading: true,
        loading_more: false,
        edit_info: None,
        block_ref_cache: &HashMap::new(),
        linked_refs: &HashMap::new(),
    };
    widget.render(area, &mut buf);
    assert!((0..area.height).any(|y| read_line(&buf, y, area.width).contains("Loading")));
}

#[test]
fn renders_empty_state() {
    let area = Rect::new(0, 0, 40, 5);
    let mut buf = Buffer::empty(area);
    let widget = MainArea {
        days: &[],
        selection: &Selection::Single(0),
        cursor_col: 0,
        loading: false,
        loading_more: false,
        edit_info: None,
        block_ref_cache: &HashMap::new(),
        linked_refs: &HashMap::new(),
    };
    widget.render(area, &mut buf);
    assert!((0..area.height).any(|y| read_line(&buf, y, area.width).contains("No notes")));
}

#[test]
fn renders_day_heading_and_blocks() {
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
    let buf = render_widget(&[day], 0, 40, 10);
    assert!(read_line(&buf, 0, 40).contains("February 21, 2026"));
    assert!(read_line(&buf, 1, 40).contains("Block one"));
}

#[test]
fn renders_selected_block_with_bullet() {
    let day = make_daily_note(
        "Feb 21",
        2026,
        2,
        21,
        vec![make_block("b1", "First", 0), make_block("b2", "Second", 1)],
    );
    let buf = render_widget(&[day], 1, 40, 10);
    assert!(read_line(&buf, 2, 40).contains("Second"));
}

#[test]
fn renders_nested_block_with_indentation() {
    let parent = Block {
        uid: "p".into(),
        string: "Parent".into(),
        order: 0,
        children: vec![make_block("c1", "Child", 0)],
        open: true,
        refs: vec![],
    };
    let day = make_daily_note("Feb 21", 2026, 2, 21, vec![parent]);
    let buf = render_widget(&[day], 0, 40, 10);

    let line1 = read_line(&buf, 1, 40);
    let line2 = read_line(&buf, 2, 40);
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
    let lines = build_visible_lines(&[day], false, &mut hl, &HashMap::new());

    let code_line_count = lines
        .iter()
        .filter(|l| matches!(l, VisibleLine::CodeLine { .. }))
        .count();
    assert!(code_line_count >= 2);

    let has_after = lines
        .iter()
        .any(|l| matches!(l, VisibleLine::Block { text, .. } if text == "After"));
    assert!(has_after);
}

#[test]
fn code_lines_have_line_numbers() {
    let code_text = "```\nrust\nfn main() {}\nlet x = 1;```";
    let day = make_daily_note("Feb 21", 2026, 2, 21, vec![make_block("c1", code_text, 0)]);
    let mut hl = CodeHighlighter::new();
    let lines = build_visible_lines(&[day], false, &mut hl, &HashMap::new());
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
    let lines = build_visible_lines(&[day], false, &mut hl, &HashMap::new());
    assert!(lines
        .iter()
        .any(|l| matches!(l, VisibleLine::CodeLabel { .. })));
}

#[test]
fn code_line_number_rendered_in_buffer() {
    let code_text = "```\nrust\nfn main() {}\nlet x = 1;```";
    let day = make_daily_note("Feb 21", 2026, 2, 21, vec![make_block("c1", code_text, 0)]);
    let buf = render_widget(&[day], 0, 60, 10);

    let line2 = read_line(&buf, 2, 60);
    let line3 = read_line(&buf, 3, 60);
    assert!(line2.contains('1'));
    assert!(line2.contains("fn"));
    assert!(line3.contains('2'));
}

#[test]
fn renders_bold_text_with_styling() {
    let day = make_daily_note(
        "Feb 21",
        2026,
        2,
        21,
        vec![make_block("b1", "**bold text**", 0)],
    );
    let buf = render_widget(&[day], 0, 60, 10);
    let line1 = read_line(&buf, 1, 60);
    assert!(line1.contains("bold text"));
    assert!(!line1.contains("**"));
}

#[test]
fn renders_page_link_without_brackets() {
    let day = make_daily_note(
        "Feb 21",
        2026,
        2,
        21,
        vec![make_block("b1", "See [[my page]]", 0)],
    );
    let buf = render_widget(&[day], 0, 60, 10);
    assert!(read_line(&buf, 1, 60).contains("my page"));
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
    let lines = build_visible_lines(&[day], false, &mut hl, &HashMap::new());
    assert!(
        matches!(&lines[1], VisibleLine::Blockquote { text, block_index: 0, .. } if text == "quoted text")
    );
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
    let lines = build_visible_lines(&[day], false, &mut hl, &HashMap::new());
    if let VisibleLine::Blockquote { text, .. } = &lines[1] {
        assert!(!text.starts_with("> "));
        assert_eq!(text, "hello world");
    } else {
        panic!("Expected Blockquote variant");
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
        refs: vec![],
    };
    let day = make_daily_note("Feb 21", 2026, 2, 21, vec![parent]);
    let mut hl = CodeHighlighter::new();
    let lines = build_visible_lines(&[day], false, &mut hl, &HashMap::new());
    assert!(matches!(
        &lines[2],
        VisibleLine::Blockquote { depth: 1, text, block_index: 1, .. } if text == "nested quote"
    ));
}

#[test]
fn blockquote_renders_with_border() {
    let day = make_daily_note(
        "Feb 21",
        2026,
        2,
        21,
        vec![make_block("b1", "> quoted text", 0)],
    );
    let buf = render_widget(&[day], 0, 60, 10);
    let line1 = read_line(&buf, 1, 60);
    assert!(line1.contains('│'));
    assert!(line1.contains("quoted text"));
}

#[test]
fn blockquote_with_inline_formatting() {
    let day = make_daily_note(
        "Feb 21",
        2026,
        2,
        21,
        vec![make_block("b1", "> **bold** and [[link]]", 0)],
    );
    let buf = render_widget(&[day], 0, 60, 10);
    let line1 = read_line(&buf, 1, 60);
    assert!(line1.contains("bold"));
    assert!(line1.contains("link"));
    assert!(!line1.contains("**"));
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
    let lines = build_visible_lines(&[day], false, &mut hl, &HashMap::new());
    assert!(matches!(&lines[1], VisibleLine::Block { text, .. } if text == "normal text"));
    assert!(matches!(&lines[2], VisibleLine::Blockquote { text, .. } if text == "quoted"));
    assert!(matches!(&lines[3], VisibleLine::Block { text, .. } if text == "also normal"));
}

#[test]
fn renders_todo_as_checkbox() {
    let day = make_daily_note(
        "Feb 21",
        2026,
        2,
        21,
        vec![make_block("b1", "{{TODO}} buy milk", 0)],
    );
    let buf = render_widget(&[day], 0, 60, 10);
    assert!(read_line(&buf, 1, 60).contains("buy milk"));
}

// -- wrap_spans tests --

fn collect_line_text(spans: &[Span]) -> String {
    spans.iter().map(|s| s.content.as_ref()).collect()
}

#[test]
fn wrap_spans_short_text_no_wrap() {
    let spans = vec![Span::styled("hello", Style::default().fg(Color::White))];
    let result = wrap_spans(spans, 20, 20);
    assert_eq!(result.len(), 1);
    assert_eq!(collect_line_text(&result[0]), "hello");
}

#[test]
fn wrap_spans_long_text_wraps_at_word() {
    let spans = vec![Span::styled(
        "hello world foo",
        Style::default().fg(Color::White),
    )];
    let result = wrap_spans(spans, 10, 10);
    assert_eq!(result.len(), 2);
    assert_eq!(collect_line_text(&result[0]), "hello ");
    assert_eq!(collect_line_text(&result[1]), "world foo");
}

#[test]
fn wrap_spans_no_space_hard_wraps() {
    let spans = vec![Span::styled(
        "abcdefghijklmno",
        Style::default().fg(Color::White),
    )];
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
    let result = wrap_spans(spans, 10, 10);
    assert_eq!(result.len(), 2);
    assert_eq!(result[0][0].content, "aaaa");
    assert!(result[0][0].style.add_modifier.contains(Modifier::BOLD));
    assert_eq!(result[0][1].content, " bbbb ");
    assert_eq!(collect_line_text(&result[1]), "cccc");
}

#[test]
fn wrap_spans_different_first_cont_widths() {
    let spans = vec![Span::styled("aaa bbb ccc ddd eee", Style::default())];
    let result = wrap_spans(spans, 15, 8);
    assert_eq!(result.len(), 2);
    assert!(collect_line_text(&result[0]).len() <= 15);
    assert!(collect_line_text(&result[1]).len() <= 8);
}

#[test]
fn wrap_spans_empty_returns_single_empty_line() {
    let result = wrap_spans(vec![], 10, 10);
    assert_eq!(result.len(), 1);
    assert!(result[0].is_empty());
}

#[test]
fn multiline_block_respects_newlines() {
    let text = "First paragraph\n\nSecond paragraph";
    let day = make_daily_note("Feb 21", 2026, 2, 21, vec![make_block("b1", text, 0)]);
    let buf = render_widget(&[day], 0, 60, 10);

    let line1 = read_line(&buf, 1, 60);
    let line2 = read_line(&buf, 2, 60);
    let line3 = read_line(&buf, 3, 60);
    assert!(line1.contains("First paragraph"));
    assert!(line1.contains('•'));
    assert!(!line2.contains("Second"));
    assert!(line3.contains("Second paragraph"));
    assert!(!line3.contains('•'));
}

#[test]
fn long_block_wraps_in_visible_output() {
    let long_text = "this is a very long block text that should wrap";
    let day = make_daily_note("Feb 21", 2026, 2, 21, vec![make_block("b1", long_text, 0)]);
    let buf = render_widget(&[day], 0, 30, 10);

    let line1 = read_line(&buf, 1, 30);
    let line2 = read_line(&buf, 2, 30);
    assert!(line1.contains("this"));
    assert!(!line2.trim().is_empty());
    assert!(!line2.contains('•'));
}

#[test]
fn test_collapsed_block_shows_arrow_indicator() {
    let parent = Block {
        uid: "p".into(),
        string: "Parent".into(),
        order: 0,
        children: vec![make_block("c1", "Child", 0)],
        open: false,
        refs: vec![],
    };
    let day = make_daily_note("Feb 21", 2026, 2, 21, vec![parent]);
    let mut hl = CodeHighlighter::new();
    let lines = build_visible_lines(&[day], false, &mut hl, &HashMap::new());
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
        refs: vec![],
    };
    let day = make_daily_note("Feb 21", 2026, 2, 21, vec![parent]);
    let buf = render_widget(&[day], 0, 60, 10);
    let line1 = read_line(&buf, 1, 60);
    assert!(line1.contains("▸"));
    assert!(line1.contains("[3]"));
}

#[test]
fn test_open_block_shows_bullet() {
    let day = make_daily_note(
        "Feb 21",
        2026,
        2,
        21,
        vec![make_block("b1", "Normal block", 0)],
    );
    let buf = render_widget(&[day], 0, 60, 10);
    let line1 = read_line(&buf, 1, 60);
    assert!(line1.contains('•'));
    assert!(!line1.contains('▸'));
}

#[test]
fn test_day_separator_rendered() {
    let day1 = make_daily_note("Day 1", 2026, 2, 21, vec![make_block("a", "A", 0)]);
    let day2 = make_daily_note("Day 2", 2026, 2, 20, vec![make_block("b", "B", 0)]);
    let buf = render_widget(&[day1, day2], 0, 40, 10);
    assert!(read_line(&buf, 2, 40).contains('─'));
}

#[test]
fn test_selected_block_has_indicator() {
    let day = make_daily_note(
        "Feb 21",
        2026,
        2,
        21,
        vec![make_block("b1", "First", 0), make_block("b2", "Second", 1)],
    );
    let buf = render_widget(&[day], 0, 60, 10);
    assert!(read_line(&buf, 1, 60).contains('▎'));
    assert!(!read_line(&buf, 2, 60).contains('▎'));
}

#[test]
fn day_with_blocks_renders_heading_not_no_notes_message() {
    let day = make_daily_note(
        "February 25th, 2026",
        2026,
        2,
        25,
        vec![make_block("b1", "Some block", 0)],
    );
    let buf = render_widget(&[day], 0, 40, 5);
    assert!(!(0..5u16).any(|y| read_line(&buf, y, 40).contains("No notes")));
    assert!((0..5u16).any(|y| read_line(&buf, y, 40).contains("February 25th, 2026")));
}

#[test]
fn day_with_empty_blocks_vec_renders_heading_not_no_notes() {
    let day = make_daily_note("February 25th, 2026", 2026, 2, 25, vec![]);
    let buf = render_widget(&[day], 0, 40, 5);
    assert!(!(0..5u16).any(|y| read_line(&buf, y, 40).contains("No notes")));
    assert!((0..5u16).any(|y| read_line(&buf, y, 40).contains("February 25th, 2026")));
}

// -- inject_cursor tests --

#[test]
fn inject_cursor_at_zero_inverts_first_char() {
    let spans = vec![Span::styled("hello", Style::default().fg(Color::Gray))];
    let result = inject_cursor(spans, 0);
    let cursor_style = Style::default().fg(Color::Black).bg(Color::White);
    assert_eq!(result[0].content.chars().next().unwrap(), 'h');
    assert_eq!(result[0].style, cursor_style);
}

#[test]
fn inject_cursor_at_mid_inverts_correct_char() {
    let spans = vec![Span::styled("hello", Style::default().fg(Color::Gray))];
    let result = inject_cursor(spans, 2);
    let all_chars: String = result.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(all_chars, "hello");
    let cursor_style = Style::default().fg(Color::Black).bg(Color::White);
    let cursor_span = result.iter().find(|s| s.style == cursor_style).unwrap();
    assert_eq!(cursor_span.content.as_ref(), "l");
}

#[test]
fn inject_cursor_on_empty_spans_returns_space_with_cursor_style() {
    let result = inject_cursor(vec![], 0);
    let cursor_style = Style::default().fg(Color::Black).bg(Color::White);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].content.as_ref(), " ");
    assert_eq!(result[0].style, cursor_style);
}

#[test]
fn inject_cursor_out_of_bounds_clamps_to_last_char() {
    let spans = vec![Span::styled("abc", Style::default().fg(Color::Gray))];
    let result = inject_cursor(spans, 99);
    let cursor_style = Style::default().fg(Color::Black).bg(Color::White);
    let cursor_span = result.iter().find(|s| s.style == cursor_style).unwrap();
    assert_eq!(cursor_span.content.as_ref(), "c");
}

#[test]
fn inject_cursor_preserves_all_chars() {
    let spans = vec![Span::styled("world", Style::default().fg(Color::Gray))];
    let result = inject_cursor(spans, 3);
    let all_chars: String = result.iter().map(|s| s.content.as_ref()).collect();
    assert_eq!(all_chars, "world");
}

// -- Linked References tests --

fn make_linked_refs() -> LinkedRefsState {
    LinkedRefsState {
        groups: vec![LinkedRefGroup {
            page_title: "Source Page".into(),
            blocks: vec![LinkedRefBlock {
                uid: "lr1".into(),
                string: "mentions [[Target]]".into(),
                page_title: "Source Page".into(),
            }],
        }],
        collapsed: false,
        loading: false,
    }
}

#[test]
fn build_visible_lines_includes_linked_refs() {
    let day = make_daily_note("Feb 25", 2026, 2, 25, vec![make_block("b1", "Block", 0)]);
    let lr_map = HashMap::from([("Feb 25".to_string(), make_linked_refs())]);
    let mut hl = CodeHighlighter::new();
    let lines = build_visible_lines(&[day], false, &mut hl, &lr_map);

    assert_eq!(lines.len(), 6);
    assert!(matches!(&lines[2], VisibleLine::LinkedRefsSeparator));
    assert!(matches!(
        &lines[3],
        VisibleLine::LinkedRefsHeader {
            count: 1,
            collapsed: false,
            ..
        }
    ));
    assert!(
        matches!(&lines[4], VisibleLine::LinkedRefsGroupHeader { ref page_title, .. } if page_title == "Source Page")
    );
    assert!(
        matches!(&lines[5], VisibleLine::LinkedRefsBlock { ref text, .. } if text.contains("Target"))
    );
}

#[test]
fn build_visible_lines_collapsed_linked_refs() {
    let day = make_daily_note("Feb 25", 2026, 2, 25, vec![make_block("b1", "Block", 0)]);
    let lr = LinkedRefsState {
        groups: vec![LinkedRefGroup {
            page_title: "P".into(),
            blocks: vec![LinkedRefBlock {
                uid: "x".into(),
                string: "ref".into(),
                page_title: "P".into(),
            }],
        }],
        collapsed: true,
        loading: false,
    };
    let lr_map = HashMap::from([("Feb 25".to_string(), lr)]);
    let mut hl = CodeHighlighter::new();
    let lines = build_visible_lines(&[day], false, &mut hl, &lr_map);

    assert_eq!(lines.len(), 4);
    assert!(matches!(
        &lines[3],
        VisibleLine::LinkedRefsHeader {
            collapsed: true,
            ..
        }
    ));
}

#[test]
fn build_visible_lines_empty_linked_refs_not_shown() {
    let day = make_daily_note("Feb 25", 2026, 2, 25, vec![make_block("b1", "Block", 0)]);
    let lr = LinkedRefsState {
        groups: vec![],
        collapsed: false,
        loading: false,
    };
    let lr_map = HashMap::from([("Feb 25".to_string(), lr)]);
    let mut hl = CodeHighlighter::new();
    let lines = build_visible_lines(&[day], false, &mut hl, &lr_map);
    assert_eq!(lines.len(), 2);
}

#[test]
fn renders_linked_refs_header() {
    let day = make_daily_note("Feb 25", 2026, 2, 25, vec![make_block("b1", "Block", 0)]);
    let lr_map = HashMap::from([("Feb 25".to_string(), make_linked_refs())]);
    let buf = render_widget_with_refs(&[day], 0, 60, 15, &lr_map);

    assert!((0..15u16).any(|y| read_line(&buf, y, 60).contains("Linked References")));
    assert!((0..15u16).any(|y| read_line(&buf, y, 60).contains("Source Page")));
}

#[test]
fn linked_refs_block_index_starts_after_flat_blocks() {
    let day = make_daily_note(
        "Feb 25",
        2026,
        2,
        25,
        vec![make_block("b1", "A", 0), make_block("b2", "B", 1)],
    );
    let lr_map = HashMap::from([("Feb 25".to_string(), make_linked_refs())]);
    let mut hl = CodeHighlighter::new();
    let lines = build_visible_lines(&[day], false, &mut hl, &lr_map);

    if let VisibleLine::LinkedRefsHeader { block_index, .. } = &lines[4] {
        assert_eq!(*block_index, 2);
    } else {
        panic!("Expected LinkedRefsHeader at index 4");
    }
    if let VisibleLine::LinkedRefsGroupHeader { block_index, .. } = &lines[5] {
        assert_eq!(*block_index, 3);
    } else {
        panic!("Expected LinkedRefsGroupHeader at index 5");
    }
}
