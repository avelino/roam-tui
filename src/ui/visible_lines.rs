use ratatui::style::{Color, Style};
use ratatui::text::Span;

use crate::api::types::{Block, DailyNote};
use crate::app::LinkedRefsState;
use crate::highlight::CodeHighlighter;
use crate::markdown;

#[derive(Debug, Clone)]
pub(crate) enum VisibleLine {
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
    LinkedRefsSeparator,
    LinkedRefsHeader {
        count: usize,
        collapsed: bool,
        block_index: usize,
    },
    LinkedRefsGroupHeader {
        page_title: String,
        block_index: usize,
    },
    LinkedRefsBlock {
        text: String,
        block_index: usize,
    },
}

/// Check if a block's text represents a code block (starts with ```)
pub(crate) fn is_code_block(text: &str) -> bool {
    text.starts_with("```")
}

/// Check if a block's text represents a blockquote (starts with `> `)
pub(crate) fn is_blockquote(text: &str) -> bool {
    text.starts_with("> ")
}

/// Markdown lang code blocks should be rendered with inline formatting, not as code
pub(crate) fn is_markdown_lang(lang: &str) -> bool {
    matches!(lang, "md" | "markdown" | "")
}

/// Parse a code block string into (language, code_lines).
/// Roam stores code blocks as: ```\nlang\ncode...```
pub(crate) fn parse_code_block(text: &str) -> (&str, &str) {
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

pub(crate) fn build_visible_lines(
    days: &[DailyNote],
    loading_more: bool,
    highlighter: &mut CodeHighlighter,
    linked_refs: &std::collections::HashMap<String, LinkedRefsState>,
) -> Vec<VisibleLine> {
    let mut lines = Vec::new();
    let mut block_index = 0;

    for (i, day) in days.iter().enumerate() {
        if i > 0 {
            lines.push(VisibleLine::DaySeparator);
        }
        lines.push(VisibleLine::DayHeading(day.title.clone()));
        flatten_blocks(&day.blocks, 0, &mut lines, &mut block_index, highlighter);

        // Append linked references for this day
        if let Some(lr) = linked_refs.get(&day.title) {
            append_linked_refs(lr, &mut lines, &mut block_index);
        }
    }

    if loading_more {
        lines.push(VisibleLine::DaySeparator);
        lines.push(VisibleLine::LoadingMore);
    }

    lines
}

fn append_linked_refs(lr: &LinkedRefsState, lines: &mut Vec<VisibleLine>, block_index: &mut usize) {
    if lr.loading {
        lines.push(VisibleLine::LinkedRefsSeparator);
        lines.push(VisibleLine::LinkedRefsHeader {
            count: 0,
            collapsed: false,
            block_index: *block_index,
        });
    } else if !lr.groups.is_empty() {
        let total_count: usize = lr.groups.iter().map(|g| g.blocks.len()).sum();
        lines.push(VisibleLine::LinkedRefsSeparator);
        lines.push(VisibleLine::LinkedRefsHeader {
            count: total_count,
            collapsed: lr.collapsed,
            block_index: *block_index,
        });
        *block_index += 1;

        if !lr.collapsed {
            for group in &lr.groups {
                lines.push(VisibleLine::LinkedRefsGroupHeader {
                    page_title: group.page_title.clone(),
                    block_index: *block_index,
                });
                *block_index += 1;

                for block in &group.blocks {
                    lines.push(VisibleLine::LinkedRefsBlock {
                        text: block.string.clone(),
                        block_index: *block_index,
                    });
                    *block_index += 1;
                }
            }
        }
    }
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
