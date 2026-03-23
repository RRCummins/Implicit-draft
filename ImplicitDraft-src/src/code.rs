//! Renders non-markdown source files with language-aware syntax hints.

use std::path::Path;

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::{filetype::FileType, theme::Theme};

const RUST_KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern",
    "false", "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub",
    "ref", "return", "Self", "self", "static", "struct", "super", "trait", "true", "type",
    "unsafe", "use", "where", "while",
];
const JS_TS_KEYWORDS: &[&str] = &[
    "async",
    "await",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "else",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "from",
    "function",
    "if",
    "import",
    "in",
    "interface",
    "let",
    "new",
    "null",
    "return",
    "static",
    "super",
    "switch",
    "this",
    "throw",
    "true",
    "try",
    "type",
    "typeof",
    "undefined",
    "var",
    "while",
    "yield",
];
const PYTHON_KEYWORDS: &[&str] = &[
    "and", "as", "async", "await", "break", "class", "continue", "def", "elif", "else", "False",
    "for", "from", "if", "import", "in", "is", "lambda", "None", "not", "or", "pass", "raise",
    "return", "self", "True", "try", "while", "with", "yield",
];
const SHELL_KEYWORDS: &[&str] = &[
    "case", "do", "done", "elif", "else", "esac", "export", "fi", "for", "function", "if", "in",
    "local", "readonly", "return", "select", "then", "until", "while",
];
const GO_KEYWORDS: &[&str] = &[
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "false",
    "for",
    "func",
    "go",
    "if",
    "import",
    "interface",
    "map",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "true",
    "type",
    "var",
];
const SWIFT_KEYWORDS: &[&str] = &[
    "associatedtype",
    "break",
    "case",
    "class",
    "continue",
    "default",
    "defer",
    "else",
    "enum",
    "extension",
    "false",
    "for",
    "func",
    "guard",
    "if",
    "import",
    "in",
    "init",
    "let",
    "nil",
    "protocol",
    "return",
    "self",
    "static",
    "struct",
    "switch",
    "true",
    "typealias",
    "var",
    "where",
    "while",
];
const C_LIKE_KEYWORDS: &[&str] = &[
    "auto",
    "bool",
    "break",
    "case",
    "catch",
    "char",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "double",
    "else",
    "enum",
    "false",
    "float",
    "for",
    "if",
    "include",
    "inline",
    "int",
    "long",
    "namespace",
    "new",
    "nullptr",
    "private",
    "protected",
    "public",
    "return",
    "short",
    "signed",
    "sizeof",
    "static",
    "struct",
    "switch",
    "template",
    "this",
    "throw",
    "true",
    "try",
    "typedef",
    "typename",
    "union",
    "unsigned",
    "using",
    "virtual",
    "void",
    "while",
];
const WEB_KEYWORDS: &[&str] = &[
    "body",
    "class",
    "color",
    "display",
    "div",
    "font-family",
    "grid",
    "height",
    "html",
    "id",
    "margin",
    "padding",
    "section",
    "span",
    "style",
    "width",
];
const DATA_KEYWORDS: &[&str] = &[
    "auto_save",
    "default_mode",
    "false",
    "line_numbers",
    "none",
    "null",
    "off",
    "on",
    "tab_width",
    "theme",
    "true",
    "vim_keys",
    "wrap",
];
const GENERIC_KEYWORDS: &[&str] = &[
    "break", "case", "class", "const", "continue", "default", "else", "enum", "false", "fn", "for",
    "function", "if", "import", "in", "let", "null", "return", "struct", "switch", "true", "type",
    "var", "while",
];
const DEFAULT_STRING_DELIMITERS: &[char] = &['"', '\''];
const JS_STRING_DELIMITERS: &[char] = &['"', '\'', '`'];

pub fn render_document(
    lines: &[String],
    theme: &Theme,
    path: Option<&Path>,
    file_type: FileType,
) -> Vec<Line<'static>> {
    let language = Language::for_document(path, file_type);
    let mut state = RenderState::default();
    lines
        .iter()
        .map(|line| render_source_line(line, theme, language, &mut state))
        .collect()
}

