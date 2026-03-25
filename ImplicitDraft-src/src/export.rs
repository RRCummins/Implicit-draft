//! Renders documents to terminal or file export formats.

use std::{
    env, fs,
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
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
    Pdf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

pub fn parse_line_range(value: &str) -> std::result::Result<LineRange, String> {
    let trimmed = value.trim();
    let (start, end) = match trimmed.split_once('-') {
        Some((start, end)) => (start, end),
        None => (trimmed, trimmed),
    };

    let start = start
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("invalid line range `{value}`"))?;
    let end = end
        .trim()
        .parse::<usize>()
        .map_err(|_| format!("invalid line range `{value}`"))?;

    if start == 0 || end == 0 || end < start {
        return Err(format!("invalid line range `{value}`"));
    }

    Ok(LineRange { start, end })
}

pub fn print_path(
    path: &Path,
    mode: PrintMode,
    theme_name: &str,
    pager: bool,
    line_range: Option<LineRange>,
) -> Result<()> {
    let theme = Theme::load_named(theme_name)
        .with_context(|| format!("failed to load theme {theme_name}"))?;
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let lines = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect::<Vec<_>>();
    let lines = slice_lines(&lines, line_range)?;
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
    line_range: Option<LineRange>,
) -> Result<PathBuf> {
    let theme = Theme::load_named(theme_name)
        .with_context(|| format!("failed to load theme {theme_name}"))?;
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let lines = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect::<Vec<_>>();
    let lines = slice_lines(&lines, line_range)?;
    let file_type = crate::filetype::detect(path);
    let width = print_width();
    let rendered = render_for_mode(&lines, path, file_type, mode, &theme, width);
    let title = html_title(&lines, path);
    let html = render_lines_to_html_document(
        &rendered,
        &theme,
        &title,
        HtmlDocumentOptions {
            paper_size: default_paper_size(),
            source_label: source_label(path, line_range),
        },
    );

    match format {
        ExportFormat::Html => {
            let output_path = resolve_html_output_path(path, output_path);
            fs::write(&output_path, html)
                .with_context(|| format!("failed to write {}", output_path.display()))?;
            Ok(output_path)
        }
        ExportFormat::Pdf => export_pdf_document(path, output_path, &html),
    }
}

