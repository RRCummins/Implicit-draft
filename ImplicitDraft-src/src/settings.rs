//! Holds config editor state and option navigation.

use anyhow::Result;
use ratatui::text::Line;

use crate::{code, config::AppConfig, filetype::FileType, preview, theme::Theme};

const TAB_WIDTH_OPTIONS: [usize; 3] = [2, 4, 8];
const PREVIEW_SAMPLE: &str = "# Heading\n## Secondary Heading\n**bold** and *italic*\n`inline code`\n\n> blockquote\n\n- list item one\n- list item two\n- [x] done item\n\n[implicit.dev](https://implicit.dev)\n\n```rust\nfn main() {}\n```";
const CODE_PREVIEW_SAMPLE: &str = "fn paint(theme: Theme) {\n    let accent = \"implicit\";\n    let port = 8080;\n    // preview code colors\n}";

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
    applied_config: AppConfig,
    draft_config: AppConfig,
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
            applied_config: config.clone(),
            draft_config: config.clone(),
            theme_names,
            selected_theme,
            active_pane: ConfigPane::Theme,
            selected_option: 0,
        })
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

    pub fn applied_theme_name(&self) -> &str {
        &self.applied_config.theme
    }

    pub fn preview_theme(&self) -> Theme {
        Theme::load_named(&self.draft_config.theme).unwrap_or_else(|_| {
            Theme::load_named("dark").unwrap_or_else(|_| Theme::source_hints_default())
        })
    }

    pub fn preview_lines(&self, theme: &Theme, width: usize) -> Vec<Line<'static>> {
        let markdown_lines = PREVIEW_SAMPLE
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let code_lines = CODE_PREVIEW_SAMPLE
            .lines()
            .map(str::to_owned)
            .collect::<Vec<_>>();

        let mut rendered = preview::render_document(&markdown_lines, theme, width);
        rendered.push(Line::raw(String::new()));
        rendered.push(Line::raw(String::from("Source Preview")));
        rendered.extend(code::render_preview_document(
            &code_lines,
            theme,
            Some(std::path::Path::new("preview.rs")),
            FileType::Code,
            width,
        ));
        rendered
    }

    pub fn option_rows(&self) -> Vec<ConfigOptionRow> {
        ConfigOption::ALL
            .iter()
            .map(|option| ConfigOptionRow {
                label: option.label().to_owned(),
                value: match option {
                    ConfigOption::DefaultMode => {
                        format!("[{} v]", display_mode(self.draft_config.default_mode))
                    }
                    ConfigOption::TabWidth => format!("[{}]", self.draft_config.tab_width()),
                    ConfigOption::LineNumbers => {
                        checkbox(self.draft_config.line_numbers).to_owned()
                    }
                    ConfigOption::Wrap => checkbox(self.draft_config.wrap).to_owned(),
                    ConfigOption::VimKeys => checkbox(self.draft_config.vim_keys).to_owned(),
                    ConfigOption::AutoSave => checkbox(self.draft_config.auto_save).to_owned(),
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
                self.sync_selected_theme();
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
                self.sync_selected_theme();
            }
            ConfigPane::Options => {
                let max_index = ConfigOption::ALL.len().saturating_sub(1);
                self.selected_option = (self.selected_option + 1).min(max_index);
            }
        }
    }

    pub fn apply_draft(&mut self, config: &mut AppConfig) {
        self.applied_config = self.draft_config.clone();
        *config = self.applied_config.clone();
    }

    pub fn keep_applied(&self, config: &mut AppConfig) {
        *config = self.applied_config.clone();
    }

    pub fn revert_to_original(&self, config: &mut AppConfig) {
        *config = self.original_config.clone();
    }

    pub fn mark_saved(&mut self) {
        self.original_config = self.applied_config.clone();
        self.draft_config = self.applied_config.clone();
        self.align_selected_theme();
    }

    pub fn cycle_option_forward(&mut self) {
        self.cycle_option(1);
    }

    pub fn cycle_option_backward(&mut self) {
        self.cycle_option(-1);
    }

    fn cycle_option(&mut self, direction: i8) {
        let option = ConfigOption::ALL[self.selected_option];
        match option {
            ConfigOption::DefaultMode => {
                self.draft_config.default_mode = if direction >= 0 {
                    self.draft_config.default_mode.next()
                } else {
                    self.draft_config.default_mode.previous()
                };
            }
            ConfigOption::TabWidth => {
                let current = self.draft_config.tab_width();
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
                self.draft_config.tab_width = TAB_WIDTH_OPTIONS[next];
            }
            ConfigOption::LineNumbers => {
                self.draft_config.line_numbers = !self.draft_config.line_numbers;
            }
            ConfigOption::Wrap => self.draft_config.wrap = !self.draft_config.wrap,
            ConfigOption::VimKeys => self.draft_config.vim_keys = !self.draft_config.vim_keys,
            ConfigOption::AutoSave => self.draft_config.auto_save = !self.draft_config.auto_save,
        }
    }

    fn sync_selected_theme(&mut self) {
        self.draft_config.theme = self.selected_theme_name().to_owned();
    }

    fn align_selected_theme(&mut self) {
        if let Some(index) = self
            .theme_names
            .iter()
            .position(|name| name == &self.draft_config.theme)
        {
            self.selected_theme = index;
        }
    }
}

