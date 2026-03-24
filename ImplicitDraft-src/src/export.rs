//! Renders documents to terminal or file export formats.

use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

use anyhow::{Context, Result, anyhow};
use clap::ValueEnum;
use ratatui::{
    style::{Color, Modifier, Style},
    text::Line,
};

use crate::{code, filetype::FileType, markdown, preview, theme::Theme};

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum PrintMode {
    Auto,
    Source,
    SourceHints,
    Preview,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
#[value(rename_all = "kebab-case")]
pub enum ExportFormat {
    Html,
}

pub fn print_path(path: &Path, mode: PrintMode, theme_name: &str, pager: bool) -> Result<()> {
    let theme = Theme::load_named(theme_name)
        .with_context(|| format!("failed to load theme {theme_name}"))?;
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let lines = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect::<Vec<_>>();
    let file_type = crate::filetype::detect(path);
    let width = print_width();
    let rendered = render_for_mode(&lines, path, file_type, mode, &theme, width);
    let no_color = env::var_os("NO_COLOR").is_some();
    let output = render_lines_to_ansi(&rendered, no_color);
    if pager {
        return pipe_through_pager(&output, no_color);
    }

    let mut stdout = io::stdout().lock();
    stdout.write_all(output.as_bytes())?;
    stdout.flush()?;
    Ok(())
}

pub fn export_path(
    path: &Path,
    format: ExportFormat,
    mode: PrintMode,
    theme_name: &str,
    output_path: Option<&Path>,
) -> Result<PathBuf> {
    let theme = Theme::load_named(theme_name)
        .with_context(|| format!("failed to load theme {theme_name}"))?;
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let lines = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect::<Vec<_>>();
    let file_type = crate::filetype::detect(path);
    let width = print_width();
    let rendered = render_for_mode(&lines, path, file_type, mode, &theme, width);

    match format {
        ExportFormat::Html => {
            let output_path = resolve_html_output_path(path, output_path);
            let title = html_title(&lines, path);
            let html = render_lines_to_html_document(&rendered, &theme, &title);
            fs::write(&output_path, html)
                .with_context(|| format!("failed to write {}", output_path.display()))?;
            Ok(output_path)
        }
    }
}

fn render_for_mode(
    lines: &[String],
    path: &Path,
    file_type: FileType,
    mode: PrintMode,
    theme: &Theme,
    width: usize,
) -> Vec<Line<'static>> {
    match resolve_mode(file_type, mode) {
        PrintMode::Source => match file_type {
            FileType::Code => code::render_document(lines, theme, Some(path), file_type),
            _ => lines.iter().cloned().map(Line::raw).collect(),
        },
        PrintMode::SourceHints => match file_type {
            FileType::Code | FileType::Unknown => {
                code::render_document(lines, theme, Some(path), file_type)
            }
            _ => markdown::style_document(lines, theme),
        },
        PrintMode::Preview => match file_type {
            FileType::Code => {
                code::render_preview_document(lines, theme, Some(path), file_type, width)
            }
            _ => preview::render_document(lines, theme, width),
        },
        PrintMode::Auto => unreachable!("auto resolved before rendering"),
    }
}

fn resolve_mode(file_type: FileType, mode: PrintMode) -> PrintMode {
    match mode {
        PrintMode::Auto => match file_type {
            FileType::Markdown => PrintMode::Preview,
            FileType::Text => PrintMode::Preview,
            FileType::Code => PrintMode::Source,
            FileType::Unknown => PrintMode::Source,
        },
        explicit => explicit,
    }
}