pub fn snapshot_path(
    path: &Path,
    mode: PrintMode,
    theme_name: &str,
    output_path: Option<&Path>,
    line_range: Option<LineRange>,
    line_numbers: bool,
) -> Result<PathBuf> {
    let theme = Theme::load_named(theme_name)
        .with_context(|| format!("failed to load theme {theme_name}"))?;
    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let lines = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect::<Vec<_>>();
    let lines = slice_lines(&lines, line_range)?;
    let file_type = crate::filetype::detect(path);
    let output = resolve_snapshot_output_path(path, output_path)?;
    let snapshot = prepare_snapshot_render(
        &lines,
        path,
        file_type,
        mode,
        &theme,
        line_range,
        line_numbers,
    );
    let svg = render_lines_to_svg_document(
        &snapshot.rendered,
        &theme,
        SnapshotOptions {
            title: snapshot.title,
            line_numbers: snapshot.line_numbers,
        },
    );
    match output.format {
        SnapshotOutputFormat::Svg => {
            fs::write(&output.path, svg)
                .with_context(|| format!("failed to write {}", output.path.display()))?;
            Ok(output.path)
        }
        SnapshotOutputFormat::Png => export_png_snapshot(&svg, &output.path),
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

#[derive(Clone, Debug)]
struct SnapshotRender {
    rendered: Vec<Line<'static>>,
    title: String,
    line_numbers: bool,
}

fn prepare_snapshot_render(
    lines: &[String],
    path: &Path,
    file_type: FileType,
    mode: PrintMode,
    theme: &Theme,
    line_range: Option<LineRange>,
    line_numbers: bool,
) -> SnapshotRender {
    let base_title = source_label(path, line_range);
    if matches!(file_type, FileType::Markdown | FileType::Text)
        && let Some(fenced) = extract_fenced_code_block(lines)
    {
        let rendered = code::render_fenced_document(&fenced.lines, theme, fenced.info.as_deref());
        let title = fenced
            .info
            .as_deref()
            .filter(|info| !info.is_empty())
            .map(|info| format!("{base_title} · {info}"))
            .unwrap_or_else(|| format!("{base_title} · code"));
        return SnapshotRender {
            rendered,
            title,
            line_numbers,
        };
    }

    let resolved_mode = resolve_mode(file_type, mode);
    let rendered = render_for_mode(lines, path, file_type, mode, theme, print_width());
    let line_numbers =
        line_numbers && matches!(resolved_mode, PrintMode::Source | PrintMode::SourceHints);
    SnapshotRender {
        rendered,
        title: base_title,
        line_numbers,
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

fn slice_lines(lines: &[String], range: Option<LineRange>) -> Result<Vec<String>> {
    let Some(range) = range else {
        return Ok(lines.to_vec());
    };

    if range.start > lines.len() {
        return Err(anyhow!(
            "line range {}-{} starts past the end of the document ({})",
            range.start,
            range.end,
            lines.len()
        ));
    }

    let start = range.start.saturating_sub(1);
    let end = range.end.min(lines.len());
    Ok(lines[start..end].to_vec())
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

fn resolve_pdf_output_path(input_path: &Path, output_path: Option<&Path>) -> PathBuf {
    output_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| input_path.with_extension("pdf"))
}

fn resolve_pdf_fallback_path(pdf_output_path: &Path) -> PathBuf {
    pdf_output_path.with_extension("html")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SnapshotOutputFormat {
    Svg,
    Png,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SnapshotOutputTarget {
    path: PathBuf,
    format: SnapshotOutputFormat,
}

fn resolve_snapshot_output_path(
    input_path: &Path,
    output_path: Option<&Path>,
) -> Result<SnapshotOutputTarget> {
    let output_path = output_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| input_path.with_extension("svg"));
    match output_path.extension().and_then(|ext| ext.to_str()) {
        None | Some("svg") => Ok(SnapshotOutputTarget {
            path: output_path,
            format: SnapshotOutputFormat::Svg,
        }),
        Some("png") => Ok(SnapshotOutputTarget {
            path: output_path,
            format: SnapshotOutputFormat::Png,
        }),
        Some(_) => Err(anyhow!("snapshot output supports only .svg or .png paths")),
    }
}

fn source_label(path: &Path, line_range: Option<LineRange>) -> String {
    match line_range {
        Some(range) => format!("{} [{}-{}]", path.display(), range.start, range.end),
        None => path.display().to_string(),
    }
}

fn export_png_snapshot(svg: &str, png_path: &Path) -> Result<PathBuf> {
    let temp_svg = temp_snapshot_svg_path(png_path);
    fs::write(&temp_svg, svg).with_context(|| format!("failed to write {}", temp_svg.display()))?;

    let convert_result = convert_svg_to_png(&temp_svg, png_path);
    let cleanup_result = fs::remove_file(&temp_svg);
    if let Err(error) = cleanup_result {
        eprintln!(
            "implicit: warning: failed to clean up temporary snapshot {}: {error}",
            temp_svg.display()
        );
    }

    convert_result?;
    Ok(png_path.to_path_buf())
}

fn temp_snapshot_svg_path(output_path: &Path) -> PathBuf {
    let stem = output_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("implicit-snapshot");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    env::temp_dir().join(format!("{stem}-{unique}.implicit-snapshot.svg"))
}

fn convert_svg_to_png(svg_path: &Path, png_path: &Path) -> Result<()> {
    let temp_dir = temp_snapshot_output_dir(png_path);
    fs::create_dir_all(&temp_dir)
        .with_context(|| format!("failed to create {}", temp_dir.display()))?;

    let status = Command::new("qlmanage")
        .arg("-t")
        .arg("-s")
        .arg("2000")
        .arg("-o")
        .arg(&temp_dir)
        .arg(svg_path)
        .status()
        .context("failed to launch qlmanage for PNG snapshot conversion")?;

    if !status.success() {
        return Err(anyhow!("qlmanage exited with status {status}"));
    }

    let generated_name = format!(
        "{}.png",
        svg_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("snapshot.svg")
    );
    let generated_path = temp_dir.join(generated_name);
    fs::copy(&generated_path, png_path).with_context(|| {
        format!(
            "failed to copy generated PNG {} to {}",
            generated_path.display(),
            png_path.display()
        )
    })?;
    fs::remove_file(&generated_path).ok();
    fs::remove_dir_all(&temp_dir).ok();

    Ok(())
}

fn temp_snapshot_output_dir(output_path: &Path) -> PathBuf {
    let stem = output_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("implicit-snapshot");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    env::temp_dir().join(format!("{stem}-{unique}.implicit-png"))
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FencedCodeBlock {
    info: Option<String>,
    lines: Vec<String>,
}

fn extract_fenced_code_block(lines: &[String]) -> Option<FencedCodeBlock> {
    let start = lines.iter().position(|line| !line.trim().is_empty())?;
    let end = lines.iter().rposition(|line| !line.trim().is_empty())?;
    if start >= end {
        return None;
    }

    let opening = lines[start].trim();
    let (marker, info) = if let Some(rest) = opening.strip_prefix("```") {
        ("```", rest.trim())
    } else if let Some(rest) = opening.strip_prefix("~~~") {
        ("~~~", rest.trim())
    } else {
        return None;
    };

    let closing = lines[end].trim();
    if !closing.starts_with(marker) {
        return None;
    }

    Some(FencedCodeBlock {
        info: (!info.is_empty()).then(|| info.to_owned()),
        lines: lines[start + 1..end].to_vec(),
    })
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

#[derive(Clone, Debug)]
struct HtmlDocumentOptions {
    paper_size: &'static str,
    source_label: String,
}

fn render_lines_to_html_document(
    lines: &[Line<'static>],
    theme: &Theme,
    title: &str,
    options: HtmlDocumentOptions,
) -> String {
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n");
    html.push_str("<title>");
    html.push_str(&html_escape(title));
    html.push_str("</title>\n<style>\n");
    html.push_str(&html_css(theme, options.paper_size));
    html.push_str("</style>\n</head>\n<body>\n<main class=\"document\">\n");
    html.push_str("<header class=\"export-header\"><h1>");
    html.push_str(&html_escape(title));
    html.push_str("</h1><p class=\"export-meta\">");
    html.push_str(&html_escape(&options.source_label));
    html.push_str("</p></header>\n");

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

#[derive(Clone, Debug)]
struct SnapshotOptions {
    title: String,
    line_numbers: bool,
}

fn render_lines_to_svg_document(
    lines: &[Line<'static>],
    theme: &Theme,
    options: SnapshotOptions,
) -> String {
    let char_width = 8.4_f32;
    let font_size = 16.0_f32;
    let line_height = 24.0_f32;
    let outer_padding = 24.0_f32;
    let frame_height = 38.0_f32;
    let content_padding_x = 24.0_f32;
    let content_padding_y = 22.0_f32;

    let base_fg = color_or_default(base_text_style(theme).fg, "#e5e7eb");
    let background = color_or_default(theme.code.bg.or(theme.background.bg), "#111827");
    let frame_fill = color_or_default(theme.background.bg.or(theme.code.bg), "#111827");
    let border = color_or_default(theme.ui_chrome.fg, "#7dd3fc");
    let line_number_color = color_or_default(theme.rule.fg.or(theme.ui_chrome.fg), "#6b7280");
    let gutter_fill = color_or_default(theme.background.bg, "#111827");
    let title = abbreviate_middle(&options.title, 52);

    let max_line_len = lines
        .iter()
        .map(plain_line_width)
        .max()
        .unwrap_or(0)
        .max(title.chars().count().min(52));
    let gutter_digits = lines.len().max(1).to_string().len();
    let gutter_width = if options.line_numbers {
        (gutter_digits as f32 * char_width) + 18.0
    } else {
        0.0
    };

    let content_width =
        (max_line_len as f32 * char_width) + gutter_width + (content_padding_x * 2.0);
    let content_height =
        (lines.len().max(1) as f32 * line_height) + frame_height + (content_padding_y * 2.0);
    let width = (outer_padding * 2.0) + content_width;
    let height = (outer_padding * 2.0) + content_height;
    let text_start_x = outer_padding + content_padding_x + gutter_width;
    let line_number_x = outer_padding + content_padding_x;
    let first_line_y = outer_padding + frame_height + content_padding_y;

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width:.0}\" height=\"{height:.0}\" viewBox=\"0 0 {width:.0} {height:.0}\" fill=\"none\">"
    ));
    svg.push_str(
        "<defs>\
         <linearGradient id=\"frameGradient\" x1=\"0\" y1=\"0\" x2=\"1\" y2=\"1\">\
         <stop offset=\"0%\" stop-color=\"#ffffff\" stop-opacity=\"0.05\"/>\
         <stop offset=\"100%\" stop-color=\"#ffffff\" stop-opacity=\"0.01\"/>\
         </linearGradient>\
         <filter id=\"shadow\" x=\"-20%\" y=\"-20%\" width=\"140%\" height=\"140%\">\
         <feDropShadow dx=\"0\" dy=\"12\" stdDeviation=\"18\" flood-opacity=\"0.28\"/>\
         </filter>\
         </defs>",
    );
    svg.push_str(&format!(
        "<rect width=\"{width:.0}\" height=\"{height:.0}\" rx=\"28\" fill=\"{}\" fill-opacity=\"0.18\"/>",
        background
    ));
    svg.push_str(&format!(
        "<rect x=\"{outer_padding}\" y=\"{outer_padding}\" width=\"{content_width}\" height=\"{content_height}\" rx=\"18\" fill=\"{background}\" filter=\"url(#shadow)\"/>"
    ));
    svg.push_str(&format!(
        "<rect x=\"{outer_padding}\" y=\"{outer_padding}\" width=\"{content_width}\" height=\"{frame_height}\" rx=\"18\" fill=\"{frame_fill}\" stroke=\"{border}\" stroke-opacity=\"0.35\"/>"
    ));
    svg.push_str(&format!(
        "<rect x=\"{outer_padding}\" y=\"{}\" width=\"{content_width}\" height=\"{}\" fill=\"{background}\"/>",
        outer_padding + frame_height - 18.0,
        content_height - frame_height + 18.0,
    ));
    svg.push_str(&format!(
        "<rect x=\"{outer_padding}\" y=\"{outer_padding}\" width=\"{content_width}\" height=\"{content_height}\" rx=\"18\" fill=\"url(#frameGradient)\" stroke=\"{border}\" stroke-opacity=\"0.18\"/>"
    ));
    svg.push_str(&format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"6\" fill=\"#ff5f57\"/><circle cx=\"{}\" cy=\"{}\" r=\"6\" fill=\"#febc2e\"/><circle cx=\"{}\" cy=\"{}\" r=\"6\" fill=\"#28c840\"/>",
        outer_padding + 20.0,
        outer_padding + 17.0,
        outer_padding + 38.0,
        outer_padding + 17.0,
        outer_padding + 56.0,
        outer_padding + 17.0
    ));
    svg.push_str(&format!(
        "<text x=\"{}\" y=\"{}\" fill=\"{}\" fill-opacity=\"0.9\" font-family=\"SFMono-Regular, Menlo, Consolas, monospace\" font-size=\"13\">{}</text>",
        outer_padding + 78.0,
        outer_padding + 23.0,
        border,
        html_escape(&title)
    ));
    if options.line_numbers {
        svg.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" fill-opacity=\"0.42\"/>",
            outer_padding,
            outer_padding + frame_height,
            content_padding_x + gutter_width - 4.0,
            content_height - frame_height,
            gutter_fill
        ));
        svg.push_str(&format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{}\" stroke-opacity=\"0.24\"/>",
            text_start_x - 10.0,
            outer_padding + frame_height + 8.0,
            text_start_x - 10.0,
            outer_padding + content_height - 10.0,
            border
        ));
    }

    for (index, line) in lines.iter().enumerate() {
        let y = first_line_y + (index as f32 * line_height);
        if index % 2 == 1 {
            svg.push_str(&format!(
                "<rect x=\"{}\" y=\"{:.1}\" width=\"{}\" height=\"{}\" fill=\"#ffffff\" fill-opacity=\"0.018\"/>",
                outer_padding + 1.0,
                y - 17.0,
                content_width - 2.0,
                line_height
            ));
        }
        if options.line_numbers {
            svg.push_str(&format!(
                "<text x=\"{line_number_x}\" y=\"{y}\" fill=\"{line_number_color}\" fill-opacity=\"0.75\" font-family=\"SFMono-Regular, Menlo, Consolas, monospace\" font-size=\"13\">{}</text>",
                index + 1
            ));
        }

        let mut x = text_start_x;
        let line_style = line.style;
        for span in &line.spans {
            if span.content.is_empty() {
                continue;
            }

            let style = line_style.patch(span.style);
            let text = span.content.as_ref();
            let span_width = visible_width(text) as f32 * char_width;
            if let Some(bg) = style.bg.and_then(color_to_css) {
                svg.push_str(&format!(
                    "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"18\" rx=\"4\" fill=\"{}\"/>",
                    x - 1.0,
                    y - 13.0,
                    span_width.max(1.0) + 2.0,
                    bg
                ));
            }

            let fill = style
                .fg
                .and_then(color_to_css)
                .unwrap_or_else(|| base_fg.clone());
            let weight = if style.add_modifier.contains(Modifier::BOLD) {
                " font-weight=\"700\""
            } else {
                ""
            };
            let font_style = if style.add_modifier.contains(Modifier::ITALIC) {
                " font-style=\"italic\""
            } else {
                ""
            };
            let text_decoration = if style.add_modifier.contains(Modifier::UNDERLINED) {
                " text-decoration=\"underline\""
            } else {
                ""
            };
            svg.push_str(&format!(
                "<text x=\"{x:.1}\" y=\"{y:.1}\" fill=\"{fill}\" font-family=\"SFMono-Regular, Menlo, Consolas, monospace\" font-size=\"{font_size}\"{weight}{font_style}{text_decoration}>{}</text>",
                html_escape(text)
            ));
            x += span_width;
        }
    }

    svg.push_str("</svg>\n");
    svg
}

fn html_css(theme: &Theme, paper_size: &str) -> String {
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
            ".export-meta {{ margin: 6px 0 0; color: color-mix(in srgb, var(--implicit-fg) 78%, transparent); font-size: 12px; }}\n",
            ".line {{ white-space: pre-wrap; padding: 0 24px; min-height: 1.45em; }}\n",
            ".line:first-of-type {{ padding-top: 20px; }}\n",
            ".line:last-of-type {{ padding-bottom: 24px; }}\n",
            "@page {{ size: {}; margin: 18mm; }}\n",
            "@media print {{ body {{ padding: 0; background: white; }} .document {{ border: none; border-radius: 0; max-width: none; }} .export-header {{ break-after: avoid; }} }}\n"
        ),
        color_or_default(theme.background.bg, "#111827"),
        color_or_default(base_text_style(theme).fg, "#e5e7eb"),
        color_or_default(theme.ui_chrome.fg, "#7dd3fc"),
        color_or_default(theme.selection.bg, "#374151"),
        color_or_default(theme.code.bg, "#1f2937"),
        paper_size,
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

fn plain_line_width(line: &Line<'static>) -> usize {
    line.spans
        .iter()
        .map(|span| visible_width(span.content.as_ref()))
        .sum()
}

fn visible_width(text: &str) -> usize {
    text.chars().count()
}

fn abbreviate_middle(text: &str, max_chars: usize) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return text.to_owned();
    }

    let keep = max_chars.saturating_sub(1);
    let front = keep / 2;
    let back = keep - front;
    let mut shortened = chars[..front].iter().collect::<String>();
    shortened.push('…');
    shortened.push_str(&chars[chars.len() - back..].iter().collect::<String>());
    shortened
}

fn default_paper_size() -> &'static str {
    let locale = env::var("LC_PAPER").ok().or_else(|| env::var("LANG").ok());
    paper_size_for_locale(locale.as_deref())
}

