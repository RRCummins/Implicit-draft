//! Styles markdown source lines for Source+Hints mode.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::{code, theme::Theme};

pub fn style_document(lines: &[String], theme: &Theme) -> Vec<Line<'static>> {
    let mut fence_block: Option<FenceBlock> = None;
    let mut rendered = Vec::with_capacity(lines.len());

    for line in lines {
        let trimmed = line.trim_start();
        if fence_block
            .as_ref()
            .is_some_and(|block| trimmed.starts_with(block.marker))
        {
            if let Some(block) = fence_block.take() {
                rendered.extend(code::render_fenced_document(
                    &block.lines,
                    theme,
                    block.info.as_deref(),
                ));
                rendered.push(Line::from(vec![Span::styled(line.clone(), theme.code)]));
            }
            continue;
        }

        if let Some(block) = &mut fence_block {
            block.lines.push(line.clone());
            continue;
        }

        if let Some((marker, info)) = parse_fence_start(trimmed) {
            rendered.push(Line::from(vec![Span::styled(line.clone(), theme.code)]));
            fence_block = Some(FenceBlock::new(marker, info));
            continue;
        }

        rendered.push(style_line(line, theme));
    }

    if let Some(block) = fence_block {
        rendered.extend(code::render_fenced_document(
            &block.lines,
            theme,
            block.info.as_deref(),
        ));
    }

    rendered
}

struct FenceBlock {
    marker: &'static str,
    info: Option<String>,
    lines: Vec<String>,
}

impl FenceBlock {
    fn new(marker: &'static str, info: Option<&str>) -> Self {
        Self {
            marker,
            info: info.map(str::to_owned),
            lines: Vec::new(),
        }
    }
}

fn style_line(line: &str, theme: &Theme) -> Line<'static> {
    if is_conflict_marker(line) {
        return Line::from(vec![Span::styled(line.to_owned(), theme.conflict_marker)]);
    }

    if is_rule(line) {
        return Line::from(vec![Span::styled(line.to_owned(), theme.rule)]);
    }

    if let Some(level) = heading_level(line) {
        return inline_line(line, theme, theme.heading(level));
    }

    if is_blockquote(line) {
        return inline_line(line, theme, theme.blockquote);
    }

    if let Some((marker, rest)) = split_list_marker(line) {
        let mut spans = vec![Span::styled(marker.to_owned(), theme.list_marker)];
        spans.extend(inline_spans(rest, theme, Style::default()));
        return Line::from(spans);
    }

    inline_line(line, theme, Style::default())
}

fn inline_line(line: &str, theme: &Theme, base: Style) -> Line<'static> {
    Line::from(inline_spans(line, theme, base))
}

fn inline_spans(line: &str, theme: &Theme, base: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = line;

    while !remaining.is_empty() {
        if let Some((plain, special, style)) = next_inline_segment(remaining, theme) {
            if !plain.is_empty() {
                spans.push(Span::styled(plain.to_owned(), base));
            }
            spans.push(Span::styled(special.to_owned(), base.patch(style)));
            remaining = &remaining[plain.len() + special.len()..];
            continue;
        }

        spans.push(Span::styled(remaining.to_owned(), base));
        break;
    }

    spans
}

fn next_inline_segment<'a>(line: &'a str, theme: &Theme) -> Option<(&'a str, &'a str, Style)> {
    let mut best = parse_link(line).map(|(plain_end, segment_end)| {
        (
            &line[..plain_end],
            &line[plain_end..segment_end],
            theme.link,
            plain_end,
            segment_end,
        )
    });

    let patterns = [
        ("`", "`", theme.code),
        ("**", "**", theme.bold),
        ("__", "__", theme.bold),
        ("*", "*", theme.italic),
        ("_", "_", theme.italic),
    ];

    for (start, end, style) in patterns {
        if let Some((start_index, end_index)) = find_wrapped(line, start, end) {
            let replace = match best {
                Some((_, _, _, best_start, best_len)) => {
                    start_index < best_start || (start_index == best_start && end_index < best_len)
                }
                None => true,
            };

            if replace {
                best = Some((
                    &line[..start_index],
                    &line[start_index..end_index],
                    style,
                    start_index,
                    end_index,
                ));
            }
        }
    }

    best.map(|(plain, segment, style, _, _)| (plain, segment, style))
}

fn find_wrapped(line: &str, start: &str, end: &str) -> Option<(usize, usize)> {
    let start_index = line.find(start)?;
    let search_from = start_index + start.len();
    let rest = &line[search_from..];
    let end_index = rest.find(end)?;
    if end_index == 0 {
        return None;
    }
    Some((start_index, search_from + end_index + end.len()))
}

fn parse_link(line: &str) -> Option<(usize, usize)> {
    let start = line.find('[')?;
    let mid_relative = line[start..].find("](")?;
    let mid = start + mid_relative;
    let end_relative = line[mid + 2..].find(')')?;
    Some((start, mid + 2 + end_relative + 1))
}