pub fn render_preview_document(
    lines: &[String],
    theme: &Theme,
    path: Option<&Path>,
    file_type: FileType,
    width: usize,
) -> Vec<Line<'static>> {
    let language = Language::for_document(path, file_type);
    let gutter_width = lines.len().max(1).to_string().len();
    let content_width = width.saturating_sub(gutter_width + 3).max(1);
    let mut state = RenderState::default();

    lines
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let mut spans = vec![Span::styled(
                format!("{:>gutter_width$} │ ", index + 1),
                theme.ui_chrome,
            )];
            let visible = truncate_chars(line, content_width);
            spans.extend(render_spans(&visible, theme, language, &mut state));
            Line::from(spans)
        })
        .collect()
}

#[derive(Clone, Copy, Debug, Default)]
struct RenderState {
    block_comment_end: Option<&'static str>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Language {
    Rust,
    JavaScript,
    Python,
    Shell,
    Go,
    Swift,
    CLike,
    Web,
    Data,
    Generic,
}

impl Language {
    fn for_document(path: Option<&Path>, file_type: FileType) -> Self {
        if file_type != FileType::Code {
            return Self::Generic;
        }

        match path
            .and_then(|path| path.extension())
            .and_then(|ext| ext.to_str())
            .unwrap_or_default()
        {
            "rs" => Self::Rust,
            "js" | "jsx" | "ts" | "tsx" => Self::JavaScript,
            "py" | "rb" => Self::Python,
            "sh" | "bash" | "zsh" => Self::Shell,
            "go" => Self::Go,
            "swift" => Self::Swift,
            "c" | "h" | "cpp" | "hpp" | "java" | "kt" | "php" => Self::CLike,
            "css" | "html" => Self::Web,
            "json" | "toml" | "yaml" | "yml" => Self::Data,
            _ => Self::Generic,
        }
    }

    fn grammar(self) -> Grammar {
        match self {
            Self::Rust => Grammar {
                line_comment: Some("//"),
                block_comment: Some(("/*", "*/")),
                string_delimiters: DEFAULT_STRING_DELIMITERS,
                keywords: RUST_KEYWORDS,
            },
            Self::JavaScript => Grammar {
                line_comment: Some("//"),
                block_comment: Some(("/*", "*/")),
                string_delimiters: JS_STRING_DELIMITERS,
                keywords: JS_TS_KEYWORDS,
            },
            Self::Python => Grammar {
                line_comment: Some("#"),
                block_comment: None,
                string_delimiters: DEFAULT_STRING_DELIMITERS,
                keywords: PYTHON_KEYWORDS,
            },
            Self::Shell => Grammar {
                line_comment: Some("#"),
                block_comment: None,
                string_delimiters: DEFAULT_STRING_DELIMITERS,
                keywords: SHELL_KEYWORDS,
            },
            Self::Go => Grammar {
                line_comment: Some("//"),
                block_comment: Some(("/*", "*/")),
                string_delimiters: DEFAULT_STRING_DELIMITERS,
                keywords: GO_KEYWORDS,
            },
            Self::Swift => Grammar {
                line_comment: Some("//"),
                block_comment: Some(("/*", "*/")),
                string_delimiters: DEFAULT_STRING_DELIMITERS,
                keywords: SWIFT_KEYWORDS,
            },
            Self::CLike => Grammar {
                line_comment: Some("//"),
                block_comment: Some(("/*", "*/")),
                string_delimiters: DEFAULT_STRING_DELIMITERS,
                keywords: C_LIKE_KEYWORDS,
            },
            Self::Web => Grammar {
                line_comment: None,
                block_comment: Some(("/*", "*/")),
                string_delimiters: DEFAULT_STRING_DELIMITERS,
                keywords: WEB_KEYWORDS,
            },
            Self::Data => Grammar {
                line_comment: Some("#"),
                block_comment: None,
                string_delimiters: DEFAULT_STRING_DELIMITERS,
                keywords: DATA_KEYWORDS,
            },
            Self::Generic => Grammar {
                line_comment: None,
                block_comment: None,
                string_delimiters: DEFAULT_STRING_DELIMITERS,
                keywords: GENERIC_KEYWORDS,
            },
        }
    }
}