fn paper_size_for_locale(locale: Option<&str>) -> &'static str {
    let Some(locale) = locale.map(str::trim).filter(|locale| !locale.is_empty()) else {
        return "Letter";
    };

    let upper = locale.to_ascii_uppercase();
    if upper.contains("US") || upper.contains("CA") || upper.contains("MX") {
        "Letter"
    } else {
        "A4"
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PdfTool {
    Wkhtmltopdf,
    Chromium,
    ChromiumBrowser,
    GoogleChrome,
    GoogleChromeStable,
}

impl PdfTool {
    fn program(self) -> &'static str {
        match self {
            PdfTool::Wkhtmltopdf => "wkhtmltopdf",
            PdfTool::Chromium => "chromium",
            PdfTool::ChromiumBrowser => "chromium-browser",
            PdfTool::GoogleChrome => "google-chrome",
            PdfTool::GoogleChromeStable => "google-chrome-stable",
        }
    }
}

fn export_pdf_document(
    input_path: &Path,
    output_path: Option<&Path>,
    html: &str,
) -> Result<PathBuf> {
    let pdf_output_path = resolve_pdf_output_path(input_path, output_path);
    if let Some(tool) = detect_pdf_tool() {
        let temp_html_path = temp_html_export_path(&pdf_output_path);
        fs::write(&temp_html_path, html)
            .with_context(|| format!("failed to write {}", temp_html_path.display()))?;

        let render_result = render_pdf_with_tool(tool, &temp_html_path, &pdf_output_path);
        let cleanup_result = fs::remove_file(&temp_html_path);
        if let Err(error) = cleanup_result {
            eprintln!(
                "implicit: warning: failed to clean up temporary export {}: {error}",
                temp_html_path.display()
            );
        }

        render_result?;
        return Ok(pdf_output_path);
    }

    let fallback_path = resolve_pdf_fallback_path(&pdf_output_path);
    fs::write(&fallback_path, html)
        .with_context(|| format!("failed to write {}", fallback_path.display()))?;
    eprintln!(
        "implicit: no PDF renderer found; wrote HTML fallback to {}",
        fallback_path.display()
    );
    Ok(fallback_path)
}