fn fence_marker_for(trimmed: &str) -> Option<&'static str> {
    if trimmed.starts_with("```") {
        Some("```")
    } else if trimmed.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

fn parse_fence_start(trimmed: &str) -> Option<(&'static str, Option<&str>)> {
    let marker = fence_marker_for(trimmed)?;
    let info = trimmed[marker.len()..]
        .split_whitespace()
        .next()
        .filter(|token| !token.is_empty());
    Some((marker, info))
}

fn is_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }

    let mut markers = trimmed.chars().filter(|ch| !ch.is_whitespace());
    let Some(first) = markers.next() else {
        return false;
    };

    if !matches!(first, '-' | '*' | '_') {
        return false;
    }

    let mut count = 1;
    for marker in markers {
        if marker != first {
            return false;
        }
        count += 1;
    }

    count >= 3
}

fn heading_level(line: &str) -> Option<usize> {
    let trimmed = line.trim_start();
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if (1..=6).contains(&level) && trimmed.chars().nth(level) == Some(' ') {
        Some(level)
    } else {
        None
    }
}

fn is_blockquote(line: &str) -> bool {
    line.trim_start().starts_with('>')
}

fn split_list_marker(line: &str) -> Option<(&str, &str)> {
    let trimmed = line.trim_start();
    let offset = line.len() - trimmed.len();

    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(bullet) {
            return Some((&line[..offset + bullet.len()], rest));
        }
    }

    let digits = trimmed.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits > 0 && trimmed[digits..].starts_with(". ") {
        let marker_len = offset + digits + 2;
        return Some((&line[..marker_len], &line[marker_len..]));
    }

    None
}

fn is_conflict_marker(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("<<<<<<<")
        || trimmed.starts_with("=======")
        || trimmed.starts_with(">>>>>>>")
        || trimmed.starts_with("|||||||")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn styles_heading_lines() {
        let theme = Theme::source_hints_default();
        let rendered = style_document(&[String::from("# Title")], &theme);

        assert_eq!(rendered.len(), 1);
        assert_eq!(rendered[0].spans.len(), 1);
    }

    #[test]
    fn matches_ordered_list_marker() {
        let marker = split_list_marker("12. item").expect("ordered list marker");
        assert_eq!(marker.0, "12. ");
        assert_eq!(marker.1, "item");
    }

    #[test]
    fn detects_subsequence_link_queries() {
        assert_eq!(
            parse_link("[OpenAI](https://openai.com) rest"),
            Some((0, 28))
        );
        assert_eq!(
            parse_link("text [OpenAI](https://openai.com)"),
            Some((5, 33))
        );
    }

    #[test]
    fn styles_links_after_plain_prefix() {
        let theme = Theme::source_hints_default();
        let line = style_document(&[String::from("see [docs](https://example.com)")], &theme);

        assert_eq!(line[0].spans.len(), 2);
        assert_eq!(line[0].spans[0].content.as_ref(), "see ");
    }

    #[test]
    fn supports_tilde_fences() {
        let theme = Theme::source_hints_default();
        let rendered = style_document(
            &[
                String::from("~~~rust"),
                String::from("fn main() {}"),
                String::from("~~~"),
            ],
            &theme,
        );

        assert_eq!(rendered.len(), 3);
        assert!(
            rendered[1]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "fn" && span.style == theme.code_keyword)
        );
    }

    #[test]
    fn prefers_bold_match_over_empty_italic_pair() {
        let theme = Theme::source_hints_default();
        let rendered = style_document(&[String::from("**bold**")], &theme);

        assert_eq!(rendered[0].spans.len(), 1);
        assert_eq!(rendered[0].spans[0].content.as_ref(), "**bold**");
    }

    #[test]
    fn matches_spaced_rules() {
        assert!(is_rule("- - -"));
        assert!(is_rule("* * *"));
        assert!(is_rule("_ _ _"));
        assert!(!is_rule("- - x"));
    }

    #[test]
    fn fenced_python_blocks_use_code_highlighting() {
        let theme = Theme::source_hints_default();
        let rendered = style_document(
            &[
                String::from("```python"),
                String::from("def greet(name):"),
                String::from("    return name"),
                String::from("```"),
            ],
            &theme,
        );

        assert_eq!(rendered[1].spans[0].content.as_ref(), "def");
        assert_eq!(rendered[1].spans[0].style, theme.code_keyword);
        assert!(
            rendered[2]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "return" && span.style == theme.code_keyword)
        );
    }

    #[test]
    fn styles_conflict_marker_lines() {
        let theme = Theme::source_hints_default();
        let rendered = style_document(&[String::from("<<<<<<< HEAD")], &theme);

        assert_eq!(rendered[0].spans[0].style, theme.conflict_marker);
    }
}
