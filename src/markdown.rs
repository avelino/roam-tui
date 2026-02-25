use std::collections::{HashMap, HashSet};

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;

/// Count the number of rendered characters after markdown processing.
///
/// This computes how many visible characters a line of text produces
/// after stripping markdown delimiters (**bold**, [[links]], etc.).
pub fn rendered_char_count(text: &str) -> usize {
    let base = Style::default();
    render_spans(text, base)
        .iter()
        .map(|s| s.content.chars().count())
        .sum()
}

/// Parse Roam-flavored inline markdown into styled ratatui Spans.
///
/// Supports: **bold**, __italic__, [[page links]], ((block refs)), #tags,
/// `inline code`, ~~strikethrough~~, ^^highlight^^, {{TODO}}, {{DONE}},
/// {{embed: ((uid))}}, {{[[embed]]: ((uid))}}.
///
/// When `block_map` is provided, ((block-uid)) references are resolved
/// to show the referenced block's text instead of the raw UID.
pub fn render_spans(text: &str, base_style: Style) -> Vec<Span<'static>> {
    render_spans_with_refs(text, base_style, None)
}

pub fn render_spans_with_refs(
    text: &str,
    base_style: Style,
    block_map: Option<&HashMap<String, String>>,
) -> Vec<Span<'static>> {
    if text.is_empty() {
        return vec![];
    }

    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut spans: Vec<Span<'static>> = Vec::new();
    let mut plain = String::new();
    let mut i = 0;

    while i < len {
        // {{TODO}} / {{DONE}} / {{[[TODO]]}} / {{[[DONE]]}} / {{embed: ...}} / {{[[embed]]: ...}}
        if chars[i] == '{' && i + 1 < len && chars[i + 1] == '{' {
            if let Some(end) = find_closing_double_brace(&chars, i + 2) {
                let inner: String = chars[i + 2..end].iter().collect();
                let normalized = inner
                    .strip_prefix("[[")
                    .and_then(|s| s.strip_suffix("]]"))
                    .unwrap_or(&inner);
                if normalized == "TODO" || normalized == "DONE" {
                    flush_plain(&mut plain, base_style, &mut spans);
                    if normalized == "TODO" {
                        spans.push(Span::styled("☐ ".to_string(), base_style.fg(Color::Red)));
                    } else {
                        spans.push(Span::styled("✓ ".to_string(), base_style.fg(Color::Green)));
                    }
                    i = end + 2; // skip past }}
                    continue;
                }
                // {{embed: ((uid))}} / {{[[embed]]: ((uid))}} / {{embed: [[page]]}}
                if let Some(ref_text) = parse_embed_inner(&inner) {
                    flush_plain(&mut plain, base_style, &mut spans);
                    let resolved = resolve_ref(&ref_text, block_map);
                    spans.push(Span::styled(
                        format!("▸ {}", resolved),
                        base_style.fg(Color::Magenta).add_modifier(Modifier::ITALIC),
                    ));
                    i = end + 2;
                    continue;
                }
            }
        }

        // `inline code`
        if chars[i] == '`' {
            if let Some(end) = find_single_delimiter(&chars, i + 1, '`') {
                flush_plain(&mut plain, base_style, &mut spans);
                let content: String = chars[i + 1..end].iter().collect();
                spans.push(Span::styled(
                    content,
                    base_style.fg(Color::Green).bg(Color::DarkGray),
                ));
                i = end + 1;
                continue;
            }
        }

        // **bold**
        if chars[i] == '*' && i + 1 < len && chars[i + 1] == '*' {
            if let Some(end) = find_double_delimiter(&chars, i + 2, '*') {
                flush_plain(&mut plain, base_style, &mut spans);
                let content: String = chars[i + 2..end].iter().collect();
                spans.push(Span::styled(
                    content,
                    base_style.fg(Color::White).add_modifier(Modifier::BOLD),
                ));
                i = end + 2;
                continue;
            }
        }

        // __italic__
        if chars[i] == '_' && i + 1 < len && chars[i + 1] == '_' {
            if let Some(end) = find_double_delimiter(&chars, i + 2, '_') {
                flush_plain(&mut plain, base_style, &mut spans);
                let content: String = chars[i + 2..end].iter().collect();
                spans.push(Span::styled(
                    content,
                    base_style.add_modifier(Modifier::ITALIC),
                ));
                i = end + 2;
                continue;
            }
        }

        // ^^highlight^^
        if chars[i] == '^' && i + 1 < len && chars[i + 1] == '^' {
            if let Some(end) = find_double_delimiter(&chars, i + 2, '^') {
                flush_plain(&mut plain, base_style, &mut spans);
                let content: String = chars[i + 2..end].iter().collect();
                spans.push(Span::styled(content, base_style.bg(Color::Yellow)));
                i = end + 2;
                continue;
            }
        }

        // ~~strikethrough~~
        if chars[i] == '~' && i + 1 < len && chars[i + 1] == '~' {
            if let Some(end) = find_double_delimiter(&chars, i + 2, '~') {
                flush_plain(&mut plain, base_style, &mut spans);
                let content: String = chars[i + 2..end].iter().collect();
                spans.push(Span::styled(
                    content,
                    base_style
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::CROSSED_OUT),
                ));
                i = end + 2;
                continue;
            }
        }

        // [[page link]]
        if chars[i] == '[' && i + 1 < len && chars[i + 1] == '[' {
            if let Some(end) = find_double_delimiter(&chars, i + 2, ']') {
                flush_plain(&mut plain, base_style, &mut spans);
                let content: String = chars[i + 2..end].iter().collect();
                spans.push(Span::styled(content, base_style.fg(Color::Cyan)));
                i = end + 2;
                continue;
            }
        }

        // [text](url) markdown link
        if chars[i] == '[' && (i + 1 >= len || chars[i + 1] != '[') {
            if let Some((text, link_end)) = parse_markdown_link(&chars, i) {
                flush_plain(&mut plain, base_style, &mut spans);
                spans.push(Span::styled(
                    text,
                    base_style
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::UNDERLINED),
                ));
                i = link_end;
                continue;
            }
        }

        // ((block ref))
        if chars[i] == '(' && i + 1 < len && chars[i + 1] == '(' {
            if let Some(end) = find_double_delimiter(&chars, i + 2, ')') {
                flush_plain(&mut plain, base_style, &mut spans);
                let uid: String = chars[i + 2..end].iter().collect();
                let display = resolve_ref(&RefText::BlockUid(uid), block_map);
                spans.push(Span::styled(display, base_style.fg(Color::Magenta)));
                i = end + 2;
                continue;
            }
        }

        // #tag (until space or end)
        if chars[i] == '#' && i + 1 < len && !chars[i + 1].is_whitespace() {
            flush_plain(&mut plain, base_style, &mut spans);
            let start = i;
            // Handle #[[page tag]] syntax
            if i + 2 < len && chars[i + 1] == '[' && chars[i + 2] == '[' {
                if let Some(end) = find_double_delimiter(&chars, i + 3, ']') {
                    let content: String = chars[start..end + 2].iter().collect();
                    spans.push(Span::styled(content, base_style.fg(Color::Cyan)));
                    i = end + 2;
                    continue;
                }
            }
            // Regular #tag
            i += 1;
            while i < len && !chars[i].is_whitespace() {
                i += 1;
            }
            let tag: String = chars[start..i].iter().collect();
            spans.push(Span::styled(tag, base_style.fg(Color::Cyan)));
            continue;
        }

        plain.push(chars[i]);
        i += 1;
    }

    flush_plain(&mut plain, base_style, &mut spans);
    spans
}

