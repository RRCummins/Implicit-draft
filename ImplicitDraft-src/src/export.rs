//! Renders documents to printable ANSI or plain-text terminal output.

use std::{
    env, fs,
    io::{self, Write},
    path::Path,
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

pub fn validate_print_path(path: Option<&Path>) -> Result<&Path> {
    let Some(path) = path else {
        return Err(anyhow!("--print requires a file path"));
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
}
