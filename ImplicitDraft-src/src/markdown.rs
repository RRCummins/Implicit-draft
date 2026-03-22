use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::theme::Theme;

pub fn style_document(lines: &[String], theme: &Theme) -> Vec<Line<'static>> {
    let mut in_code_block = false;
    let mut rendered = Vec::with_capacity(lines.len());

    for line in lines {
        let trimmed = line.trim_start();
        let is_fence = trimmed.starts_with("```");

        if is_fence {
            rendered.push(Line::from(vec![Span::styled(line.clone(), theme.code)]));
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            rendered.push(Line::from(vec![Span::styled(line.clone(), theme.code)]));
            continue;
        }

        rendered.push(style_line(line, theme));
    }

    rendered
}

fn style_line(line: &str, theme: &Theme) -> Line<'static> {
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
        if let Some(link_end) = parse_link(remaining) {
            spans.push(Span::styled(
                remaining[..link_end].to_owned(),
                base.patch(theme.link),
            ));
            remaining = &remaining[link_end..];
            continue;
        }

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
    let patterns = [
        ("`", "`", theme.code),
        ("**", "**", theme.bold),
        ("__", "__", theme.bold),
        ("*", "*", theme.italic),
        ("_", "_", theme.italic),
    ];

    let mut best: Option<(&str, &str, Style, usize, usize)> = None;

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
    Some((start_index, search_from + end_index + end.len()))
}

fn parse_link(line: &str) -> Option<usize> {
    let start = line.find('[')?;
    if start != 0 {
        return None;
    }

    let mid = line.find("](")?;
    let end = line[mid + 2..].find(')')?;
    Some(mid + 2 + end + 1)
}

fn is_rule(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.len() < 3 {
        return false;
    }

    ['-', '*', '_']
        .into_iter()
        .any(|marker| trimmed.chars().all(|ch| ch == marker))
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
        assert_eq!(parse_link("[OpenAI](https://openai.com) rest"), Some(28));
        assert_eq!(parse_link("text [OpenAI](https://openai.com)"), None);
    }
}