/// Represents a reference extracted from embed syntax.
enum RefText {
    BlockUid(String),
    PageName(String),
}

/// Parse the inner content of {{...}} to extract embed references.
/// Handles: `embed: ((uid))`, `[[embed]]: ((uid))`, `embed: [[page]]`, `[[embed]]: [[page]]`
fn parse_embed_inner(inner: &str) -> Option<RefText> {
    let trimmed = inner
        .strip_prefix("[[embed]]:")
        .or_else(|| inner.strip_prefix("embed:"))?
        .trim();

    if let Some(uid) = trimmed
        .strip_prefix("((")
        .and_then(|s| s.strip_suffix("))"))
    {
        Some(RefText::BlockUid(uid.to_string()))
    } else if let Some(page) = trimmed
        .strip_prefix("[[")
        .and_then(|s| s.strip_suffix("]]"))
    {
        Some(RefText::PageName(page.to_string()))
    } else {
        Some(RefText::PageName(trimmed.to_string()))
    }
}

/// Resolve a reference to display text using the block map.
fn resolve_ref(ref_text: &RefText, block_map: Option<&HashMap<String, String>>) -> String {
    match ref_text {
        RefText::BlockUid(uid) => {
            if let Some(map) = block_map {
                if let Some(text) = map.get(uid.as_str()) {
                    return text.clone();
                }
            }
            uid.clone()
        }
        RefText::PageName(name) => name.clone(),
    }
}