#[derive(Clone, Copy)]
struct Grammar {
    line_comment: Option<&'static str>,
    block_comment: Option<(&'static str, &'static str)>,
    string_delimiters: &'static [char],
    keywords: &'static [&'static str],
}

fn render_source_line(
    line: &str,
    theme: &Theme,
    language: Language,
    state: &mut RenderState,
) -> Line<'static> {
    Line::from(render_spans(line, theme, language, state))
}

fn render_spans(
    line: &str,
    theme: &Theme,
    language: Language,
    state: &mut RenderState,
) -> Vec<Span<'static>> {
    let grammar = language.grammar();
    let chars = line.char_indices().collect::<Vec<_>>();
    let mut spans = Vec::new();
    let mut index = 0;

    while index < chars.len() {
        let (byte_index, ch) = chars[index];

        if let Some(end_marker) = state.block_comment_end {
            let end = match line[byte_index..].find(end_marker) {
                Some(relative) => {
                    let end = byte_index + relative + end_marker.len();
                    state.block_comment_end = None;
                    end
                }
                None => line.len(),
            };
            spans.push(Span::styled(
                line[byte_index..end].to_owned(),
                theme.code_comment,
            ));
            index = char_position_at_or_after(&chars, end);
            continue;
        }

        if let Some(comment) = grammar.line_comment
            && line[byte_index..].starts_with(comment)
        {
            spans.push(Span::styled(
                line[byte_index..].to_owned(),
                theme.code_comment,
            ));
            break;
        }

        if let Some((start_marker, end_marker)) = grammar.block_comment
            && line[byte_index..].starts_with(start_marker)
        {
            let end = match line[byte_index + start_marker.len()..].find(end_marker) {
                Some(relative) => byte_index + start_marker.len() + relative + end_marker.len(),
                None => {
                    state.block_comment_end = Some(end_marker);
                    line.len()
                }
            };
            spans.push(Span::styled(
                line[byte_index..end].to_owned(),
                theme.code_comment,
            ));
            index = char_position_at_or_after(&chars, end);
            continue;
        }

        if let Some((key_end, delimiter_end)) =
            data_key_range(line, byte_index, language, grammar.line_comment)
        {
            spans.push(Span::styled(
                line[byte_index..key_end].to_owned(),
                theme.code_type,
            ));
            if delimiter_end > key_end {
                spans.push(Span::styled(
                    line[key_end..delimiter_end].to_owned(),
                    theme.code_punctuation,
                ));
            }
            index = char_position_at_or_after(&chars, delimiter_end);
            continue;
        }

        if grammar.string_delimiters.contains(&ch) {
            let end = find_string_end(line, &chars, index + 1, ch);
            spans.push(Span::styled(
                line[byte_index..end].to_owned(),
                theme.code_string,
            ));
            index = char_position_at_or_after(&chars, end);
            continue;
        }

        if let Some(end) = shell_variable_end(line, &chars, index, language) {
            spans.push(Span::styled(
                line[byte_index..end].to_owned(),
                theme.code_type,
            ));
            index = char_position_at_or_after(&chars, end);
            continue;
        }

        if ch.is_ascii_digit() {
            let end = find_number_end(line, &chars, index + 1);
            spans.push(Span::styled(
                line[byte_index..end].to_owned(),
                theme.code_number,
            ));
            index = char_position_at_or_after(&chars, end);
            continue;
        }

        if is_identifier_start(ch) {
            let end = find_ident_end(line, &chars, index + 1);
            let token = &line[byte_index..end];
            let style = if grammar.keywords.contains(&token) {
                theme.code_keyword
            } else if is_type_like(token, language) {
                theme.code_type
            } else {
                Style::default()
            };
            spans.push(Span::styled(token.to_owned(), style));
            index = char_position_at_or_after(&chars, end);
            continue;
        }

        let style = if is_punctuation(ch) {
            theme.code_punctuation
        } else {
            Style::default()
        };
        spans.push(Span::styled(ch.to_string(), style));
        index += 1;
    }

    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }

    spans
}

