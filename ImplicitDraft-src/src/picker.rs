use std::{
    cmp::Ordering,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use anyhow::{Context, Result};

use crate::filetype;

#[derive(Debug)]
pub struct Picker {
    cwd: PathBuf,
    entries: Vec<PickerEntry>,
    selected: usize,
    scroll: usize,
    show_all: bool,
    query: String,
}

#[derive(Clone, Debug)]
pub struct PickerEntry {
    path: PathBuf,
    label: String,
    is_dir: bool,
    size: Option<u64>,
    modified: Option<SystemTime>,
}

#[derive(Debug)]
pub enum PickerAction {
    None,
    OpenFile(PathBuf),
}

impl Picker {
    pub fn new(cwd: PathBuf) -> Result<Self> {
        let mut picker = Self {
            cwd,
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
            show_all: false,
            query: String::new(),
        };
        picker.reload()?;
        Ok(picker)
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        if self.selected + 1 < self.entries.len() {
            self.selected += 1;
        }
    }

    pub fn page_up(&mut self, height: usize) {
        let step = height.max(1);
        self.selected = self.selected.saturating_sub(step);
    }

    pub fn page_down(&mut self, height: usize) {
        let step = height.max(1);
        let max_index = self.entries.len().saturating_sub(1);
        self.selected = (self.selected + step).min(max_index);
    }

    pub fn toggle_show_all(&mut self) -> Result<()> {
        self.show_all = !self.show_all;
        self.reload()
    }

    pub fn append_query(&mut self, ch: char) -> Result<()> {
        self.query.push(ch);
        self.reload()
    }

    pub fn pop_query(&mut self) -> Result<()> {
        self.query.pop();
        self.reload()
    }

    pub fn clear_query(&mut self) -> Result<()> {
        self.query.clear();
        self.reload()
    }

    pub fn open_selected(&mut self) -> Result<PickerAction> {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return Ok(PickerAction::None);
        };

        if entry.is_dir {
            self.cwd = entry.path;
            self.reload()?;
            return Ok(PickerAction::None);
        }

        Ok(PickerAction::OpenFile(entry.path))
    }

    pub fn go_parent(&mut self) -> Result<()> {
        let Some(parent) = self.cwd.parent() else {
            return Ok(());
        };

        self.cwd = parent.to_path_buf();
        self.reload()
    }

    pub fn sync_viewport(&mut self, height: usize) {
        if height == 0 {
            return;
        }

        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + height {
            self.scroll = self.selected + 1 - height;
        }
    }

    pub fn visible_entries(&self, height: usize) -> Vec<PickerEntry> {
        let end = (self.scroll + height).min(self.entries.len());
        self.entries[self.scroll..end].to_vec()
    }

    pub fn selected_screen_row(&self) -> Option<usize> {
        if self.selected < self.scroll {
            return None;
        }

        Some(self.selected - self.scroll)
    }

    pub fn cwd_display(&self) -> String {
        self.cwd.display().to_string()
    }

    pub fn filter_label(&self) -> &'static str {
        if self.show_all {
            "all files"
        } else {
            "notes/code"
        }
    }

    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn selected_entry(&self) -> Option<&PickerEntry> {
        self.entries.get(self.selected)
    }

    fn reload(&mut self) -> Result<()> {
        let mut entries = Vec::new();

        for item in fs::read_dir(&self.cwd)
            .with_context(|| format!("failed to read {}", self.cwd.display()))?
        {
            let item = item?;
            let path = item.path();
            let metadata = item.metadata()?;
            let is_dir = metadata.is_dir();

            if !is_dir && !self.show_all && !is_supported_candidate(&path) {
                continue;
            }

            let name = item.file_name();
            let mut label = name.to_string_lossy().into_owned();
            if is_dir {
                label.push('/');
            }

            if !self.query.is_empty() && !matches_query(&label, &self.query) {
                continue;
            }

            entries.push(PickerEntry {
                path,
                label,
                is_dir,
                size: (!is_dir).then_some(metadata.len()),
                modified: metadata.modified().ok(),
            });
        }

        entries.sort_by(compare_entries);
        self.entries = entries;
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
        self.scroll = 0;
        Ok(())
    }
}

impl PickerEntry {
    pub fn label(&self) -> &str {
        &self.label
    }

    pub fn metadata_lines(&self) -> [String; 4] {
        [
            format!("path: {}", self.path.display()),
            format!("type: {}", if self.is_dir { "directory" } else { "file" }),
            format!("size: {}", format_size(self.size)),
            format!("modified: {}", format_modified(self.modified)),
        ]
    }
}

fn is_supported_candidate(path: &Path) -> bool {
    filetype::is_supported(path)
}

fn compare_entries(left: &PickerEntry, right: &PickerEntry) -> Ordering {
    right
        .is_dir
        .cmp(&left.is_dir)
        .then_with(|| left.label.to_lowercase().cmp(&right.label.to_lowercase()))
}

fn format_size(size: Option<u64>) -> String {
    let Some(size) = size else {
        return "-".to_owned();
    };

    match size {
        0..=1023 => format!("{size} B"),
        1024..=1_048_575 => format!("{:.1} KB", size as f64 / 1024.0),
        1_048_576..=1_073_741_823 => format!("{:.1} MB", size as f64 / 1_048_576.0),
        _ => format!("{:.1} GB", size as f64 / 1_073_741_824.0),
    }
}

fn format_modified(modified: Option<SystemTime>) -> String {
    let Some(modified) = modified else {
        return "-".to_owned();
    };

    let age = SystemTime::now()
        .duration_since(modified)
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

fn matches_query(label: &str, query: &str) -> bool {
    let mut query_chars = query.chars().flat_map(char::to_lowercase);
    let mut current = query_chars.next();

    if current.is_none() {
        return true;
    }

    for ch in label.chars().flat_map(char::to_lowercase) {
        if Some(ch) == current {
            current = query_chars.next();
            if current.is_none() {
                return true;
            }
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_directories_before_files() {
        let dir = PickerEntry {
            path: PathBuf::from("notes"),
            label: "notes/".to_owned(),
            is_dir: true,
            size: None,
            modified: None,
        };
        let file = PickerEntry {
            path: PathBuf::from("alpha.md"),
            label: "alpha.md".to_owned(),
            is_dir: false,
            size: Some(10),
            modified: None,
        };

        assert_eq!(compare_entries(&dir, &file), Ordering::Less);
    }

    #[test]
    fn filters_supported_editable_extensions() {
        assert!(is_supported_candidate(Path::new("note.md")));
        assert!(is_supported_candidate(Path::new("note.txt")));
        assert!(is_supported_candidate(Path::new("main.swift")));
        assert!(is_supported_candidate(Path::new("Program.cs")));
        assert!(is_supported_candidate(Path::new("Dockerfile")));
        assert!(!is_supported_candidate(Path::new("image.png")));
    }

    #[test]
    fn query_filters_entries() {
        assert!(matches_query("alpha.md", "ap"));
        assert!(matches_query("project_notes.md", "pn"));
        assert!(!matches_query("beta.md", "az"));
    }
}