fn print_width() -> usize {
    env::var("COLUMNS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|width| *width > 0)
        .unwrap_or(100)
}

fn render_lines_to_ansi(lines: &[Line<'static>], no_color: bool) -> String {
    let mut output = String::new();
    for line in lines {
        let line_style = line.style;
        for span in &line.spans {
            let style = line_style.patch(span.style);
            if no_color || style == Style::default() {
                output.push_str(span.content.as_ref());
            } else {
                output.push_str(&ansi_escape(style));
                output.push_str(span.content.as_ref());
                output.push_str("\x1b[0m");
            }
        }
        output.push('\n');
    }
    output
}

fn ansi_escape(style: Style) -> String {
    let mut codes = Vec::new();

    if style.add_modifier.contains(Modifier::BOLD) {
        codes.push(String::from("1"));
    }
    if style.add_modifier.contains(Modifier::DIM) {
        codes.push(String::from("2"));
    }
    if style.add_modifier.contains(Modifier::ITALIC) {
        codes.push(String::from("3"));
    }
    if style.add_modifier.contains(Modifier::UNDERLINED) {
        codes.push(String::from("4"));
    }

    if let Some(fg) = style.fg {
        codes.push(color_code(fg, false));
    }
    if let Some(bg) = style.bg {
        codes.push(color_code(bg, true));
    }

    if codes.is_empty() {
        String::from("\x1b[0m")
    } else {
        format!("\x1b[{}m", codes.join(";"))
    }
}

fn color_code(color: Color, background: bool) -> String {
    match color {
        Color::Reset => {
            if background {
                String::from("49")
            } else {
                String::from("39")
            }
        }
        Color::Black => base_color(30, background),
        Color::Red => base_color(31, background),
        Color::Green => base_color(32, background),
        Color::Yellow => base_color(33, background),
        Color::Blue => base_color(34, background),
        Color::Magenta => base_color(35, background),
        Color::Cyan => base_color(36, background),
        Color::Gray => base_color(37, background),
        Color::DarkGray => base_color(90, background),
        Color::LightRed => base_color(91, background),
        Color::LightGreen => base_color(92, background),
        Color::LightYellow => base_color(93, background),
        Color::LightBlue => base_color(94, background),
        Color::LightMagenta => base_color(95, background),
        Color::LightCyan => base_color(96, background),
        Color::White => base_color(97, background),
        Color::Indexed(index) => format!("{};5;{index}", if background { 48 } else { 38 }),
        Color::Rgb(r, g, b) => format!("{};2;{r};{g};{b}", if background { 48 } else { 38 }),
    }
}

fn base_color(code: u8, background: bool) -> String {
    let code = if background { code + 10 } else { code };
    code.to_string()
}

pub fn validate_input_path<'a>(action: &str, path: Option<&'a Path>) -> Result<&'a Path> {
    let Some(path) = path else {
        return Err(anyhow!("{action} requires a file path"));
    };
    Ok(path)
}

fn pipe_through_pager(output: &str, no_color: bool) -> Result<()> {
    let pager = pager_command_from_env(env::var("PAGER").ok().as_deref(), no_color);
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&pager)
        .stdin(Stdio::piped())
        .spawn()
        .with_context(|| format!("failed to spawn pager `{pager}`"))?;

    {
        let stdin = child.stdin.as_mut().context("failed to open pager stdin")?;
        stdin.write_all(output.as_bytes())?;
    }

    let status = child.wait()?;
    if !status.success() {
        return Err(anyhow!("pager exited with status {status}"));
    }

    Ok(())
}

fn pager_command_from_env(pager: Option<&str>, no_color: bool) -> String {
    match pager.map(str::trim).filter(|pager| !pager.is_empty()) {
        Some(pager) => pager.to_owned(),
        None if no_color => String::from("less"),
        None => String::from("less -R"),
    }
}

fn resolve_html_output_path(input_path: &Path, output_path: Option<&Path>) -> PathBuf {
    output_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| input_path.with_extension("html"))
}

fn html_title(lines: &[String], path: &Path) -> String {
    extract_frontmatter_title(lines)
        .or_else(|| {
            path.file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| String::from("implicit export"))
}

fn extract_frontmatter_title(lines: &[String]) -> Option<String> {
    if lines.first().map(|line| line.trim()) != Some("---") {
        return None;
    }

    for line in lines.iter().skip(1) {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }

        let (key, value) = trimmed.split_once(':')?;
        if key.trim() == "title" {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            if !value.is_empty() {
                return Some(value.to_owned());
            }
        }
    }

    None
}

