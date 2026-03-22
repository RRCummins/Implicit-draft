//! Loads built-in and user-overridden color themes for Source+Hints mode.

use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result, anyhow};
use ratatui::style::{Color, Modifier, Style};
use serde::Deserialize;

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub heading1: Style,
    pub heading2: Style,
    pub heading3: Style,
    pub heading4: Style,
    pub heading5: Style,
    pub heading6: Style,
    pub bold: Style,
    pub italic: Style,
    pub code: Style,
    pub blockquote: Style,
    pub link: Style,
    pub rule: Style,
    pub list_marker: Style,
}

#[derive(Debug, Deserialize)]
struct ThemeFile {
    heading1: Option<String>,
    heading2: Option<String>,
    heading3: Option<String>,
    heading4: Option<String>,
    heading5: Option<String>,
    heading6: Option<String>,
    bold: Option<String>,
    italic: Option<String>,
    code: Option<String>,
    code_bg: Option<String>,
    blockquote: Option<String>,
    link: Option<String>,
    rule: Option<String>,
    list_marker: Option<String>,
}

impl Theme {
    pub fn source_hints_default() -> Self {
        Self::builtin("dark").expect("dark builtin exists")
    }

    pub fn load_named(name: &str) -> Result<Self> {
        let mut theme = Self::builtin(name)?;

        if let Some(path) = theme_path(name).filter(|path| path.exists()) {
            let text = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let file: ThemeFile = toml::from_str(&text)
                .with_context(|| format!("failed to parse {}", path.display()))?;
            theme.apply_overrides(file)?;
        }

        Ok(theme)
    }

    pub fn heading(&self, level: usize) -> Style {
        match level {
            1 => self.heading1,
            2 => self.heading2,
            3 => self.heading3,
            4 => self.heading4,
            5 => self.heading5,
            _ => self.heading6,
        }
    }

    fn builtin(name: &str) -> Result<Self> {
        match name {
            "dark" => Ok(Self::from_palette(Palette {
                headings: [
                    Color::LightCyan,
                    Color::LightBlue,
                    Color::LightGreen,
                    Color::LightYellow,
                    Color::LightMagenta,
                    Color::Gray,
                ],
                code_fg: Color::Yellow,
                code_bg: Color::DarkGray,
                blockquote: Color::Gray,
                link: Color::LightBlue,
                rule: Color::DarkGray,
                list_marker: Color::LightGreen,
            })),
            "light" => Ok(Self::from_palette(Palette {
                headings: [
                    Color::Blue,
                    Color::Cyan,
                    Color::Green,
                    Color::Yellow,
                    Color::Magenta,
                    Color::Gray,
                ],
                code_fg: Color::DarkGray,
                code_bg: Color::Rgb(235, 235, 235),
                blockquote: Color::DarkGray,
                link: Color::Blue,
                rule: Color::Gray,
                list_marker: Color::Green,
            })),
            "gruvbox" => Ok(Self::from_palette(Palette {
                headings: [
                    Color::Rgb(250, 189, 47),
                    Color::Rgb(131, 165, 152),
                    Color::Rgb(184, 187, 38),
                    Color::Rgb(211, 134, 155),
                    Color::Rgb(251, 73, 52),
                    Color::Rgb(146, 131, 116),
                ],
                code_fg: Color::Rgb(235, 219, 178),
                code_bg: Color::Rgb(60, 56, 54),
                blockquote: Color::Rgb(168, 153, 132),
                link: Color::Rgb(131, 165, 152),
                rule: Color::Rgb(80, 73, 69),
                list_marker: Color::Rgb(184, 187, 38),
            })),
            "catppuccin-mocha" => Ok(Self::from_palette(Palette {
                headings: [
                    Color::Rgb(137, 180, 250),
                    Color::Rgb(116, 199, 236),
                    Color::Rgb(166, 227, 161),
                    Color::Rgb(249, 226, 175),
                    Color::Rgb(245, 194, 231),
                    Color::Rgb(186, 194, 222),
                ],
                code_fg: Color::Rgb(205, 214, 244),
                code_bg: Color::Rgb(49, 50, 68),
                blockquote: Color::Rgb(166, 173, 200),
                link: Color::Rgb(137, 180, 250),
                rule: Color::Rgb(88, 91, 112),
                list_marker: Color::Rgb(166, 227, 161),
            })),
            "catppuccin-latte" => Ok(Self::from_palette(Palette {
                headings: [
                    Color::Rgb(30, 102, 245),
                    Color::Rgb(4, 165, 229),
                    Color::Rgb(64, 160, 43),
                    Color::Rgb(223, 142, 29),
                    Color::Rgb(234, 118, 203),
                    Color::Rgb(108, 111, 133),
                ],
                code_fg: Color::Rgb(76, 79, 105),
                code_bg: Color::Rgb(220, 224, 232),
                blockquote: Color::Rgb(124, 127, 147),
                link: Color::Rgb(30, 102, 245),
                rule: Color::Rgb(172, 176, 190),
                list_marker: Color::Rgb(64, 160, 43),
            })),
            _ => Err(anyhow!("unknown theme: {name}")),
        }
    }

