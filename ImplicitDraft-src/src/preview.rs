//! Renders markdown lines into a read-only preview view.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::theme::Theme;

pub fn render_document(lines: &[String], theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let mut code_block: Option<CodeBlock> = None;
    let mut rendered = Vec::with_capacity(lines.len());

    for line in lines {
        let trimmed = line.trim_start();
        let fence = fence_marker_for(trimmed);

        if fence.is_some() {
            match (code_block, fence) {
                (None, Some(marker)) => {
                    let block = CodeBlock::new(marker, trimmed);
                    rendered.push(render_code_border(&block, width, theme, true));
                    code_block = Some(block);
                }
                (Some(active), Some(marker)) if active.marker == marker => {
                    rendered.push(render_code_border(&active, width, theme, false));
                    code_block = None;
                }
                _ => rendered.push(Line::from(vec![Span::styled(line.clone(), theme.code)])),
            }
            continue;
        }

        if code_block.is_some() {
            rendered.push(render_code_line(line, width, theme));
            continue;
        }

        rendered.push(render_line(line, theme, width));
    }

    rendered
}

fn render_line(line: &str, theme: &Theme, width: usize) -> Line<'static> {
    if is_rule(line) {
        return Line::from(vec![Span::styled("─".repeat(width.max(3)), theme.rule)]);
    }

    if let Some(level) = heading_level(line) {
        let trimmed = line.trim_start();
        let content = trimmed[level + 1..].trim_start();
        return render_heading(content, level, width, theme);
    }

    if let Some((prefix, content)) = parse_task_item(line) {
        let mut spans = vec![Span::styled(prefix.to_owned(), theme.list_marker)];
        spans.extend(inline_preview_spans(content, theme, Style::default()));
        return Line::from(spans);
    }

    if let Some((prefix, content)) = parse_list_item(line) {
        let mut spans = vec![Span::styled(prefix, theme.list_marker)];
        spans.extend(inline_preview_spans(content, theme, Style::default()));
        return Line::from(spans);
    }

    if let Some(content) = parse_blockquote(line) {
        let mut spans = vec![Span::styled("▌ ", theme.blockquote)];
        spans.extend(inline_preview_spans(content, theme, theme.blockquote));
        return Line::from(spans);
    }

    Line::from(inline_preview_spans(line, theme, Style::default()))
}

fn render_heading(content: &str, level: usize, width: usize, theme: &Theme) -> Line<'static> {
    let indent = " ".repeat(level.saturating_sub(1));
    let mut spans = Vec::new();

    if !indent.is_empty() {
        spans.push(Span::raw(indent));
    }

    spans.extend(inline_preview_spans(content, theme, theme.heading(level)));

    let used = content.chars().count() + level.saturating_sub(1);
    let fill = width.saturating_sub(used + 2);
    if fill > 2 && level <= 2 {
        spans.push(Span::raw(" "));
        spans.push(Span::styled("─".repeat(fill), theme.rule));
    }

    Line::from(spans)
}

fn render_code_border(
    block: &CodeBlock,
    width: usize,
    theme: &Theme,
    opening: bool,
) -> Line<'static> {
    let min_width = width.max(8);
    if opening {
        let label = if block.info.is_empty() {
            " code ".to_owned()
        } else {
            format!(" code {} ", block.info)
        };
        let line = format!(
            "┌{}{}",
            label,
            "─".repeat(min_width.saturating_sub(label.chars().count() + 1))
        );
        Line::from(vec![Span::styled(line, theme.code)])
    } else {
        Line::from(vec![Span::styled(
            format!("└{}", "─".repeat(min_width.saturating_sub(1))),
            theme.code,
        )])
    }
}

fn render_code_line(line: &str, width: usize, theme: &Theme) -> Line<'static> {
    let content_width = width.saturating_sub(4);
    let visible = truncate_chars(line, content_width);
    let padding = content_width.saturating_sub(visible.chars().count());
    let mut content = String::from("│ ");
    content.push_str(&visible);
    content.push_str(&" ".repeat(padding));
    if width >= 4 {
        content.push_str(" │");
    }
    Line::from(vec![Span::styled(content, theme.code)])
}

fn inline_preview_spans(line: &str, theme: &Theme, base: Style) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut remaining = line;

    while !remaining.is_empty() {
        if let Some(segment) = next_inline_segment(remaining, theme) {
            if segment.start > 0 {
                spans.push(Span::styled(remaining[..segment.start].to_owned(), base));
            }

            match segment.kind {
                InlineKind::Styled(style) => spans.push(Span::styled(
                    remaining[segment.content_start..segment.content_end].to_owned(),
                    base.patch(style),
                )),
                InlineKind::Link => {
                    spans.push(Span::styled(
                        remaining[segment.content_start..segment.content_end].to_owned(),
                        base.patch(theme.link),
                    ));

                    let url = &remaining[segment.meta_start..segment.meta_end];
                    spans.push(Span::styled(format!(" [{url}]"), base.patch(theme.rule)));
                }
            }

            remaining = &remaining[segment.end..];
            continue;
        }

        spans.push(Span::styled(remaining.to_owned(), base));
        break;
    }

    spans
}

