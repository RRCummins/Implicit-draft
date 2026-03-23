//! Loads editor configuration from `~/.config/implicit/config.toml`.

use std::{env, fs, path::PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub theme: String,
    pub default_mode: DefaultMode,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DefaultMode {
    #[default]
    SourceHints,
    Preview,
    Source,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: String::from("dark"),
            default_mode: DefaultMode::SourceHints,
        }
    }
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let path = config_path();
        if !path.exists() {
            return Ok(Self::default());
        }

        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))
    }
}

fn config_path() -> PathBuf {
    let mut path = config_dir();
    path.push("config.toml");
    path
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_when_config_file_is_missing() {
        assert_eq!(AppConfig::default().theme, "dark");
        assert_eq!(AppConfig::default().default_mode, DefaultMode::SourceHints);
    }

    #[test]
    fn parses_partial_config() {
        let parsed: AppConfig = toml::from_str(r#"default_mode = "source""#).expect("config");

        assert_eq!(parsed.theme, "dark");
        assert_eq!(parsed.default_mode, DefaultMode::Source);
    }
}
