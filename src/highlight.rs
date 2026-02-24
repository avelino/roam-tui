use ratatui::style::{Color, Style};
use ratatui::text::Span;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

const HIGHLIGHT_NAMES: &[&str] = &[
    "keyword",
    "string",
    "comment",
    "function",
    "type",
    "number",
    "operator",
    "variable",
    "constant",
    "property",
    "punctuation",
    "tag",
    "attribute",
];

fn style_for_highlight(index: usize, base_style: Style) -> Style {
    match HIGHLIGHT_NAMES.get(index) {
        Some(&"keyword") => base_style.fg(Color::Blue),
        Some(&"string") => base_style.fg(Color::Green),
        Some(&"comment") => base_style.fg(Color::DarkGray),
        Some(&"function") => base_style.fg(Color::Yellow),
        Some(&"type") => base_style.fg(Color::Cyan),
        Some(&"number") => base_style.fg(Color::Magenta),
        Some(&"operator") => base_style.fg(Color::White),
        Some(&"variable") => base_style.fg(Color::White),
        Some(&"constant") => base_style.fg(Color::Magenta),
        Some(&"property") => base_style.fg(Color::Cyan),
        Some(&"punctuation") => base_style.fg(Color::White),
        Some(&"tag") => base_style.fg(Color::Red),
        Some(&"attribute") => base_style.fg(Color::Yellow),
        _ => base_style,
    }
}

fn make_config(
    lang: tree_sitter::Language,
    highlights: &str,
) -> Option<HighlightConfiguration> {
    let mut config = HighlightConfiguration::new(lang, "highlight", highlights, "", "").ok()?;
    config.configure(HIGHLIGHT_NAMES);
    Some(config)
}

fn load_lang_config(lang: &str) -> Option<HighlightConfiguration> {
    let (language, highlights) = match lang {
        "rust" | "rs" => (
            tree_sitter_rust::LANGUAGE.into(),
            tree_sitter_rust::HIGHLIGHTS_QUERY,
        ),
        "python" | "py" => (
            tree_sitter_python::LANGUAGE.into(),
            tree_sitter_python::HIGHLIGHTS_QUERY,
        ),
        "javascript" | "js" => (
            tree_sitter_javascript::LANGUAGE.into(),
            tree_sitter_javascript::HIGHLIGHT_QUERY,
        ),
        "typescript" | "ts" => (
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
        ),
        "tsx" => (
            tree_sitter_typescript::LANGUAGE_TSX.into(),
            tree_sitter_typescript::HIGHLIGHTS_QUERY,
        ),
        "go" => (
            tree_sitter_go::LANGUAGE.into(),
            tree_sitter_go::HIGHLIGHTS_QUERY,
        ),
        "bash" | "sh" | "shell" => (
            tree_sitter_bash::LANGUAGE.into(),
            tree_sitter_bash::HIGHLIGHT_QUERY,
        ),
        "json" => (
            tree_sitter_json::LANGUAGE.into(),
            tree_sitter_json::HIGHLIGHTS_QUERY,
        ),
        "html" => (
            tree_sitter_html::LANGUAGE.into(),
            tree_sitter_html::HIGHLIGHTS_QUERY,
        ),
        "css" => (
            tree_sitter_css::LANGUAGE.into(),
            tree_sitter_css::HIGHLIGHTS_QUERY,
        ),
        "c" => (
            tree_sitter_c::LANGUAGE.into(),
            tree_sitter_c::HIGHLIGHT_QUERY,
        ),
        "toml" => (
            tree_sitter_toml_ng::LANGUAGE.into(),
            tree_sitter_toml_ng::HIGHLIGHTS_QUERY,
        ),
        "yaml" | "yml" => (
            tree_sitter_yaml::LANGUAGE.into(),
            tree_sitter_yaml::HIGHLIGHTS_QUERY,
        ),
        "markdown" | "md" => (
            tree_sitter_md::LANGUAGE.into(),
            "",
        ),
        _ => return None,
    };

    make_config(language, highlights)
}

pub struct CodeHighlighter {
    highlighter: Highlighter,
}

impl CodeHighlighter {
    pub fn new() -> Self {
        CodeHighlighter {
            highlighter: Highlighter::new(),
        }
    }