/// Parse `[text](url)` starting at position `start` (which points to the `[`).
/// Returns `(text, end_position)` where end_position is past the closing `)`.
fn parse_markdown_link(chars: &[char], start: usize) -> Option<(String, usize)> {
    // Find closing ]
    let mut i = start + 1;
    while i < chars.len() && chars[i] != ']' {
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    let text: String = chars[start + 1..i].iter().collect();
    i += 1; // skip ]

    // Expect (
    if i >= chars.len() || chars[i] != '(' {
        return None;
    }
    i += 1; // skip (

    // Find closing )
    let url_start = i;
    while i < chars.len() && chars[i] != ')' {
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    let _url: String = chars[url_start..i].iter().collect();
    i += 1; // skip )

    if text.is_empty() {
        return None;
    }

    Some((text, i))
}

fn flush_plain(plain: &mut String, style: Style, spans: &mut Vec<Span<'static>>) {
    if !plain.is_empty() {
        spans.push(Span::styled(plain.clone(), style));
        plain.clear();
    }
}

/// Find position of closing `}}` starting from `start`.
fn find_closing_double_brace(chars: &[char], start: usize) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == '}' && chars[i + 1] == '}' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Find position of single delimiter char starting from `start`.
fn find_single_delimiter(chars: &[char], start: usize, delim: char) -> Option<usize> {
    (start..chars.len()).find(|&i| chars[i] == delim)
}

/// Find position of double delimiter (e.g., `**`) starting from `start`.
fn find_double_delimiter(chars: &[char], start: usize, delim: char) -> Option<usize> {
    let mut i = start;
    while i + 1 < chars.len() {
        if chars[i] == delim && chars[i + 1] == delim {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Extract page names from `[[...]]` and `#[[...]]` links in block text.
///
/// Returns a deduplicated list of page names in order of first appearance.
pub fn extract_page_links(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut links = Vec::new();
    let mut seen = HashSet::new();
    let mut i = 0;

    while i < len {
        // Skip `inline code` — links inside code are not navigable
        if chars[i] == '`' {
            if let Some(end) = find_single_delimiter(&chars, i + 1, '`') {
                i = end + 1;
                continue;
            }
        }

        // #[[page tag]] — extract page name without the #
        if chars[i] == '#' && i + 2 < len && chars[i + 1] == '[' && chars[i + 2] == '[' {
            if let Some(end) = find_double_delimiter(&chars, i + 3, ']') {
                let name: String = chars[i + 3..end].iter().collect();
                if !name.is_empty() && seen.insert(name.clone()) {
                    links.push(name);
                }
                i = end + 2;
                continue;
            }
        }

        // [[page link]]
        if chars[i] == '[' && i + 1 < len && chars[i + 1] == '[' {
            if let Some(end) = find_double_delimiter(&chars, i + 2, ']') {
                let name: String = chars[i + 2..end].iter().collect();
                if !name.is_empty() && seen.insert(name.clone()) {
                    links.push(name);
                }
                i = end + 2;
                continue;
            }
        }

        i += 1;
    }

    links
}

/// Build a uid → text lookup map from loaded daily notes.
pub fn build_block_text_map(days: &[crate::api::types::DailyNote]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for day in days {
        collect_block_texts(&day.blocks, &mut map);
    }
    map
}

fn collect_block_texts(blocks: &[crate::api::types::Block], map: &mut HashMap<String, String>) {
    for block in blocks {
        if !block.string.is_empty() {
            map.insert(block.uid.clone(), block.string.clone());
        }
        collect_block_texts(&block.children, map);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_style() -> Style {
        Style::default().fg(Color::White)
    }

    #[test]
    fn plain_text_single_span() {
        let spans = render_spans("hello world", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "hello world");
    }

    #[test]
    fn bold_renders_bold() {
        let spans = render_spans("**word**", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "word");
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    #[test]
    fn italic_renders_italic() {
        let spans = render_spans("__word__", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "word");
        assert!(spans[0].style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn page_link_cyan_no_delimiters() {
        let spans = render_spans("[[page]]", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "page");
        assert_eq!(spans[0].style.fg, Some(Color::Cyan));
    }

    #[test]
    fn block_ref_magenta() {
        let spans = render_spans("((uid123))", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "uid123");
        assert_eq!(spans[0].style.fg, Some(Color::Magenta));
    }

    #[test]
    fn tag_cyan() {
        let spans = render_spans("#tag", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "#tag");
        assert_eq!(spans[0].style.fg, Some(Color::Cyan));
    }

    #[test]
    fn inline_code_green() {
        let spans = render_spans("`code`", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "code");
        assert_eq!(spans[0].style.fg, Some(Color::Green));
        assert_eq!(spans[0].style.bg, Some(Color::DarkGray));
    }

    #[test]
    fn highlight_yellow_bg() {
        let spans = render_spans("^^text^^", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "text");
        assert_eq!(spans[0].style.bg, Some(Color::Yellow));
    }

    #[test]
    fn strikethrough_gray() {
        let spans = render_spans("~~text~~", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "text");
        assert_eq!(spans[0].style.fg, Some(Color::DarkGray));
        assert!(spans[0].style.add_modifier.contains(Modifier::CROSSED_OUT));
    }

    #[test]
    fn todo_checkbox() {
        let spans = render_spans("{{TODO}}", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "☐ ");
        assert_eq!(spans[0].style.fg, Some(Color::Red));
    }

    #[test]
    fn done_checkmark() {
        let spans = render_spans("{{DONE}}", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "✓ ");
        assert_eq!(spans[0].style.fg, Some(Color::Green));
    }

    #[test]
    fn roam_todo_with_brackets() {
        let spans = render_spans("{{[[TODO]]}}", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "☐ ");
        assert_eq!(spans[0].style.fg, Some(Color::Red));
    }

    #[test]
    fn roam_done_with_brackets() {
        let spans = render_spans("{{[[DONE]]}}", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "✓ ");
        assert_eq!(spans[0].style.fg, Some(Color::Green));
    }

    #[test]
    fn roam_todo_brackets_before_text() {
        let spans = render_spans("{{[[TODO]]}} buy milk", default_style());
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, "☐ ");
        assert_eq!(spans[0].style.fg, Some(Color::Red));
        assert_eq!(spans[1].content, " buy milk");
    }

    #[test]
    fn mixed_formatting() {
        let spans = render_spans("**bold** and [[link]]", default_style());
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "bold");
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
        assert_eq!(spans[1].content, " and ");
        assert_eq!(spans[2].content, "link");
        assert_eq!(spans[2].style.fg, Some(Color::Cyan));
    }

    #[test]
    fn unclosed_delimiter_raw() {
        let spans = render_spans("**unclosed", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "**unclosed");
    }

    #[test]
    fn empty_string_empty_vec() {
        let spans = render_spans("", default_style());
        assert!(spans.is_empty());
    }

    #[test]
    fn tag_with_following_text() {
        let spans = render_spans("#tag rest", default_style());
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, "#tag");
        assert_eq!(spans[0].style.fg, Some(Color::Cyan));
        assert_eq!(spans[1].content, " rest");
    }

    #[test]
    fn todo_before_text() {
        let spans = render_spans("{{TODO}} buy milk", default_style());
        assert_eq!(spans.len(), 2);
        assert_eq!(spans[0].content, "☐ ");
        assert_eq!(spans[1].content, " buy milk");
    }

    #[test]
    fn preserves_base_style_bg_on_bold() {
        let base = Style::default().fg(Color::White).bg(Color::DarkGray);
        let spans = render_spans("**bold**", base);
        assert_eq!(spans[0].style.bg, Some(Color::DarkGray));
        assert!(spans[0].style.add_modifier.contains(Modifier::BOLD));
    }

    // --- block ref resolution tests ---

    #[test]
    fn block_ref_resolves_with_map() {
        let mut map = HashMap::new();
        map.insert("uid123".to_string(), "Referenced text".to_string());
        let spans = render_spans_with_refs("((uid123))", default_style(), Some(&map));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "Referenced text");
        assert_eq!(spans[0].style.fg, Some(Color::Magenta));
    }

    #[test]
    fn block_ref_shows_uid_without_map() {
        let spans = render_spans_with_refs("((uid123))", default_style(), None);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "uid123");
        assert_eq!(spans[0].style.fg, Some(Color::Magenta));
    }

    #[test]
    fn block_ref_shows_uid_when_not_in_map() {
        let map = HashMap::new();
        let spans = render_spans_with_refs("((unknown))", default_style(), Some(&map));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "unknown");
    }

    #[test]
    fn block_ref_with_surrounding_text() {
        let mut map = HashMap::new();
        map.insert("abc".to_string(), "hello world".to_string());
        let spans = render_spans_with_refs("see ((abc)) here", default_style(), Some(&map));
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "see ");
        assert_eq!(spans[1].content, "hello world");
        assert_eq!(spans[1].style.fg, Some(Color::Magenta));
        assert_eq!(spans[2].content, " here");
    }

    // --- embed tests ---

    #[test]
    fn embed_block_uid_renders_with_indicator() {
        let mut map = HashMap::new();
        map.insert("ref1".to_string(), "Embedded content".to_string());
        let spans = render_spans_with_refs("{{embed: ((ref1))}}", default_style(), Some(&map));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "▸ Embedded content");
        assert_eq!(spans[0].style.fg, Some(Color::Magenta));
        assert!(spans[0].style.add_modifier.contains(Modifier::ITALIC));
    }

    #[test]
    fn embed_with_brackets_syntax() {
        let mut map = HashMap::new();
        map.insert("ref1".to_string(), "Embedded content".to_string());
        let spans = render_spans_with_refs("{{[[embed]]: ((ref1))}}", default_style(), Some(&map));
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "▸ Embedded content");
    }

    #[test]
    fn embed_page_renders_page_name() {
        let spans = render_spans_with_refs("{{embed: [[my page]]}}", default_style(), None);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "▸ my page");
    }

    #[test]
    fn embed_unresolved_shows_uid() {
        let spans = render_spans_with_refs("{{embed: ((xyz))}}", default_style(), None);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "▸ xyz");
    }

    #[test]
    fn embed_plain_name() {
        let spans = render_spans_with_refs("{{embed: fin-questions}}", default_style(), None);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "▸ fin-questions");
    }

    // --- build_block_text_map tests ---

    #[test]
    fn build_map_from_flat_blocks() {
        use crate::api::types::{Block, DailyNote};
        use chrono::NaiveDate;

        let day = DailyNote {
            date: NaiveDate::from_ymd_opt(2026, 2, 21).unwrap(),
            uid: "02-21-2026".into(),
            title: "Feb 21".into(),
            blocks: vec![
                Block {
                    uid: "b1".into(),
                    string: "Hello".into(),
                    order: 0,
                    children: vec![],
                    open: true,
                },
                Block {
                    uid: "b2".into(),
                    string: "World".into(),
                    order: 1,
                    children: vec![],
                    open: true,
                },
            ],
        };
        let map = build_block_text_map(&[day]);
        assert_eq!(map.get("b1").unwrap(), "Hello");
        assert_eq!(map.get("b2").unwrap(), "World");
    }

    #[test]
    fn build_map_includes_nested_blocks() {
        use crate::api::types::{Block, DailyNote};
        use chrono::NaiveDate;

        let day = DailyNote {
            date: NaiveDate::from_ymd_opt(2026, 2, 21).unwrap(),
            uid: "02-21-2026".into(),
            title: "Feb 21".into(),
            blocks: vec![Block {
                uid: "p".into(),
                string: "Parent".into(),
                order: 0,
                children: vec![Block {
                    uid: "c".into(),
                    string: "Child".into(),
                    order: 0,
                    children: vec![],
                    open: true,
                }],
                open: true,
            }],
        };
        let map = build_block_text_map(&[day]);
        assert_eq!(map.get("p").unwrap(), "Parent");
        assert_eq!(map.get("c").unwrap(), "Child");
    }

    #[test]
    fn build_map_skips_empty_strings() {
        use crate::api::types::{Block, DailyNote};
        use chrono::NaiveDate;

        let day = DailyNote {
            date: NaiveDate::from_ymd_opt(2026, 2, 21).unwrap(),
            uid: "02-21-2026".into(),
            title: "Feb 21".into(),
            blocks: vec![Block {
                uid: "empty".into(),
                string: "".into(),
                order: 0,
                children: vec![],
                open: true,
            }],
        };
        let map = build_block_text_map(&[day]);
        assert!(!map.contains_key("empty"));
    }

    // --- markdown link tests ---

    #[test]
    fn markdown_link_renders_text_only() {
        let spans = render_spans("[Click here](https://example.com)", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "Click here");
        assert_eq!(spans[0].style.fg, Some(Color::Cyan));
        assert!(spans[0].style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn markdown_link_with_surrounding_text() {
        let spans = render_spans("see [link](http://x.com) here", default_style());
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "see ");
        assert_eq!(spans[1].content, "link");
        assert_eq!(spans[1].style.fg, Some(Color::Cyan));
        assert_eq!(spans[2].content, " here");
    }

    #[test]
    fn markdown_link_does_not_conflict_with_page_link() {
        let spans = render_spans("[[page]] and [link](url)", default_style());
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "page");
        assert_eq!(spans[0].style.fg, Some(Color::Cyan));
        assert_eq!(spans[1].content, " and ");
        assert_eq!(spans[2].content, "link");
        assert!(spans[2].style.add_modifier.contains(Modifier::UNDERLINED));
    }

    #[test]
    fn unclosed_markdown_link_shows_raw() {
        let spans = render_spans("[broken link", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "[broken link");
    }

    #[test]
    fn markdown_link_missing_parens_shows_raw() {
        let spans = render_spans("[text] no url", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "[text] no url");
    }

    // --- backward compatibility: render_spans still works without map ---

    #[test]
    fn render_spans_backward_compat_block_ref() {
        let spans = render_spans("((uid123))", default_style());
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "uid123");
        assert_eq!(spans[0].style.fg, Some(Color::Magenta));
    }

    // --- extract_page_links tests ---

    #[test]
    fn extract_no_links_from_plain_text() {
        assert!(extract_page_links("hello world").is_empty());
    }

    #[test]
    fn extract_single_page_link() {
        assert_eq!(extract_page_links("see [[My Page]]"), vec!["My Page"]);
    }

    #[test]
    fn extract_multiple_page_links() {
        assert_eq!(
            extract_page_links("[[Page A]] and [[Page B]]"),
            vec!["Page A", "Page B"]
        );
    }

    #[test]
    fn extract_deduplicates_links() {
        assert_eq!(
            extract_page_links("[[Dup]] then [[Dup]] again"),
            vec!["Dup"]
        );
    }

    #[test]
    fn extract_ignores_block_refs() {
        assert!(extract_page_links("((uid123))").is_empty());
    }

    #[test]
    fn extract_ignores_markdown_links() {
        assert!(extract_page_links("[text](http://url.com)").is_empty());
    }

    #[test]
    fn extract_hashtag_page_link() {
        assert_eq!(extract_page_links("#[[Tag Page]]"), vec!["Tag Page"]);
    }

    #[test]
    fn extract_mixed_links_and_tags() {
        assert_eq!(
            extract_page_links("[[Page]] and #[[Tag]]"),
            vec!["Page", "Tag"]
        );
    }

    #[test]
    fn extract_unclosed_brackets_ignored() {
        assert!(extract_page_links("[[unclosed").is_empty());
    }

    #[test]
    fn extract_link_inside_bold() {
        assert_eq!(extract_page_links("**[[Bold Page]]**"), vec!["Bold Page"]);
    }

    #[test]
    fn extract_ignores_links_in_inline_code() {
        assert!(extract_page_links("`[[Code]]`").is_empty());
    }

    #[test]
    fn extract_empty_brackets_skipped() {
        assert!(extract_page_links("[[]]").is_empty());
    }
}