fn detect_pdf_tool() -> Option<PdfTool> {
    select_pdf_tool(command_exists)
}

fn select_pdf_tool<F>(mut command_exists: F) -> Option<PdfTool>
where
    F: FnMut(&str) -> bool,
{
    [
        PdfTool::Wkhtmltopdf,
        PdfTool::Chromium,
        PdfTool::ChromiumBrowser,
        PdfTool::GoogleChrome,
        PdfTool::GoogleChromeStable,
    ]
    .into_iter()
    .find(|tool| command_exists(tool.program()))
}

fn command_exists(program: &str) -> bool {
    env::var_os("PATH")
        .map(|paths| env::split_paths(&paths).any(|path| path.join(program).exists()))
        .unwrap_or(false)
}

fn temp_html_export_path(pdf_output_path: &Path) -> PathBuf {
    let stem = pdf_output_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("implicit-export");
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);
    env::temp_dir().join(format!("{stem}-{unique}.implicit-export.html"))
}

fn render_pdf_with_tool(tool: PdfTool, html_path: &Path, pdf_path: &Path) -> Result<()> {
    let status = match tool {
        PdfTool::Wkhtmltopdf => Command::new(tool.program())
            .arg(html_path)
            .arg(pdf_path)
            .status(),
        PdfTool::Chromium
        | PdfTool::ChromiumBrowser
        | PdfTool::GoogleChrome
        | PdfTool::GoogleChromeStable => Command::new(tool.program())
            .arg("--headless")
            .arg("--disable-gpu")
            .arg(format!("--print-to-pdf={}", pdf_path.display()))
            .arg(file_url(html_path)?)
            .status(),
    }
    .with_context(|| format!("failed to launch PDF renderer `{}`", tool.program()))?;

    if !status.success() {
        return Err(anyhow!(
            "PDF renderer `{}` exited with status {status}",
            tool.program()
        ));
    }

    Ok(())
}

