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
    pub ui_chrome: Style,
    pub git_added: Style,
    pub git_modified: Style,
    pub git_untracked: Style,
    pub cursor: Style,
    pub selection: Style,
    pub background: Style,
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
    ui_chrome: Option<String>,
    git_added: Option<String>,
    git_modified: Option<String>,
    git_untracked: Option<String>,
    cursor: Option<String>,
    selection: Option<String>,
    background: Option<String>,
}

impl Theme {
    pub fn source_hints_default() -> Self {
        Self::builtin("dark").expect("dark builtin exists")
    }

    pub fn available_names() -> Result<Vec<String>> {
        let mut names = BUILTIN_THEME_NAMES
            .iter()
            .map(|name| (*name).to_owned())
            .collect::<Vec<_>>();

        let themes_dir = config_dir().join("themes");
        if themes_dir.exists() {
            for entry in fs::read_dir(&themes_dir)
                .with_context(|| format!("failed to read {}", themes_dir.display()))?
            {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|value| value.to_str()) != Some("toml") {
                    continue;
                }

                let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                    continue;
                };

                if !names.iter().any(|name| name == stem) {
                    names.push(stem.to_owned());
                }
            }
        }

        names.sort();
        Ok(names)
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
                ui_chrome: Color::Cyan,
                cursor: Color::Rgb(42, 58, 76),
                selection: Color::DarkGray,
                background: Color::Reset,
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
                ui_chrome: Color::Blue,
                cursor: Color::Rgb(214, 224, 235),
                selection: Color::Rgb(220, 228, 238),
                background: Color::Reset,
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
                ui_chrome: Color::Rgb(250, 189, 47),
                cursor: Color::Rgb(69, 64, 61),
                selection: Color::Rgb(60, 56, 54),
                background: Color::Reset,
            })),
            "gruvbox-dark" => Ok(Self::from_palette(Palette {
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
                ui_chrome: Color::Rgb(250, 189, 47),
                cursor: Color::Rgb(69, 64, 61),
                selection: Color::Rgb(60, 56, 54),
                background: Color::Reset,
            })),
            "gruvbox-light" => Ok(Self::from_palette(Palette {
                headings: [
                    Color::Rgb(175, 58, 3),
                    Color::Rgb(7, 102, 120),
                    Color::Rgb(121, 116, 14),
                    Color::Rgb(143, 63, 113),
                    Color::Rgb(157, 0, 6),
                    Color::Rgb(146, 131, 116),
                ],
                code_fg: Color::Rgb(60, 56, 54),
                code_bg: Color::Rgb(251, 241, 199),
                blockquote: Color::Rgb(124, 111, 100),
                link: Color::Rgb(7, 102, 120),
                rule: Color::Rgb(213, 196, 161),
                list_marker: Color::Rgb(121, 116, 14),
                ui_chrome: Color::Rgb(175, 58, 3),
                cursor: Color::Rgb(235, 219, 178),
                selection: Color::Rgb(239, 225, 188),
                background: Color::Reset,
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
                ui_chrome: Color::Rgb(148, 226, 213),
                cursor: Color::Rgb(69, 71, 90),
                selection: Color::Rgb(49, 50, 68),
                background: Color::Reset,
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
                ui_chrome: Color::Rgb(30, 102, 245),
                cursor: Color::Rgb(220, 224, 232),
                selection: Color::Rgb(204, 208, 218),
                background: Color::Reset,
            })),
            "dracula" => Ok(Self::from_palette(Palette {
                headings: [
                    Color::Rgb(139, 233, 253),
                    Color::Rgb(80, 250, 123),
                    Color::Rgb(241, 250, 140),
                    Color::Rgb(255, 184, 108),
                    Color::Rgb(255, 121, 198),
                    Color::Rgb(189, 147, 249),
                ],
                code_fg: Color::Rgb(248, 248, 242),
                code_bg: Color::Rgb(68, 71, 90),
                blockquote: Color::Rgb(98, 114, 164),
                link: Color::Rgb(139, 233, 253),
                rule: Color::Rgb(98, 114, 164),
                list_marker: Color::Rgb(80, 250, 123),
                ui_chrome: Color::Rgb(255, 121, 198),
                cursor: Color::Rgb(68, 71, 90),
                selection: Color::Rgb(68, 71, 90),
                background: Color::Reset,
            })),
            "nord" => Ok(Self::from_palette(Palette {
                headings: [
                    Color::Rgb(136, 192, 208),
                    Color::Rgb(129, 161, 193),
                    Color::Rgb(163, 190, 140),
                    Color::Rgb(235, 203, 139),
                    Color::Rgb(180, 142, 173),
                    Color::Rgb(216, 222, 233),
                ],
                code_fg: Color::Rgb(229, 233, 240),
                code_bg: Color::Rgb(67, 76, 94),
                blockquote: Color::Rgb(129, 161, 193),
                link: Color::Rgb(136, 192, 208),
                rule: Color::Rgb(76, 86, 106),
                list_marker: Color::Rgb(163, 190, 140),
                ui_chrome: Color::Rgb(143, 188, 187),
                cursor: Color::Rgb(59, 66, 82),
                selection: Color::Rgb(67, 76, 94),
                background: Color::Reset,
            })),
            "one-dark" => Ok(Self::from_palette(Palette {
                headings: [
                    Color::Rgb(97, 175, 239),
                    Color::Rgb(86, 182, 194),
                    Color::Rgb(152, 195, 121),
                    Color::Rgb(229, 192, 123),
                    Color::Rgb(198, 120, 221),
                    Color::Rgb(171, 178, 191),
                ],
                code_fg: Color::Rgb(171, 178, 191),
                code_bg: Color::Rgb(40, 44, 52),
                blockquote: Color::Rgb(92, 99, 112),
                link: Color::Rgb(97, 175, 239),
                rule: Color::Rgb(92, 99, 112),
                list_marker: Color::Rgb(152, 195, 121),
                ui_chrome: Color::Rgb(198, 120, 221),
                cursor: Color::Rgb(49, 54, 63),
                selection: Color::Rgb(40, 44, 52),
                background: Color::Reset,
            })),
            "rose-pine" => Ok(Self::from_palette(Palette {
                headings: [
                    Color::Rgb(196, 167, 231),
                    Color::Rgb(156, 207, 216),
                    Color::Rgb(235, 188, 186),
                    Color::Rgb(246, 193, 119),
                    Color::Rgb(235, 111, 146),
                    Color::Rgb(224, 222, 244),
                ],
                code_fg: Color::Rgb(224, 222, 244),
                code_bg: Color::Rgb(38, 35, 58),
                blockquote: Color::Rgb(144, 140, 170),
                link: Color::Rgb(156, 207, 216),
                rule: Color::Rgb(57, 53, 82),
                list_marker: Color::Rgb(246, 193, 119),
                ui_chrome: Color::Rgb(196, 167, 231),
                cursor: Color::Rgb(49, 44, 63),
                selection: Color::Rgb(38, 35, 58),
                background: Color::Reset,
            })),
            "solarized-dark" => Ok(Self::from_palette(Palette {
                headings: [
                    Color::Rgb(38, 139, 210),
                    Color::Rgb(42, 161, 152),
                    Color::Rgb(133, 153, 0),
                    Color::Rgb(181, 137, 0),
                    Color::Rgb(211, 54, 130),
                    Color::Rgb(147, 161, 161),
                ],
                code_fg: Color::Rgb(147, 161, 161),
                code_bg: Color::Rgb(7, 54, 66),
                blockquote: Color::Rgb(88, 110, 117),
                link: Color::Rgb(38, 139, 210),
                rule: Color::Rgb(88, 110, 117),
                list_marker: Color::Rgb(133, 153, 0),
                ui_chrome: Color::Rgb(42, 161, 152),
                cursor: Color::Rgb(0, 43, 54),
                selection: Color::Rgb(7, 54, 66),
                background: Color::Reset,
            })),
            "tokyo-night" => Ok(Self::from_palette(Palette {
                headings: [
                    Color::Rgb(122, 162, 247),
                    Color::Rgb(125, 207, 255),
                    Color::Rgb(158, 206, 106),
                    Color::Rgb(224, 175, 104),
                    Color::Rgb(187, 154, 247),
                    Color::Rgb(192, 202, 245),
                ],
                code_fg: Color::Rgb(192, 202, 245),
                code_bg: Color::Rgb(36, 40, 59),
                blockquote: Color::Rgb(86, 95, 137),
                link: Color::Rgb(122, 162, 247),
                rule: Color::Rgb(65, 72, 104),
                list_marker: Color::Rgb(158, 206, 106),
                ui_chrome: Color::Rgb(125, 207, 255),
                cursor: Color::Rgb(41, 46, 66),
                selection: Color::Rgb(36, 40, 59),
                background: Color::Reset,
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
            ui_chrome: Style::default()
                .fg(palette.ui_chrome)
                .add_modifier(Modifier::BOLD),
            git_added: Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
            git_modified: Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
            git_untracked: Style::default()
                .fg(Color::LightMagenta)
                .add_modifier(Modifier::BOLD),
            cursor: Style::default().bg(palette.cursor),
            selection: Style::default().bg(palette.selection),
            background: Style::default().bg(palette.background),
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
        apply_fg(&mut self.ui_chrome, file.ui_chrome)?;
        apply_fg(&mut self.git_added, file.git_added)?;
        apply_fg(&mut self.git_modified, file.git_modified)?;
        apply_fg(&mut self.git_untracked, file.git_untracked)?;
        apply_bg(&mut self.cursor, file.cursor)?;
        apply_bg(&mut self.selection, file.selection)?;
        apply_bg(&mut self.background, file.background)?;
        Ok(())
    }
}

const BUILTIN_THEME_NAMES: &[&str] = &[
    "dark",
    "light",
    "gruvbox",
    "gruvbox-dark",
    "gruvbox-light",
    "catppuccin-mocha",
    "catppuccin-latte",
    "dracula",
    "nord",
    "one-dark",
    "rose-pine",
    "solarized-dark",
    "tokyo-night",
];

#[derive(Clone, Copy)]
struct Palette {
    headings: [Color; 6],
    code_fg: Color,
    code_bg: Color,
    blockquote: Color,
    link: Color,
    rule: Color,
    list_marker: Color,
    ui_chrome: Color,
    cursor: Color,
    selection: Color,
    background: Color,
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
