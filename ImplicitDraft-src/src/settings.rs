//! Holds config editor state and option navigation.

use anyhow::Result;

use crate::{config::AppConfig, preview, theme::Theme};

const TAB_WIDTH_OPTIONS: [usize; 3] = [2, 4, 8];
const PREVIEW_SAMPLE: &str = "# Heading\n**bold** and *italic*\n`inline code`\n\n> blockquote\n\n- list item one\n- list item two\n\n[implicit.dev](https://implicit.dev)\n\n```rust\nfn main() {}\n```";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigPane {
    Theme,
    Options,
}

impl ConfigPane {
    pub fn toggle(self) -> Self {
        match self {
            Self::Theme => Self::Options,
            Self::Options => Self::Theme,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigOption {
    DefaultMode,
    TabWidth,
    LineNumbers,
    Wrap,
    VimKeys,
    AutoSave,
}

impl ConfigOption {
    pub const ALL: [Self; 6] = [
        Self::DefaultMode,
        Self::TabWidth,
        Self::LineNumbers,
        Self::Wrap,
        Self::VimKeys,
        Self::AutoSave,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::DefaultMode => "default_mode",
            Self::TabWidth => "tab_width",
            Self::LineNumbers => "line_numbers",
            Self::Wrap => "wrap",
            Self::VimKeys => "vim_keys",
            Self::AutoSave => "auto_save",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ConfigOptionRow {
    pub label: String,
    pub value: String,
}

#[derive(Debug)]
pub struct ConfigState {
    original_config: AppConfig,
    theme_names: Vec<String>,
    selected_theme: usize,
    active_pane: ConfigPane,
    selected_option: usize,
}

impl ConfigState {
    pub fn new(config: &AppConfig) -> Result<Self> {
        let mut theme_names = Theme::available_names()?;
        if !theme_names.iter().any(|name| name == &config.theme) {
            theme_names.push(config.theme.clone());
            theme_names.sort();
        }

        let selected_theme = theme_names
            .iter()
            .position(|name| name == &config.theme)
            .unwrap_or(0);

        Ok(Self {
            original_config: config.clone(),
            theme_names,
            selected_theme,
            active_pane: ConfigPane::Theme,
            selected_option: 0,
        })
    }

    pub fn original_config(&self) -> &AppConfig {
        &self.original_config
    }

    pub fn active_pane(&self) -> ConfigPane {
        self.active_pane
    }

    pub fn selected_theme(&self) -> usize {
        self.selected_theme
    }

    pub fn selected_option(&self) -> usize {
        self.selected_option
    }

    pub fn theme_names(&self) -> &[String] {
        &self.theme_names
    }

    pub fn selected_theme_name(&self) -> &str {
        &self.theme_names[self.selected_theme]
    }

    pub fn preview_lines(&self, theme: &Theme, width: usize) -> Vec<ratatui::text::Line<'static>> {
        let lines = PREVIEW_SAMPLE
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        preview::render_document(&lines, theme, width)
    }

    pub fn option_rows(&self, config: &AppConfig) -> Vec<ConfigOptionRow> {
        ConfigOption::ALL
            .iter()
            .map(|option| ConfigOptionRow {
                label: option.label().to_owned(),
                value: match option {
                    ConfigOption::DefaultMode => config.default_mode.label().to_owned(),
                    ConfigOption::TabWidth => config.tab_width().to_string(),
                    ConfigOption::LineNumbers => on_off(config.line_numbers).to_owned(),
                    ConfigOption::Wrap => on_off(config.wrap).to_owned(),
                    ConfigOption::VimKeys => on_off(config.vim_keys).to_owned(),
                    ConfigOption::AutoSave => on_off(config.auto_save).to_owned(),
                },
            })
            .collect()
    }

    pub fn toggle_pane(&mut self) {
        self.active_pane = self.active_pane.toggle();
    }

    pub fn move_up(&mut self) {
        match self.active_pane {
            ConfigPane::Theme => {
                self.selected_theme = self.selected_theme.saturating_sub(1);
            }
            ConfigPane::Options => {
                self.selected_option = self.selected_option.saturating_sub(1);
            }
        }
    }

    pub fn move_down(&mut self) {
        match self.active_pane {
            ConfigPane::Theme => {
                let max_index = self.theme_names.len().saturating_sub(1);
                self.selected_theme = (self.selected_theme + 1).min(max_index);
            }
            ConfigPane::Options => {
                let max_index = ConfigOption::ALL.len().saturating_sub(1);
                self.selected_option = (self.selected_option + 1).min(max_index);
            }
        }
    }

    pub fn sync_theme(&self, config: &mut AppConfig) {
        config.theme = self.selected_theme_name().to_owned();
    }

    pub fn cycle_option_forward(&self, config: &mut AppConfig) {
        self.cycle_option(config, 1);
    }

    pub fn cycle_option_backward(&self, config: &mut AppConfig) {
        self.cycle_option(config, -1);
    }

    fn cycle_option(&self, config: &mut AppConfig, direction: i8) {
        let option = ConfigOption::ALL[self.selected_option];
        match option {
            ConfigOption::DefaultMode => {
                config.default_mode = if direction >= 0 {
                    config.default_mode.next()
                } else {
                    config.default_mode.previous()
                };
            }
            ConfigOption::TabWidth => {
                let current = config.tab_width();
                let index = TAB_WIDTH_OPTIONS
                    .iter()
                    .position(|value| *value == current)
                    .unwrap_or(1);
                let next = if direction >= 0 {
                    (index + 1) % TAB_WIDTH_OPTIONS.len()
                } else if index == 0 {
                    TAB_WIDTH_OPTIONS.len() - 1
                } else {
                    index - 1
                };
                config.tab_width = TAB_WIDTH_OPTIONS[next];
            }
            ConfigOption::LineNumbers => config.line_numbers = !config.line_numbers,
            ConfigOption::Wrap => config.wrap = !config.wrap,
            ConfigOption::VimKeys => config.vim_keys = !config.vim_keys,
            ConfigOption::AutoSave => config.auto_save = !config.auto_save,
        }
    }
}

fn on_off(enabled: bool) -> &'static str {
    if enabled { "on" } else { "off" }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_options_render_current_values() {
        let state = ConfigState::new(&AppConfig::default()).expect("config state");
        let rows = state.option_rows(&AppConfig::default());

        assert_eq!(rows[0].label, "default_mode");
        assert_eq!(rows[0].value, "source_hints");
        assert_eq!(rows[1].value, "4");
    }

    #[test]
    fn cycling_tab_width_rotates_known_values() {
        let mut state = ConfigState::new(&AppConfig::default()).expect("config state");
        state.toggle_pane();
        state.move_down();
        let mut config = AppConfig::default();

        state.cycle_option_forward(&mut config);
        assert_eq!(config.tab_width, 8);
    }
}