    /// Highlight a code block. Returns one `Vec<Span>` per line of code.
    /// Falls back to plain text if language is not supported.
    pub fn highlight_code(
        &mut self,
        lang: &str,
        code: &str,
        base_style: Style,
    ) -> Vec<Vec<Span<'static>>> {
        if code.is_empty() {
            return vec![];
        }

        let config = match load_lang_config(lang) {
            Some(c) => c,
            None => return plain_lines(code, base_style),
        };

        let events = match self.highlighter.highlight(&config, code.as_bytes(), None, |_| None) {
            Ok(events) => events,
            Err(_) => return plain_lines(code, base_style),
        };

        let mut lines: Vec<Vec<Span<'static>>> = vec![vec![]];
        let mut current_style = base_style;

        for event in events {
            match event {
                Ok(HighlightEvent::Source { start, end }) => {
                    let text = &code[start..end];
                    for (i, part) in text.split('\n').enumerate() {
                        if i > 0 {
                            lines.push(vec![]);
                        }
                        if !part.is_empty() {
                            lines
                                .last_mut()
                                .unwrap()
                                .push(Span::styled(part.to_string(), current_style));
                        }
                    }
                }
                Ok(HighlightEvent::HighlightStart(h)) => {
                    current_style = style_for_highlight(h.0, base_style);
                }
                Ok(HighlightEvent::HighlightEnd) => {
                    current_style = base_style;
                }
                Err(_) => break,
            }
        }

        // Remove trailing empty line if the code doesn't end with newline
        if lines.last().map_or(false, |l| l.is_empty()) && !code.ends_with('\n') {
            lines.pop();
        }

        lines
    }
}

fn plain_lines(code: &str, base_style: Style) -> Vec<Vec<Span<'static>>> {
    code.lines()
        .map(|line| vec![Span::styled(line.to_string(), base_style)])
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Style {
        Style::default().fg(Color::White)
    }

    #[test]
    fn highlight_rust_code() {
        let mut hl = CodeHighlighter::new();
        let lines = hl.highlight_code("rust", "fn main() {}", base());
        assert!(!lines.is_empty());
        // "fn" should be a keyword (Blue)
        let first_line = &lines[0];
        assert!(!first_line.is_empty());
        let has_keyword = first_line
            .iter()
            .any(|s| s.content.contains("fn") && s.style.fg == Some(Color::Blue));
        assert!(has_keyword, "Expected 'fn' keyword in blue, got: {:?}", first_line);
    }

    #[test]
    fn highlight_python_code() {
        let mut hl = CodeHighlighter::new();
        let lines = hl.highlight_code("python", "def hello():\n    pass", base());
        assert!(lines.len() >= 2);
        let has_keyword = lines[0]
            .iter()
            .any(|s| s.content.contains("def") && s.style.fg == Some(Color::Blue));
        assert!(has_keyword, "Expected 'def' keyword in blue, got: {:?}", lines[0]);
    }

    #[test]
    fn unknown_lang_fallback() {
        let mut hl = CodeHighlighter::new();
        let lines = hl.highlight_code("brainfuck", "+++.", base());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0][0].content, "+++.");
    }

    #[test]
    fn empty_code_returns_empty() {
        let mut hl = CodeHighlighter::new();
        let lines = hl.highlight_code("rust", "", base());
        assert!(lines.is_empty());
    }

    #[test]
    fn multiline_code_returns_multiple_lines() {
        let mut hl = CodeHighlighter::new();
        let code = "let x = 1;\nlet y = 2;\nlet z = 3;";
        let lines = hl.highlight_code("rust", code, base());
        assert_eq!(lines.len(), 3, "Expected 3 lines, got: {:?}", lines);
    }

    #[test]
    fn javascript_support() {
        let mut hl = CodeHighlighter::new();
        let lines = hl.highlight_code("js", "const x = 42;", base());
        assert!(!lines.is_empty());
    }

    #[test]
    fn go_support() {
        let mut hl = CodeHighlighter::new();
        let lines = hl.highlight_code("go", "func main() {}", base());
        assert!(!lines.is_empty());
    }

    #[test]
    fn bash_support() {
        let mut hl = CodeHighlighter::new();
        let lines = hl.highlight_code("bash", "echo hello", base());
        assert!(!lines.is_empty());
    }
}