fn find_string_end(
    line: &str,
    chars: &[(usize, char)],
    mut index: usize,
    delimiter: char,
) -> usize {
    let mut escaped = false;
    while index < chars.len() {
        let (byte_index, ch) = chars[index];
        if escaped {
            escaped = false;
        } else if ch == '\\' {
            escaped = true;
        } else if ch == delimiter {
            return byte_index + ch.len_utf8();
        }
        index += 1;
    }
    line.len()
}

fn find_number_end(line: &str, chars: &[(usize, char)], mut index: usize) -> usize {
    while index < chars.len() {
        let (byte_index, ch) = chars[index];
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.')) {
            return byte_index;
        }
        index += 1;
    }
    line.len()
}

fn find_ident_end(line: &str, chars: &[(usize, char)], mut index: usize) -> usize {
    while index < chars.len() {
        let (byte_index, ch) = chars[index];
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')) {
            return byte_index;
        }
        index += 1;
    }
    line.len()
}

fn data_key_range(
    line: &str,
    byte_index: usize,
    language: Language,
    line_comment: Option<&str>,
) -> Option<(usize, usize)> {
    if language != Language::Data || byte_index != data_key_start(line)? {
        return None;
    }

    let search_end = comment_start_after(line, line_comment);
    let relevant = &line[byte_index..search_end];

    if relevant.starts_with('"') || relevant.starts_with('\'') {
        let delimiter = relevant.chars().next()?;
        let local_end = find_quoted_key_end(relevant, delimiter)?;
        let remainder = &relevant[local_end..];
        let delimiter_offset = remainder
            .char_indices()
            .find_map(|(offset, ch)| matches!(ch, ':' | '=').then_some(offset))?;
        let key_end = byte_index + local_end;
        let delimiter_start = key_end + delimiter_offset;
        return Some((key_end, delimiter_start + 1));
    }

    let delimiter_offset = relevant
        .char_indices()
        .find_map(|(offset, ch)| matches!(ch, ':' | '=').then_some(offset))?;
    let raw_key = relevant[..delimiter_offset].trim_end();
    if raw_key.is_empty() {
        return None;
    }
    let key_end = byte_index + raw_key.len();
    Some((key_end, byte_index + delimiter_offset + 1))
}

fn data_key_start(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let offset = line.len() - trimmed.len();
    let trimmed = trimmed.strip_prefix("- ").unwrap_or(trimmed);
    let marker_offset = if line[offset..].starts_with("- ") {
        2
    } else {
        0
    };
    if trimmed.is_empty() {
        None
    } else {
        Some(offset + marker_offset)
    }
}

fn find_quoted_key_end(text: &str, delimiter: char) -> Option<usize> {
    let mut escaped = false;
    for (offset, ch) in text.char_indices().skip(1) {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == delimiter {
            return Some(offset + ch.len_utf8());
        }
    }
    None
}

fn comment_start_after(line: &str, line_comment: Option<&str>) -> usize {
    line_comment
        .and_then(|marker| line.find(marker))
        .unwrap_or(line.len())
}

fn shell_variable_end(
    line: &str,
    chars: &[(usize, char)],
    index: usize,
    language: Language,
) -> Option<usize> {
    if language != Language::Shell || chars.get(index)?.1 != '$' {
        return None;
    }

    let start = chars[index].0;
    let next = chars.get(index + 1).copied()?;
    if next.1 == '{' {
        for (byte_index, ch) in chars.iter().copied().skip(index + 2) {
            if ch == '}' {
                return Some(byte_index + ch.len_utf8());
            }
        }
        return Some(line.len());
    }

    if !is_identifier_start(next.1) {
        return None;
    }

    let end = find_ident_end(line, chars, index + 2);
    Some(end.max(start + 1))
}

fn char_position_at_or_after(chars: &[(usize, char)], byte_index: usize) -> usize {
    chars
        .iter()
        .position(|(candidate, _)| *candidate >= byte_index)
        .unwrap_or(chars.len())
}