fn render_lines_to_html_document(lines: &[Line<'static>], theme: &Theme, title: &str) -> String {
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str("<title>");
    html.push_str(&html_escape(title));
    html.push_str("</title>\n<style>\n");
    html.push_str(&html_css(theme));
    html.push_str("</style>\n</head>\n<body>\n<main class=\"document\">\n");
    html.push_str("<header class=\"export-header\"><h1>");
    html.push_str(&html_escape(title));
    html.push_str("</h1></header>\n");

    for line in lines {
        html.push_str("<div class=\"line\">");
        if line.spans.is_empty() {
            html.push_str("&nbsp;");
        } else {
            let line_style = line.style;
            let mut emitted = false;
            for span in &line.spans {
                if span.content.is_empty() {
                    continue;
                }

                emitted = true;
                let style = line_style.patch(span.style);
                let content = html_escape(span.content.as_ref());
                let css = style_to_css(style);
                if css.is_empty() {
                    html.push_str(&content);
                } else {
                    html.push_str("<span style=\"");
                    html.push_str(&css);
                    html.push_str("\">");
                    html.push_str(&content);
                    html.push_str("</span>");
                }
            }

            if !emitted {
                html.push_str("&nbsp;");
            }
        }
        html.push_str("</div>\n");
    }

    html.push_str("</main>\n</body>\n</html>\n");
    html
}

fn html_css(theme: &Theme) -> String {
    format!(
        concat!(
            ":root {{",
            "--implicit-bg: {};",
            "--implicit-fg: {};",
            "--implicit-chrome: {};",
            "--implicit-selection: {};",
            "--implicit-code-bg: {};",
            "}}\n",
            "* {{ box-sizing: border-box; }}\n",
            "body {{ margin: 0; padding: 32px; background: var(--implicit-bg); color: var(--implicit-fg); font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace; }}\n",
            ".document {{ max-width: 1100px; margin: 0 auto; border: 1px solid color-mix(in srgb, var(--implicit-chrome) 35%, transparent); border-radius: 16px; overflow: hidden; background: color-mix(in srgb, var(--implicit-bg) 94%, var(--implicit-chrome)); }}\n",
            ".export-header {{ padding: 20px 24px; border-bottom: 1px solid color-mix(in srgb, var(--implicit-chrome) 35%, transparent); }}\n",
            ".export-header h1 {{ margin: 0; font-size: 18px; color: var(--implicit-chrome); }}\n",
            ".line {{ white-space: pre-wrap; padding: 0 24px; min-height: 1.45em; }}\n",
            ".line:first-of-type {{ padding-top: 20px; }}\n",
            ".line:last-of-type {{ padding-bottom: 24px; }}\n",
            "@media print {{ body {{ padding: 0; background: white; }} .document {{ border: none; border-radius: 0; max-width: none; }} }}\n"
        ),
        color_or_default(theme.background.bg, "#111827"),
        color_or_default(base_text_style(theme).fg, "#e5e7eb"),
        color_or_default(theme.ui_chrome.fg, "#7dd3fc"),
        color_or_default(theme.selection.bg, "#374151"),
        color_or_default(theme.code.bg, "#1f2937"),
    )
}

fn base_text_style(theme: &Theme) -> Style {
    if theme.code.fg.is_some() {
        theme.code
    } else if theme.heading6.fg.is_some() {
        theme.heading6
    } else {
        Style::default().fg(Color::White)
    }
}

fn style_to_css(style: Style) -> String {
    let mut rules = Vec::new();

    if let Some(color) = style.fg.and_then(color_to_css) {
        rules.push(format!("color:{color}"));
    }
    if let Some(color) = style.bg.and_then(color_to_css) {
        rules.push(format!("background-color:{color}"));
    }
    if style.add_modifier.contains(Modifier::BOLD) {
        rules.push(String::from("font-weight:700"));
    }
    if style.add_modifier.contains(Modifier::ITALIC) {
        rules.push(String::from("font-style:italic"));
    }
    if style.add_modifier.contains(Modifier::UNDERLINED) {
        rules.push(String::from("text-decoration:underline"));
    }
    if style.add_modifier.contains(Modifier::DIM) {
        rules.push(String::from("opacity:0.78"));
    }

    rules.join(";")
}

fn color_or_default(color: Option<Color>, default: &str) -> String {
    color
        .and_then(color_to_css)
        .unwrap_or_else(|| default.to_owned())
}

fn color_to_css(color: Color) -> Option<String> {
    match color {
        Color::Reset => None,
        Color::Black => Some(String::from("#000000")),
        Color::Red => Some(String::from("#aa0000")),
        Color::Green => Some(String::from("#00aa00")),
        Color::Yellow => Some(String::from("#aa5500")),
        Color::Blue => Some(String::from("#0000aa")),
        Color::Magenta => Some(String::from("#aa00aa")),
        Color::Cyan => Some(String::from("#00aaaa")),
        Color::Gray => Some(String::from("#aaaaaa")),
        Color::DarkGray => Some(String::from("#555555")),
        Color::LightRed => Some(String::from("#ff5555")),
        Color::LightGreen => Some(String::from("#55ff55")),
        Color::LightYellow => Some(String::from("#ffff55")),
        Color::LightBlue => Some(String::from("#5555ff")),
        Color::LightMagenta => Some(String::from("#ff55ff")),
        Color::LightCyan => Some(String::from("#55ffff")),
        Color::White => Some(String::from("#ffffff")),
        Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
        Color::Indexed(index) => Some(indexed_color_to_css(index)),
    }
}

fn indexed_color_to_css(index: u8) -> String {
    match index {
        0 => String::from("#000000"),
        1 => String::from("#800000"),
        2 => String::from("#008000"),
        3 => String::from("#808000"),
        4 => String::from("#000080"),
        5 => String::from("#800080"),
        6 => String::from("#008080"),
        7 => String::from("#c0c0c0"),
        8 => String::from("#808080"),
        9 => String::from("#ff0000"),
        10 => String::from("#00ff00"),
        11 => String::from("#ffff00"),
        12 => String::from("#0000ff"),
        13 => String::from("#ff00ff"),
        14 => String::from("#00ffff"),
        15 => String::from("#ffffff"),
        value @ 16..=231 => {
            let cube = value - 16;
            let r = cube / 36;
            let g = (cube / 6) % 6;
            let b = cube % 6;
            let channel = |n: u8| if n == 0 { 0 } else { 55 + (n * 40) };
            format!("#{:02x}{:02x}{:02x}", channel(r), channel(g), channel(b))
        }
        value => {
            let gray = 8 + ((value - 232) * 10);
            format!("#{gray:02x}{gray:02x}{gray:02x}")
        }
    }
}

fn html_escape(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_color_print_is_plain_text() {
        let lines = vec![Line::styled("hello", Style::default().fg(Color::Red))];
        let rendered = render_lines_to_ansi(&lines, true);
        assert_eq!(rendered, "hello\n");
    }

    #[test]
    fn ansi_print_includes_escape_codes() {
        let lines = vec![Line::styled("hello", Style::default().fg(Color::Red))];
        let rendered = render_lines_to_ansi(&lines, false);
        assert!(rendered.contains("\x1b["));
        assert!(rendered.ends_with("\x1b[0m\n"));
    }

    #[test]
    fn auto_mode_prefers_preview_for_markdown() {
        assert_eq!(
            resolve_mode(FileType::Markdown, PrintMode::Auto),
            PrintMode::Preview
        );
        assert_eq!(
            resolve_mode(FileType::Code, PrintMode::Auto),
            PrintMode::Source
        );
    }

    #[test]
    fn pager_command_defaults_to_less_with_ansi_support() {
        assert_eq!(pager_command_from_env(None, false), "less -R");
        assert_eq!(pager_command_from_env(None, true), "less");
        assert_eq!(
            pager_command_from_env(Some("bat --paging=always"), false),
            "bat --paging=always"
        );
    }

    #[test]
    fn html_output_defaults_to_input_stem() {
        let output = resolve_html_output_path(Path::new("notes.md"), None);
        assert_eq!(output, PathBuf::from("notes.html"));
    }

    #[test]
    fn html_title_prefers_frontmatter_title() {
        let lines = vec![
            String::from("---"),
            String::from("title: Exported Note"),
            String::from("---"),
            String::from("# Heading"),
        ];
        assert_eq!(
            html_title(&lines, Path::new("fallback.md")),
            String::from("Exported Note")
        );
    }

    #[test]
    fn html_renderer_escapes_text_and_embeds_css() {
        let theme = Theme::source_hints_default();
        let lines = vec![Line::styled(
            "<hello>",
            Style::default()
                .fg(Color::LightBlue)
                .add_modifier(Modifier::BOLD),
        )];
        let html = render_lines_to_html_document(&lines, &theme, "Demo");
        assert!(html.contains("<title>Demo</title>"));
        assert!(html.contains("&lt;hello&gt;"));
        assert!(html.contains("--implicit-bg"));
        assert!(html.contains("font-weight:700"));
    }
}
