//! Loads editor configuration and first-run defaults from `~/.config/implicit/`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub theme: String,
    pub default_mode: DefaultMode,
    pub tab_width: usize,
    pub line_numbers: bool,
    pub wrap: bool,
    pub vim_keys: bool,
    pub auto_save: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DefaultMode {
    #[default]
    SourceHints,
    Preview,
    Source,
}

impl DefaultMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::SourceHints => "source_hints",
            Self::Preview => "preview",
            Self::Source => "source",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::SourceHints => Self::Preview,
            Self::Preview => Self::Source,
            Self::Source => Self::SourceHints,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::SourceHints => Self::Source,
            Self::Preview => Self::SourceHints,
            Self::Source => Self::Preview,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: String::from("dark"),
            default_mode: DefaultMode::SourceHints,
            tab_width: 4,
            line_numbers: false,
            wrap: true,
            vim_keys: false,
            auto_save: false,
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        Self::load_from_dir(&config_dir())
    }

    pub fn save(&self) -> Result<()> {
        let dir = config_dir();
        ensure_default_files(&dir)?;
        let path = config_path(&dir);
        fs::write(&path, self.to_toml())
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    fn load_from_dir(dir: &Path) -> Result<Self> {
        ensure_default_files(dir)?;
        let path = config_path(dir);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let parsed =
            toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(parsed)
    }

    pub fn tab_width(&self) -> usize {
        self.tab_width.max(1)
    }

    fn to_toml(&self) -> String {
        let defaults = Self::default();
        let mut lines = Vec::new();

        if self.theme != defaults.theme {
            lines.push(format!("theme = {:?}", self.theme));
        }
        if self.default_mode != defaults.default_mode {
            lines.push(format!("default_mode = {:?}", self.default_mode.label()));
        }
        if self.tab_width() != defaults.tab_width {
            lines.push(format!("tab_width = {}", self.tab_width()));
        }
        if self.line_numbers != defaults.line_numbers {
            lines.push(format!("line_numbers = {}", self.line_numbers));
        }
        if self.wrap != defaults.wrap {
            lines.push(format!("wrap = {}", self.wrap));
        }
        if self.vim_keys != defaults.vim_keys {
            lines.push(format!("vim_keys = {}", self.vim_keys));
        }
        if self.auto_save != defaults.auto_save {
            lines.push(format!("auto_save = {}", self.auto_save));
        }

        if lines.is_empty() {
            String::from("# implicit uses built-in defaults\n")
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }
}

fn ensure_default_files(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;

    let themes_dir = dir.join("themes");
    fs::create_dir_all(&themes_dir)
        .with_context(|| format!("failed to create {}", themes_dir.display()))?;

    let config_path = config_path(dir);
    if !config_path.exists() {
        fs::write(&config_path, default_config_toml())
            .with_context(|| format!("failed to write {}", config_path.display()))?;
    }

    let keybindings_path = dir.join("keybindings.toml");
    if !keybindings_path.exists() {
        fs::write(&keybindings_path, default_keybindings_toml())
            .with_context(|| format!("failed to write {}", keybindings_path.display()))?;
    }

    Ok(())
}

fn config_path(dir: &Path) -> PathBuf {
    let mut path = dir.to_path_buf();
    path.push("config.toml");
    path
}

fn config_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("IMPLICIT_CONFIG_DIR") {
        return PathBuf::from(path);
    }

    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".config/implicit"),
        None => PathBuf::from(".implicit"),
    }
}

fn default_config_toml() -> &'static str {
    r#"theme = "dark"
default_mode = "source_hints"
tab_width = 4
line_numbers = false
wrap = true
vim_keys = false
auto_save = false
"#
}

fn default_keybindings_toml() -> &'static str {
    "# key overrides land here in a later phase\n"
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_config_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("implicit-config-{name}-{unique}"))
    }

    #[test]
    fn load_creates_default_files_on_first_run() {
        let dir = temp_config_dir("create-defaults");

        let config = AppConfig::load_from_dir(&dir).expect("load config");

        assert_eq!(config.theme, "dark");
        assert_eq!(config.default_mode, DefaultMode::SourceHints);
        assert_eq!(config.tab_width, 4);
        assert!(dir.join("config.toml").exists());
        assert!(dir.join("keybindings.toml").exists());
        assert!(dir.join("themes").is_dir());
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn parses_partial_config() {
        let parsed: AppConfig = toml::from_str(
            r#"
default_mode = "source"
tab_width = 8
line_numbers = true
"#,
        )
        .expect("config");

        assert_eq!(parsed.theme, "dark");
        assert_eq!(parsed.default_mode, DefaultMode::Source);
        assert_eq!(parsed.tab_width, 8);
        assert!(parsed.line_numbers);
        assert!(parsed.wrap);
        assert!(!parsed.auto_save);
    }

    #[test]
    fn zero_tab_width_falls_back_to_one() {
        let parsed: AppConfig = toml::from_str(r#"tab_width = 0"#).expect("config");

        assert_eq!(parsed.tab_width(), 1);
    }

    #[test]
    fn save_omits_default_values() {
        let config = AppConfig::default();

        assert_eq!(config.to_toml(), "# implicit uses built-in defaults\n");
    }

    #[test]
    fn save_only_writes_changed_values() {
        let config = AppConfig {
            theme: String::from("gruvbox"),
            line_numbers: true,
            ..AppConfig::default()
        };

        assert_eq!(
            config.to_toml(),
            "theme = \"gruvbox\"\nline_numbers = true\n"
        );
    }
}