fn is_identifier_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_type_like(token: &str, language: Language) -> bool {
    match language {
        Language::Rust | Language::Go | Language::Swift | Language::CLike => token
            .chars()
            .next()
            .is_some_and(|first| first.is_ascii_uppercase()),
        _ => false,
    }
}

fn is_punctuation(ch: char) -> bool {
    matches!(
        ch,
        '{' | '}'
            | '['
            | ']'
            | '('
            | ')'
            | ':'
            | ';'
            | ','
            | '.'
            | '='
            | '+'
            | '-'
            | '*'
            | '/'
            | '|'
            | '&'
            | '<'
            | '>'
            | '!'
            | '%'
    )
}

fn truncate_chars(line: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let visible = line.chars().take(width).collect::<String>();
    if visible.chars().count() == line.chars().count() {
        visible
    } else if width == 1 {
        "…".to_owned()
    } else {
        let mut truncated = visible.chars().take(width - 1).collect::<String>();
        truncated.push('…');
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn highlights_rust_keywords_strings_and_comments() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[String::from("fn main() { let name = \"implicit\"; // hi }")],
            &theme,
            Some(Path::new("main.rs")),
            FileType::Code,
        );

        let spans = &rendered[0].spans;
        assert_eq!(spans[0].content.as_ref(), "fn");
        assert!(
            spans
                .iter()
                .any(|span| span.content.as_ref() == "\"implicit\"")
        );
        assert_eq!(spans.last().expect("comment").content.as_ref(), "// hi }");
    }

    #[test]
    fn rust_attributes_are_not_treated_as_comments() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[String::from("#[derive(Debug)]")],
            &theme,
            Some(Path::new("main.rs")),
            FileType::Code,
        );

        assert_eq!(rendered[0].spans[0].content.as_ref(), "#");
    }

    #[test]
    fn shell_hash_comments_are_highlighted() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[String::from("echo implicit # note")],
            &theme,
            Some(Path::new("script.sh")),
            FileType::Code,
        );

        assert_eq!(
            rendered[0].spans.last().expect("comment").content.as_ref(),
            "# note"
        );
    }

    #[test]
    fn block_comments_span_multiple_lines() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[
                String::from("/* start"),
                String::from("still comment */ let value = 1;"),
            ],
            &theme,
            Some(Path::new("main.rs")),
            FileType::Code,
        );

        assert_eq!(rendered[0].spans[0].content.as_ref(), "/* start");
        assert_eq!(rendered[1].spans[0].content.as_ref(), "still comment */");
        assert!(
            rendered[1]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "let")
        );
    }

    #[test]
    fn preview_renders_line_number_gutter() {
        let theme = Theme::source_hints_default();
        let rendered = render_preview_document(
            &[String::from("fn main() {}")],
            &theme,
            Some(Path::new("main.rs")),
            FileType::Code,
            40,
        );

        assert_eq!(rendered[0].spans[0].content.as_ref(), "1 │ ");
    }

    #[test]
    fn keyword_tokens_use_code_keyword_style() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[String::from("fn main() {}")],
            &theme,
            Some(Path::new("main.rs")),
            FileType::Code,
        );

        assert_eq!(rendered[0].spans[0].style, theme.code_keyword);
    }

    #[test]
    fn data_keys_use_code_type_style() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[String::from("theme = \"dark\"")],
            &theme,
            Some(Path::new("config.toml")),
            FileType::Code,
        );

        assert_eq!(rendered[0].spans[0].content.as_ref(), "theme");
        assert_eq!(rendered[0].spans[0].style, theme.code_type);
    }

    #[test]
    fn shell_variables_use_code_type_style() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[String::from("echo $HOME ${USER}")],
            &theme,
            Some(Path::new("script.sh")),
            FileType::Code,
        );

        assert!(
            rendered[0]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "$HOME" && span.style == theme.code_type)
        );
        assert!(
            rendered[0]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "${USER}" && span.style == theme.code_type)
        );
    }

    #[test]
    fn file_type_detection_still_handles_unknown_code_generically() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[String::from("const value = 42;")],
            &theme,
            Some(&PathBuf::from("notes.custom")),
            FileType::Unknown,
        );

        assert!(
            rendered[0]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "42")
        );
    }
}