fn next_inline_segment(line: &str, theme: &Theme) -> Option<InlineSegment> {
    let mut best = parse_link(line);

    for (delimiter, style) in [
        ("`", InlineKind::Styled(theme.code)),
        ("**", InlineKind::Styled(theme.bold)),
        ("__", InlineKind::Styled(theme.bold)),
        ("*", InlineKind::Styled(theme.italic)),
        ("_", InlineKind::Styled(theme.italic)),
    ] {
        if let Some(candidate) = find_wrapped(line, delimiter, style) {
            let replace = match best {
                Some(current) => {
                    candidate.start < current.start
                        || (candidate.start == current.start && candidate.end < current.end)
                }
                None => true,
            };

            if replace {
                best = Some(candidate);
            }
        }
    }

    best
}

fn find_wrapped(line: &str, delimiter: &str, kind: InlineKind) -> Option<InlineSegment> {
    let start = line.find(delimiter)?;
    let content_start = start + delimiter.len();
    let rest = &line[content_start..];
    let closing = rest.find(delimiter)?;

    if closing == 0 {
        return None;
    }

    let content_end = content_start + closing;
    let end = content_end + delimiter.len();
    Some(InlineSegment {
        start,
        content_start,
        content_end,
        meta_start: content_end,
        meta_end: content_end,
        end,
        kind,
    })
}

fn parse_link(line: &str) -> Option<InlineSegment> {
    let start = line.find('[')?;
    let label_end = start + line[start..].find("](")?;
    let url_start = label_end + 2;
    let url_end = url_start + line[url_start..].find(')')?;

    if label_end == start + 1 {
        return None;
    }

    Some(InlineSegment {
        start,
        content_start: start + 1,
        content_end: label_end,
        meta_start: url_start,
        meta_end: url_end,
        end: url_end + 1,
        kind: InlineKind::Link,
    })
}

fn parse_task_item(line: &str) -> Option<(&'static str, &str)> {
    let trimmed = line.trim_start();
    let offset = line.len() - trimmed.len();

    for prefix in ["- [ ] ", "* [ ] ", "+ [ ] "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some((
                "☐ ",
                &line[offset + prefix.len()..offset + prefix.len() + rest.len()],
            ));
        }
    }

    for prefix in ["- [x] ", "- [X] ", "* [x] ", "* [X] ", "+ [x] ", "+ [X] "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some((
                "☑ ",
                &line[offset + prefix.len()..offset + prefix.len() + rest.len()],
            ));
        }
    }

    None
}

fn parse_list_item(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim_start();
    let offset = line.len() - trimmed.len();

    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(bullet) {
            let content_start = offset + bullet.len();
            return Some((
                "• ".to_owned(),
                &line[content_start..content_start + rest.len()],
            ));
        }
    }

    let digits = trimmed.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits > 0 && trimmed[digits..].starts_with(". ") {
        let marker_end = offset + digits + 2;
        return Some((line[offset..marker_end].to_owned(), &line[marker_end..]));
    }

    None
}

fn parse_blockquote(line: &str) -> Option<&str> {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix('>')?;
    Some(rest.trim_start())
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

#[derive(Clone, Copy)]
enum InlineKind {
    Styled(Style),
    Link,
}

#[derive(Clone, Copy)]
struct InlineSegment {
    start: usize,
    content_start: usize,
    content_end: usize,
    meta_start: usize,
    meta_end: usize,
    end: usize,
    kind: InlineKind,
}

#[derive(Clone, Copy)]
struct CodeBlock<'a> {
    marker: &'a str,
    info: &'a str,
}

impl<'a> CodeBlock<'a> {
    fn new(marker: &'a str, line: &'a str) -> Self {
        Self {
            marker,
            info: line[marker.len()..].trim(),
        }
    }
}

fn truncate_chars(line: &str, max_chars: usize) -> String {
    line.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_bullets_and_tasks() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[
                String::from("- item"),
                String::from("- [ ] open"),
                String::from("- [x] done"),
            ],
            &theme,
            20,
        );

        assert_eq!(rendered[0].spans[0].content.as_ref(), "• ");
        assert_eq!(rendered[1].spans[0].content.as_ref(), "☐ ");
        assert_eq!(rendered[2].spans[0].content.as_ref(), "☑ ");
    }

    #[test]
    fn hides_heading_markup_and_shows_rule() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(&[String::from("# Title"), String::from("---")], &theme, 8);

        assert_eq!(rendered[0].spans[0].content.as_ref(), "Title");
        assert_eq!(rendered[1].spans[0].content.as_ref(), "────────");
    }

    #[test]
    fn renders_links_without_markdown_syntax() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(&[String::from("[docs](https://example.com)")], &theme, 24);

        assert_eq!(rendered[0].spans.len(), 2);
        assert_eq!(rendered[0].spans[0].content.as_ref(), "docs");
        assert_eq!(
            rendered[0].spans[1].content.as_ref(),
            " [https://example.com]"
        );
    }

    #[test]
    fn renders_code_fences_as_preview_blocks() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[
                String::from("```rust"),
                String::from("fn main() {}"),
                String::from("```"),
            ],
            &theme,
            24,
        );

        assert!(rendered[0].spans[0].content.starts_with("┌ code rust "));
        assert_eq!(
            rendered[1].spans[0].content.as_ref(),
            "│ fn main() {}         │"
        );
        assert_eq!(
            rendered[2].spans[0].content.as_ref(),
            "└───────────────────────"
        );
    }

    #[test]
    fn decorates_primary_headings_with_rule_fill() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(&[String::from("# Title")], &theme, 12);

        assert_eq!(rendered[0].spans.len(), 3);
        assert_eq!(rendered[0].spans[0].content.as_ref(), "Title");
        assert_eq!(rendered[0].spans[1].content.as_ref(), " ");
        assert_eq!(rendered[0].spans[2].content.as_ref(), "─────");
    }
}
