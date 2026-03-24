//! Renders markdown lines into a read-only preview view.

use ratatui::{
    style::Style,
    text::{Line, Span},
};

use crate::{code, theme::Theme};

pub fn render_document(lines: &[String], theme: &Theme, width: usize) -> Vec<Line<'static>> {
    let mut rendered = Vec::with_capacity(lines.len());
    let mut index = 0usize;

    while index < lines.len() {
        let line = &lines[index];

        if let Some((block, next_index)) = parse_code_block(lines, index) {
            rendered.extend(render_code_block(&block, width, theme));
            index = next_index;
            continue;
        }

        if let Some((table, next_index)) = parse_table_block(lines, index) {
            rendered.extend(render_table_block(&table, width, theme));
            index = next_index;
            continue;
        }

        rendered.push(render_line(line, theme, width));
        index += 1;
    }

    rendered
}

fn render_line(line: &str, theme: &Theme, width: usize) -> Line<'static> {
    if is_conflict_marker(line) {
        return Line::from(vec![Span::styled(line.to_owned(), theme.conflict_marker)]);
    }

    if is_rule(line) {
        return Line::from(vec![Span::styled("─".repeat(width.max(3)), theme.rule)]);
    }

    if let Some(level) = heading_level(line) {
        let trimmed = line.trim_start();
        let content = trimmed[level + 1..].trim_start();
        return render_heading(content, level, width, theme);
    }

    if let Some((prefix, content)) = parse_task_item(line) {
        let mut spans = vec![Span::styled(prefix, theme.list_marker)];
        spans.extend(inline_preview_spans(content, theme, Style::default()));
        return Line::from(spans);
    }

    if let Some((prefix, content)) = parse_list_item(line) {
        let mut spans = vec![Span::styled(prefix, theme.list_marker)];
        spans.extend(inline_preview_spans(content, theme, Style::default()));
        return Line::from(spans);
    }

    if let Some((prefix, content)) = parse_blockquote(line) {
        let mut spans = vec![Span::styled(prefix, theme.blockquote)];
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
        let label = block.info.as_deref().unwrap_or("code");
        let label = format!("─ {label} ");
        let line = format!(
            "┌{}{}",
            label,
            "─".repeat(min_width.saturating_sub(label.chars().count() + 1))
        );
        Line::from(vec![Span::styled(line, theme.ui_chrome)])
    } else {
        Line::from(vec![Span::styled(
            format!("└{}", "─".repeat(min_width.saturating_sub(1))),
            theme.ui_chrome,
        )])
    }
}

fn render_code_block(block: &CodeBlock, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let mut rendered = Vec::with_capacity(block.lines.len() + 2);
    rendered.push(render_code_border(block, width, theme, true));

    let content_width = width.saturating_sub(4);
    for line in code::render_fenced_preview_document(
        &block.lines,
        theme,
        block.info.as_deref(),
        content_width,
    ) {
        rendered.push(render_preview_code_line(line, content_width, width, theme));
    }

    rendered.push(render_code_border(block, width, theme, false));
    rendered
}

fn render_preview_code_line(
    line: Line<'static>,
    content_width: usize,
    width: usize,
    theme: &Theme,
) -> Line<'static> {
    let content_chars = line
        .spans
        .iter()
        .map(|span| span.content.chars().count())
        .sum::<usize>();
    let padding = content_width.saturating_sub(content_chars);
    let mut spans = vec![Span::styled(String::from("│ "), theme.ui_chrome)];
    spans.extend(line.spans);
    if padding > 0 {
        spans.push(Span::styled(" ".repeat(padding), theme.background));
    }
    if width >= 4 {
        spans.push(Span::styled(String::from(" │"), theme.ui_chrome));
    }
    Line::from(spans)
}

fn parse_code_block(lines: &[String], start_index: usize) -> Option<(CodeBlock, usize)> {
    let trimmed = lines.get(start_index)?.trim_start();
    let (marker, info) = parse_fence_start(trimmed)?;
    let mut block = CodeBlock::new(info);
    let mut index = start_index + 1;

    while index < lines.len() {
        let trimmed = lines[index].trim_start();
        if trimmed.starts_with(marker) {
            return Some((block, index + 1));
        }
        block.lines.push(lines[index].clone());
        index += 1;
    }

    Some((block, lines.len()))
}

fn parse_table_block(lines: &[String], start_index: usize) -> Option<(TableBlock, usize)> {
    let header = parse_table_row(lines.get(start_index)?)?;
    let separator = parse_table_separator(lines.get(start_index + 1)?)?;
    if header.len() != separator.len() || header.len() < 2 {
        return None;
    }

    let mut rows = Vec::new();
    let mut index = start_index + 2;
    while index < lines.len() {
        let Some(row) = parse_table_row(&lines[index]) else {
            break;
        };
        rows.push(row);
        index += 1;
    }

    Some((TableBlock { header, rows }, index))
}