fn file_url(path: &Path) -> Result<String> {
    let canonical = path
        .canonicalize()
        .with_context(|| format!("failed to resolve {}", path.display()))?;
    let encoded = canonical.to_string_lossy().replace(' ', "%20");
    Ok(format!("file://{encoded}"))
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
    fn pdf_output_defaults_to_input_stem() {
        let output = resolve_pdf_output_path(Path::new("notes.md"), None);
        assert_eq!(output, PathBuf::from("notes.pdf"));
        assert_eq!(
            resolve_pdf_fallback_path(Path::new("notes.pdf")),
            PathBuf::from("notes.html")
        );
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
        let html = render_lines_to_html_document(
            &lines,
            &theme,
            "Demo",
            HtmlDocumentOptions {
                paper_size: "Letter",
                source_label: String::from("demo.md"),
            },
        );
        assert!(html.contains("<title>Demo</title>"));
        assert!(html.contains("&lt;hello&gt;"));
        assert!(html.contains("--implicit-bg"));
        assert!(html.contains("font-weight:700"));
        assert!(html.contains("demo.md"));
        assert!(html.contains("@page { size: Letter; margin: 18mm; }"));
    }

    #[test]
    fn pdf_tool_selection_prefers_wkhtmltopdf_first() {
        let tool = select_pdf_tool(|program| matches!(program, "chromium" | "wkhtmltopdf"));
        assert_eq!(tool, Some(PdfTool::Wkhtmltopdf));
    }

    #[test]
    fn pdf_tool_selection_falls_back_to_chrome_family() {
        let tool = select_pdf_tool(|program| program == "google-chrome");
        assert_eq!(tool, Some(PdfTool::GoogleChrome));
    }

    #[test]
    fn paper_size_defaults_to_letter_for_us_locale() {
        assert_eq!(paper_size_for_locale(Some("en_US.UTF-8")), "Letter");
        assert_eq!(paper_size_for_locale(Some("en_CA.UTF-8")), "Letter");
    }

    #[test]
    fn paper_size_defaults_to_a4_outside_letter_regions() {
        assert_eq!(paper_size_for_locale(Some("en_GB.UTF-8")), "A4");
        assert_eq!(paper_size_for_locale(Some("de_DE.UTF-8")), "A4");
    }

    #[test]
    fn line_range_parser_accepts_single_line_or_range() {
        assert_eq!(parse_line_range("7"), Ok(LineRange { start: 7, end: 7 }));
        assert_eq!(
            parse_line_range("10-25"),
            Ok(LineRange { start: 10, end: 25 })
        );
    }

    #[test]
    fn slice_lines_applies_requested_range() {
        let lines = vec![
            String::from("one"),
            String::from("two"),
            String::from("three"),
            String::from("four"),
        ];
        let slice = slice_lines(&lines, Some(LineRange { start: 2, end: 3 })).expect("slice");
        assert_eq!(slice, vec![String::from("two"), String::from("three")]);
    }

    #[test]
    fn source_label_includes_range_when_present() {
        assert_eq!(
            source_label(
                Path::new("notes.md"),
                Some(LineRange { start: 10, end: 25 })
            ),
            String::from("notes.md [10-25]")
        );
    }

    #[test]
    fn snapshot_output_defaults_to_svg() {
        let output = resolve_snapshot_output_path(Path::new("main.rs"), None).expect("svg path");
        assert_eq!(
            output,
            SnapshotOutputTarget {
                path: PathBuf::from("main.svg"),
                format: SnapshotOutputFormat::Svg
            }
        );
    }

    #[test]
    fn snapshot_output_accepts_png_extensions() {
        let output =
            resolve_snapshot_output_path(Path::new("main.rs"), Some(Path::new("main.png")))
                .expect("png path");
        assert_eq!(
            output,
            SnapshotOutputTarget {
                path: PathBuf::from("main.png"),
                format: SnapshotOutputFormat::Png
            }
        );
    }

    #[test]
    fn snapshot_output_rejects_other_extensions() {
        let error = resolve_snapshot_output_path(Path::new("main.rs"), Some(Path::new("main.jpg")))
            .expect_err("jpg should be rejected");
        assert!(error.to_string().contains("supports only .svg or .png"));
    }

    #[test]
    fn svg_snapshot_includes_window_chrome_and_code() {
        let theme = Theme::source_hints_default();
        let lines = vec![Line::styled(
            "fn main() {}",
            Style::default().fg(Color::LightBlue),
        )];
        let svg = render_lines_to_svg_document(
            &lines,
            &theme,
            SnapshotOptions {
                title: String::from("main.rs [1-1]"),
                line_numbers: true,
            },
        );
        assert!(svg.contains("<svg"));
        assert!(svg.contains("<circle"));
        assert!(svg.contains("main.rs [1-1]"));
        assert!(svg.contains("fn main() {}"));
    }

    #[test]
    fn fenced_code_block_is_extracted_from_markdown_selection() {
        let lines = vec![
            String::from("```python"),
            String::from("print('hi')"),
            String::from("```"),
        ];
        let fenced = extract_fenced_code_block(&lines).expect("fenced block");
        assert_eq!(fenced.info.as_deref(), Some("python"));
        assert_eq!(fenced.lines, vec![String::from("print('hi')")]);
    }

    #[test]
    fn snapshot_prefers_fenced_code_render_for_markdown_selection() {
        let lines = vec![
            String::from("```rust"),
            String::from("fn main() {}"),
            String::from("```"),
        ];
        let theme = Theme::source_hints_default();
        let snapshot = prepare_snapshot_render(
            &lines,
            Path::new("notes.md"),
            FileType::Markdown,
            PrintMode::Auto,
            &theme,
            Some(LineRange { start: 5, end: 7 }),
            true,
        );
        assert!(snapshot.title.contains("rust"));
        assert!(snapshot.line_numbers);
        let text = snapshot
            .rendered
            .iter()
            .flat_map(|line| {
                line.spans
                    .iter()
                    .map(|span| span.content.as_ref().to_owned())
            })
            .collect::<String>();
        assert!(text.contains("fn"));
        assert!(!text.contains("```"));
    }

    #[test]
    fn preview_snapshots_disable_line_numbers() {
        let lines = vec![String::from("# Heading")];
        let theme = Theme::source_hints_default();
        let snapshot = prepare_snapshot_render(
            &lines,
            Path::new("notes.md"),
            FileType::Markdown,
            PrintMode::Auto,
            &theme,
            None,
            true,
        );
        assert!(!snapshot.line_numbers);
    }

    #[test]
    fn title_abbreviation_preserves_ends() {
        let text = abbreviate_middle("abcdefghijklmnopqrstuvwxyz", 10);
        assert_eq!(text, "abcd…vwxyz");
    }
}
