use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct SessionState {
    pub sidebar_open: bool,
    pub sidebar_width: u16,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            sidebar_open: false,
            sidebar_width: 22,
        }
    }
}

impl SessionState {
    pub fn load() -> Result<Self> {
        let path = session_path(&config_dir());
        if !path.exists() {
            return Ok(Self::default());
        }
        Self::load_from_path(&path)
    }

    fn load_from_path(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let parsed =
            toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(parsed)
    }

    fn save_to_dir(&self, dir: &Path) -> Result<()> {
        fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;
        let path = session_path(dir);
        let text = toml::to_string(self).context("failed to encode session")?;
        fs::write(&path, text).with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    pub fn save(&self) -> Result<()> {
        let dir = config_dir();
        self.save_to_dir(&dir)
    }
}

fn session_path(dir: &Path) -> PathBuf {
    dir.join("session.toml")
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_config_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("implicit-session-{name}-{unique}"))
    }

    #[test]
    fn missing_session_uses_defaults() {
        let dir = temp_config_dir("default");
        let session = SessionState::load_from_path(&session_path(&dir)).unwrap_or_default();

        assert_eq!(session, SessionState::default());
    }

    #[test]
    fn saves_and_loads_sidebar_state() {
        let dir = temp_config_dir("roundtrip");

        let session = SessionState {
            sidebar_open: true,
            sidebar_width: 30,
        };
        session.save_to_dir(&dir).expect("save session");

        let loaded = SessionState::load_from_path(&session_path(&dir)).expect("load session");
        assert_eq!(loaded, session);

        fs::remove_dir_all(dir).expect("cleanup");
    }
}