fn render_table_block(table: &TableBlock, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let widths = fit_table_widths(table, width.max(8));
    let mut rendered = Vec::with_capacity(table.rows.len() + 4);

    rendered.push(render_table_border(&widths, '┌', '┬', '┐', theme));
    rendered.push(render_table_row(&table.header, &widths, theme, true));
    rendered.push(render_table_border(&widths, '├', '┼', '┤', theme));
    for row in &table.rows {
        rendered.push(render_table_row(row, &widths, theme, false));
    }
    rendered.push(render_table_border(&widths, '└', '┴', '┘', theme));

    rendered
}

fn render_table_border(
    widths: &[usize],
    left: char,
    middle: char,
    right: char,
    theme: &Theme,
) -> Line<'static> {
    let mut line = String::new();
    line.push(left);
    for (index, width) in widths.iter().enumerate() {
        if index > 0 {
            line.push(middle);
        }
        line.push_str(&"─".repeat(*width + 2));
    }
    line.push(right);
    Line::from(vec![Span::styled(line, theme.ui_chrome)])
}

fn render_table_row(
    cells: &[String],
    widths: &[usize],
    theme: &Theme,
    header: bool,
) -> Line<'static> {
    let mut spans = vec![Span::styled("│".to_owned(), theme.ui_chrome)];

    for (index, width) in widths.iter().enumerate() {
        let cell = cells.get(index).map(String::as_str).unwrap_or("");
        let fitted = fit_cell_content(cell, *width);
        let style = if header {
            theme.ui_chrome.patch(theme.bold)
        } else {
            theme.background
        };
        let padding = width.saturating_sub(fitted.chars().count());

        spans.push(Span::styled(" ".to_owned(), theme.background));
        spans.push(Span::styled(fitted, style));
        if padding > 0 {
            spans.push(Span::styled(" ".repeat(padding), style));
        }
        spans.push(Span::styled(" ".to_owned(), theme.background));
        spans.push(Span::styled("│".to_owned(), theme.ui_chrome));
    }

    Line::from(spans)
}