    fn from_palette(palette: Palette) -> Self {
        Self {
            heading1: Style::default()
                .fg(palette.headings[0])
                .add_modifier(Modifier::BOLD),
            heading2: Style::default()
                .fg(palette.headings[1])
                .add_modifier(Modifier::BOLD),
            heading3: Style::default()
                .fg(palette.headings[2])
                .add_modifier(Modifier::BOLD),
            heading4: Style::default()
                .fg(palette.headings[3])
                .add_modifier(Modifier::BOLD),
            heading5: Style::default()
                .fg(palette.headings[4])
                .add_modifier(Modifier::BOLD),
            heading6: Style::default()
                .fg(palette.headings[5])
                .add_modifier(Modifier::BOLD),
            bold: Style::default().add_modifier(Modifier::BOLD),
            italic: Style::default().add_modifier(Modifier::ITALIC),
            code: Style::default().fg(palette.code_fg).bg(palette.code_bg),
            blockquote: Style::default()
                .fg(palette.blockquote)
                .add_modifier(Modifier::ITALIC),
            link: Style::default()
                .fg(palette.link)
                .add_modifier(Modifier::UNDERLINED),
            rule: Style::default().fg(palette.rule),
            list_marker: Style::default().fg(palette.list_marker),
        }
    }

    fn apply_overrides(&mut self, file: ThemeFile) -> Result<()> {
        apply_fg(&mut self.heading1, file.heading1)?;
        apply_fg(&mut self.heading2, file.heading2)?;
        apply_fg(&mut self.heading3, file.heading3)?;
        apply_fg(&mut self.heading4, file.heading4)?;
        apply_fg(&mut self.heading5, file.heading5)?;
        apply_fg(&mut self.heading6, file.heading6)?;
        apply_fg(&mut self.bold, file.bold)?;
        apply_fg(&mut self.italic, file.italic)?;
        apply_fg(&mut self.code, file.code)?;
        apply_bg(&mut self.code, file.code_bg)?;
        apply_fg(&mut self.blockquote, file.blockquote)?;
        apply_fg(&mut self.link, file.link)?;
        apply_fg(&mut self.rule, file.rule)?;
        apply_fg(&mut self.list_marker, file.list_marker)?;
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct Palette {
    headings: [Color; 6],
    code_fg: Color,
    code_bg: Color,
    blockquote: Color,
    link: Color,
    rule: Color,
    list_marker: Color,
}

fn config_dir() -> PathBuf {
    if let Some(path) = env::var_os("IMPLICIT_CONFIG_DIR") {
        return PathBuf::from(path);
    }

    match env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".config/implicit"),
        None => PathBuf::from(".implicit"),
    }
}

fn theme_path(name: &str) -> Option<PathBuf> {
    let mut path = config_dir();
    path.push("themes");
    path.push(format!("{name}.toml"));
    Some(path)
}

fn apply_fg(style: &mut Style, value: Option<String>) -> Result<()> {
    if let Some(value) = value {
        *style = style.fg(parse_color(&value)?);
    }
    Ok(())
}

fn apply_bg(style: &mut Style, value: Option<String>) -> Result<()> {
    if let Some(value) = value {
        *style = style.bg(parse_color(&value)?);
    }
    Ok(())
}

fn parse_color(value: &str) -> Result<Color> {
    let trimmed = value.trim();
    let hex = trimmed.strip_prefix('#').unwrap_or(trimmed);

    if hex.len() != 6 {
        return Err(anyhow!("expected 6-digit hex color, got {value}"));
    }

    let red = u8::from_str_radix(&hex[0..2], 16)?;
    let green = u8::from_str_radix(&hex[2..4], 16)?;
    let blue = u8::from_str_radix(&hex[4..6], 16)?;
    Ok(Color::Rgb(red, green, blue))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_builtin_theme() {
        let theme = Theme::load_named("dark").expect("builtin theme");
        assert_eq!(theme.heading(1), theme.heading1);
    }

    #[test]
    fn loads_catppuccin_latte_theme() {
        let theme = Theme::load_named("catppuccin-latte").expect("builtin theme");
        assert_eq!(theme.heading(1), theme.heading1);
    }

    #[test]
    fn parses_hex_colors() {
        assert_eq!(
            parse_color("#112233").expect("hex color"),
            Color::Rgb(0x11, 0x22, 0x33)
        );
    }
}