fn checkbox(enabled: bool) -> &'static str {
    if enabled { "[x] on" } else { "[ ] off" }
}

fn display_mode(mode: crate::config::DefaultMode) -> &'static str {
    match mode {
        crate::config::DefaultMode::SourceHints => "source+hints",
        crate::config::DefaultMode::Preview => "preview",
        crate::config::DefaultMode::Source => "source",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_options_render_current_values() {
        let state = ConfigState::new(&AppConfig::default()).expect("config state");
        let rows = state.option_rows();

        assert_eq!(rows[0].label, "default_mode");
        assert_eq!(rows[0].value, "[source+hints v]");
        assert_eq!(rows[1].value, "[4]");
    }

    #[test]
    fn cycling_tab_width_rotates_known_values() {
        let mut state = ConfigState::new(&AppConfig::default()).expect("config state");
        state.toggle_pane();
        state.move_down();

        state.cycle_option_forward();
        let mut applied = AppConfig::default();
        state.apply_draft(&mut applied);

        assert_eq!(applied.tab_width, 8);
    }

    #[test]
    fn moving_theme_selection_only_changes_preview_until_applied() {
        let mut state = ConfigState::new(&AppConfig::default()).expect("config state");

        state.move_down();

        assert_ne!(state.selected_theme_name(), "dark");
        assert_eq!(state.applied_theme_name(), "dark");
        assert_eq!(state.draft_config.theme, state.selected_theme_name());
    }

    #[test]
    fn cancel_restores_original_config() {
        let mut state = ConfigState::new(&AppConfig::default()).expect("config state");
        let mut config = AppConfig::default();

        state.toggle_pane();
        state.move_down();
        state.move_down();
        state.cycle_option_forward();
        state.apply_draft(&mut config);
        state.revert_to_original(&mut config);

        assert!(!config.line_numbers);
        assert_eq!(config.theme, "dark");
    }

    #[test]
    fn preview_lines_include_code_preview_section() {
        let state = ConfigState::new(&AppConfig::default()).expect("config state");
        let theme = Theme::source_hints_default();
        let lines = state.preview_lines(&theme, 48);

        assert!(lines.iter().any(|line| {
            line.spans
                .iter()
                .any(|span| span.content.as_ref() == "Source Preview")
        }));
        assert!(
            lines
                .iter()
                .any(|line| line.spans.iter().any(|span| span.content.as_ref() == "fn"))
        );
    }
}