fn fit_table_widths(table: &TableBlock, width: usize) -> Vec<usize> {
    let column_count = table.header.len();
    let mut widths = vec![1; column_count];

    for (index, cell) in table.header.iter().enumerate() {
        widths[index] = widths[index].max(cell.chars().count());
    }
    for row in &table.rows {
        for (index, cell) in row.iter().enumerate().take(column_count) {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }

    let mut total_width = table_render_width(&widths);
    while total_width > width {
        let Some((widest_index, widest_width)) = widths
            .iter()
            .copied()
            .enumerate()
            .max_by_key(|(_, width)| *width)
        else {
            break;
        };
        if widest_width <= 3 {
            break;
        }
        widths[widest_index] -= 1;
        total_width -= 1;
    }

    widths
}

fn table_render_width(widths: &[usize]) -> usize {
    widths.iter().sum::<usize>() + (widths.len() * 3) + 1
}

fn fit_cell_content(cell: &str, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    let count = cell.chars().count();
    if count <= width {
        return cell.to_owned();
    }

    if width == 1 {
        return "…".to_owned();
    }

    let head = cell.chars().take(width - 1).collect::<String>();
    format!("{head}…")
}

fn parse_table_row(line: &str) -> Option<Vec<String>> {
    let trimmed = line.trim();
    if trimmed.is_empty() || !trimmed.contains('|') {
        return None;
    }

    let inner = trimmed.trim_matches('|');
    if inner.is_empty() {
        return None;
    }

    let cells = inner
        .split('|')
        .map(|cell| cell.trim().to_owned())
        .collect::<Vec<_>>();
    if cells.len() < 2 {
        return None;
    }

    Some(cells)
}

fn parse_table_separator(line: &str) -> Option<Vec<()>> {
    let cells = parse_table_row(line)?;
    if cells.iter().any(|cell| {
        let compact = cell.replace(' ', "");
        compact.is_empty()
            || !compact.contains('-')
            || !compact.chars().all(|ch| matches!(ch, '-' | ':'))
    }) {
        return None;
    }

    Some(vec![(); cells.len()])
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

fn parse_task_item(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim_start();
    let offset = line.len() - trimmed.len();
    let indent = " ".repeat(offset);

    for prefix in ["- [ ] ", "* [ ] ", "+ [ ] "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some((
                format!("{indent}☐ "),
                &line[offset + prefix.len()..offset + prefix.len() + rest.len()],
            ));
        }
    }

    for prefix in ["- [x] ", "- [X] ", "* [x] ", "* [X] ", "+ [x] ", "+ [X] "] {
        if let Some(rest) = trimmed.strip_prefix(prefix) {
            return Some((
                format!("{indent}☑ "),
                &line[offset + prefix.len()..offset + prefix.len() + rest.len()],
            ));
        }
    }

    None
}

fn parse_list_item(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim_start();
    let offset = line.len() - trimmed.len();
    let indent = " ".repeat(offset);

    for bullet in ["- ", "* ", "+ "] {
        if let Some(rest) = trimmed.strip_prefix(bullet) {
            let content_start = offset + bullet.len();
            return Some((
                format!("{indent}• "),
                &line[content_start..content_start + rest.len()],
            ));
        }
    }

    let digits = trimmed.chars().take_while(|ch| ch.is_ascii_digit()).count();
    if digits > 0 && trimmed[digits..].starts_with(". ") {
        let marker_end = offset + digits + 2;
        return Some((
            format!("{indent}{}", &line[offset..marker_end]),
            &line[marker_end..],
        ));
    }

    None
}

fn parse_blockquote(line: &str) -> Option<(String, &str)> {
    let trimmed = line.trim_start();
    let offset = line.len() - trimmed.len();
    let mut rest = trimmed;
    let mut depth = 0usize;

    while let Some(next) = rest.strip_prefix('>') {
        depth += 1;
        rest = next.trim_start();
    }

    if depth == 0 {
        return None;
    }

    let prefix = format!("{}{}", " ".repeat(offset), "▌ ".repeat(depth));
    Some((prefix, rest))
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

fn is_conflict_marker(line: &str) -> bool {
    let trimmed = line.trim_start();
    trimmed.starts_with("<<<<<<<")
        || trimmed.starts_with("=======")
        || trimmed.starts_with(">>>>>>>")
        || trimmed.starts_with("|||||||")
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

#[derive(Clone)]
struct CodeBlock {
    info: Option<String>,
    lines: Vec<String>,
}

impl CodeBlock {
    fn new(info: Option<&str>) -> Self {
        Self {
            info: info.map(str::to_owned),
            lines: Vec::new(),
        }
    }
}

struct TableBlock {
    header: Vec<String>,
    rows: Vec<Vec<String>>,
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
    fn preserves_nested_list_indentation() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[
                String::from("  - child"),
                String::from("    - [ ] task"),
                String::from("  2. item"),
            ],
            &theme,
            24,
        );

        assert_eq!(rendered[0].spans[0].content.as_ref(), "  • ");
        assert_eq!(rendered[1].spans[0].content.as_ref(), "    ☐ ");
        assert_eq!(rendered[2].spans[0].content.as_ref(), "  2. ");
    }

    #[test]
    fn hides_heading_markup_and_shows_rule() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(&[String::from("# Title"), String::from("---")], &theme, 8);

        assert_eq!(rendered[0].spans[0].content.as_ref(), "Title");
        assert_eq!(rendered[1].spans[0].content.as_ref(), "────────");
    }

    #[test]
    fn preserves_nested_blockquote_depth() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(&[String::from("  > > quoted")], &theme, 20);

        assert_eq!(rendered[0].spans[0].content.as_ref(), "  ▌ ▌ ");
        assert_eq!(rendered[0].spans[1].content.as_ref(), "quoted");
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

        assert!(rendered[0].spans[0].content.starts_with("┌─ rust "));
        assert_eq!(rendered[1].spans[0].content.as_ref(), "│ ");
        assert!(
            rendered[1]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "fn" && span.style == theme.code_keyword)
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

    #[test]
    fn preview_code_blocks_use_fence_language_highlighting() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[
                String::from("```python"),
                String::from("def greet(name):"),
                String::from("```"),
            ],
            &theme,
            28,
        );

        assert!(
            rendered[1]
                .spans
                .iter()
                .any(|span| span.content.as_ref() == "def" && span.style == theme.code_keyword)
        );
    }

    #[test]
    fn renders_markdown_tables_as_aligned_blocks() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[
                String::from("| API | Purpose |"),
                String::from("| --- | --- |"),
                String::from("| auth | Authenticate session |"),
                String::from("| ping | Connectivity test |"),
            ],
            &theme,
            40,
        );

        assert_eq!(
            rendered[0].spans[0].content.as_ref(),
            "┌──────┬──────────────────────┐"
        );
        assert_eq!(rendered[1].spans[0].content.as_ref(), "│");
        assert_eq!(rendered[1].spans[2].content.as_ref(), "API");
        assert_eq!(rendered[1].spans[7].content.as_ref(), "Purpose");
        assert_eq!(
            rendered[2].spans[0].content.as_ref(),
            "├──────┼──────────────────────┤"
        );
        assert_eq!(rendered[3].spans[2].content.as_ref(), "auth");
        assert_eq!(rendered[4].spans[2].content.as_ref(), "ping");
    }

    #[test]
    fn compacts_code_block_chrome() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(
            &[
                String::from("```json"),
                String::from("{\"type\": \"auth\"}"),
                String::from("```"),
            ],
            &theme,
            28,
        );

        assert!(rendered[0].spans[0].content.starts_with("┌─ json "));
        assert_eq!(rendered[0].spans[0].style, theme.ui_chrome);
        assert_eq!(rendered[1].spans[0].style, theme.ui_chrome);
        assert_eq!(
            rendered[1].spans.last().expect("right border").style,
            theme.ui_chrome
        );
    }

    #[test]
    fn styles_conflict_marker_lines() {
        let theme = Theme::source_hints_default();
        let rendered = render_document(&[String::from("<<<<<<< theirs")], &theme, 24);

        assert_eq!(rendered[0].spans[0].style, theme.conflict_marker);
    }
}
