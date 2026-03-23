use std::{
    env, fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

const MAX_RECENTS: usize = 10;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentFile {
    path: PathBuf,
    last_opened: u64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct RecentsDocument {
    #[serde(default)]
    files: Vec<RecentRecord>,
}

#[derive(Debug, Serialize, Deserialize)]
struct RecentRecord {
    path: String,
    last_opened: u64,
}

pub fn load() -> Result<Vec<RecentFile>> {
    load_from(&recents_path())
}

pub fn remember(path: &Path) -> Result<()> {
    remember_in(&recents_path(), path)
}

impl RecentFile {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn display_path(&self) -> String {
        let parts: Vec<&str> = self
            .path
            .components()
            .filter_map(|c| match c {
                std::path::Component::Normal(s) => s.to_str(),
                _ => None,
            })
            .collect();
        let tail = if parts.len() > 3 {
            &parts[parts.len() - 3..]
        } else {
            &parts[..]
        };
        tail.join(" ❯ ")
    }

    pub fn relative_age(&self) -> String {
        let opened = UNIX_EPOCH + Duration::from_secs(self.last_opened);
        let age = SystemTime::now()
            .duration_since(opened)
            .unwrap_or(Duration::from_secs(0));

        if age.as_secs() < 60 {
            format!("{}s ago", age.as_secs())
        } else if age.as_secs() < 3_600 {
            format!("{}m ago", age.as_secs() / 60)
        } else if age.as_secs() < 86_400 {
            format!("{}h ago", age.as_secs() / 3_600)
        } else {
            format!("{}d ago", age.as_secs() / 86_400)
        }
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(path: PathBuf, last_opened: u64) -> Self {
        Self { path, last_opened }
    }
}

fn recents_path() -> PathBuf {
    config_dir().join("recents.toml")
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

fn load_from(path: &Path) -> Result<Vec<RecentFile>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let text =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    let document: RecentsDocument =
        toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;

    let mut files = document
        .files
        .into_iter()
        .map(|record| RecentFile {
            path: PathBuf::from(record.path),
            last_opened: record.last_opened,
        })
        .collect::<Vec<_>>();

    files.sort_by(|left, right| right.last_opened.cmp(&left.last_opened));
    Ok(files)
}

fn remember_in(store_path: &Path, file_path: &Path) -> Result<()> {
    let mut files = load_from(store_path)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs();

    files.retain(|entry| entry.path != file_path);
    files.insert(
        0,
        RecentFile {
            path: file_path.to_path_buf(),
            last_opened: now,
        },
    );
    files.truncate(MAX_RECENTS);

    let document = RecentsDocument {
        files: files
            .into_iter()
            .map(|entry| RecentRecord {
                path: entry.path.display().to_string(),
                last_opened: entry.last_opened,
            })
            .collect(),
    };

    if let Some(parent) = store_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    let text = toml::to_string_pretty(&document)?;
    fs::write(store_path, text)
        .with_context(|| format!("failed to write {}", store_path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let unique = format!(
            "{}-{}",
            name,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or(Duration::from_secs(0))
                .as_nanos()
        );
        env::temp_dir().join(unique).join("recents.toml")
    }

    #[test]
    fn remember_puts_newest_entry_first() {
        let store_path = temp_file("remember-puts-newest-first");
        let older = PathBuf::from("/tmp/older.md");
        let newer = PathBuf::from("/tmp/newer.md");

        remember_in(&store_path, &older).expect("write older recent");
        remember_in(&store_path, &newer).expect("write newer recent");

        let recents = load_from(&store_path).expect("load recents");
        assert_eq!(recents[0].path(), newer);
        assert_eq!(recents[1].path(), older);
    }

    #[test]
    fn remember_deduplicates_existing_paths() {
        let store_path = temp_file("remember-deduplicates");
        let file = PathBuf::from("/tmp/file.md");

        remember_in(&store_path, &file).expect("initial write");
        remember_in(&store_path, &file).expect("duplicate write");

        let recents = load_from(&store_path).expect("load recents");
        assert_eq!(recents.len(), 1);
        assert_eq!(recents[0].path(), file);
    }
}
