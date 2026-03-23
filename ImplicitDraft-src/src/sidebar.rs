use std::{
    cmp::Ordering,
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result};

const DEFAULT_WIDTH: u16 = 22;
const MIN_WIDTH: u16 = 14;
const MAX_WIDTH: u16 = 40;

#[derive(Clone, Debug)]
pub struct SidebarRow {
    pub label: String,
    pub marker: Option<char>,
}

#[derive(Debug)]
pub struct SidebarState {
    root: PathBuf,
    entries: Vec<SidebarEntry>,
    selected: usize,
    scroll: usize,
    expanded: BTreeSet<PathBuf>,
    git_statuses: BTreeMap<PathBuf, GitMarker>,
    open: bool,
    width: u16,
}

#[derive(Debug, Default)]
struct IgnoreFilter {
    exact: BTreeSet<PathBuf>,
    directories: Vec<PathBuf>,
}

#[derive(Clone, Debug)]
struct SidebarEntry {
    path: PathBuf,
    depth: usize,
    label: String,
    is_dir: bool,
    expanded: bool,
    marker: Option<GitMarker>,
}

#[derive(Debug)]
pub enum SidebarAction {
    None,
    OpenFile(PathBuf),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum GitMarker {
    Modified,
    Added,
    Untracked,
}

impl SidebarState {
    pub fn for_file(path: Option<&Path>) -> Result<Self> {
        let root = match path.and_then(Path::parent) {
            Some(parent) => parent.to_path_buf(),
            None => env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };
        let mut sidebar = Self::new(root)?;
        if let Some(path) = path {
            sidebar.select_path(path);
        }
        Ok(sidebar)
    }

    pub fn new(root: PathBuf) -> Result<Self> {
        let mut expanded = BTreeSet::new();
        expanded.insert(root.clone());
        let mut sidebar = Self {
            root,
            entries: Vec::new(),
            selected: 0,
            scroll: 0,
            expanded,
            git_statuses: BTreeMap::new(),
            open: false,
            width: DEFAULT_WIDTH,
        };
        sidebar.refresh()?;
        Ok(sidebar)
    }

    pub fn fallback() -> Self {
        let root = env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let label = root_label(&root);
        Self {
            root: root.clone(),
            entries: vec![SidebarEntry {
                path: root.clone(),
                depth: 0,
                label,
                is_dir: true,
                expanded: true,
                marker: None,
            }],
            selected: 0,
            scroll: 0,
            expanded: BTreeSet::from([root]),
            git_statuses: BTreeMap::new(),
            open: false,
            width: DEFAULT_WIDTH,
        }
    }

    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn open(&mut self) -> Result<()> {
        self.open = true;
        self.refresh()
    }

    pub fn close(&mut self) {
        self.open = false;
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn set_width(&mut self, width: u16) {
        self.width = width.clamp(MIN_WIDTH, MAX_WIDTH);
    }

    pub fn resize_narrower(&mut self) {
        self.width = self.width.saturating_sub(2).max(MIN_WIDTH);
    }

    pub fn resize_wider(&mut self) {
        self.width = (self.width + 2).min(MAX_WIDTH);
    }

    pub fn refresh(&mut self) -> Result<()> {
        let selected_path = self.selected_path().cloned();
        self.git_statuses = load_git_statuses(&self.root);
        let ignored = load_ignored_paths(&self.root);
        self.entries = build_entries(&self.root, &self.expanded, &self.git_statuses, &ignored)?;

        if self.entries.is_empty() {
            self.selected = 0;
            self.scroll = 0;
            return Ok(());
        }

        if let Some(path) = selected_path {
            self.select_path(&path);
        }
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
        self.scroll = self.scroll.min(self.selected);
        Ok(())
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

    pub fn visible_rows(&self, height: usize) -> Vec<SidebarRow> {
        let end = (self.scroll + height).min(self.entries.len());
        self.entries[self.scroll..end]
            .iter()
            .map(|entry| SidebarRow {
                label: entry.render_label(),
                marker: entry.marker.map(GitMarker::symbol),
            })
            .collect()
    }

    pub fn selected_row(&self) -> Option<usize> {
        if self.selected < self.scroll {
            return None;
        }
        Some(self.selected - self.scroll)
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
        self.selected = self.selected.saturating_sub(height.max(1));
    }

    pub fn page_down(&mut self, height: usize) {
        let max_index = self.entries.len().saturating_sub(1);
        self.selected = (self.selected + height.max(1)).min(max_index);
    }

    pub fn open_selected(&mut self) -> Result<SidebarAction> {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return Ok(SidebarAction::None);
        };

        if entry.is_dir {
            self.toggle_selected_dir()?;
            return Ok(SidebarAction::None);
        }

        Ok(SidebarAction::OpenFile(entry.path))
    }

    pub fn toggle_selected_dir(&mut self) -> Result<()> {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return Ok(());
        };

        if !entry.is_dir {
            return Ok(());
        }

        if entry.expanded {
            self.expanded.remove(&entry.path);
        } else {
            self.expanded.insert(entry.path);
        }

        self.refresh()
    }

    pub fn move_left(&mut self) -> Result<()> {
        let Some(entry) = self.entries.get(self.selected).cloned() else {
            return Ok(());
        };

        if entry.is_dir && entry.expanded && entry.path != self.root {
            self.expanded.remove(&entry.path);
            self.refresh()?;
            return Ok(());
        }

        let target_depth = entry.depth.saturating_sub(1);
        for index in (0..self.selected).rev() {
            if self.entries[index].depth == target_depth {
                self.selected = index;
                break;
            }
        }

        Ok(())
    }

    pub fn root_display(&self) -> String {
        self.root.display().to_string()
    }

    pub fn select_path(&mut self, path: &Path) {
        if let Some(index) = self.entries.iter().position(|entry| entry.path == path) {
            self.selected = index;
        }
    }

    fn selected_path(&self) -> Option<&PathBuf> {
        self.entries.get(self.selected).map(|entry| &entry.path)
    }
}

impl SidebarEntry {
    fn render_label(&self) -> String {
        let indent = "  ".repeat(self.depth);
        let prefix = if self.is_dir {
            if self.expanded { "▾ " } else { "▸ " }
        } else {
            "  "
        };
        format!("{indent}{prefix}{}", self.label)
    }
}

impl GitMarker {
    fn symbol(self) -> char {
        match self {
            Self::Modified => 'M',
            Self::Added => 'A',
            Self::Untracked => '?',
        }
    }
}

impl IgnoreFilter {
    fn contains(&self, path: &Path) -> bool {
        self.exact.contains(path)
            || self
                .directories
                .iter()
                .any(|directory| path.starts_with(directory))
    }
}

fn build_entries(
    root: &Path,
    expanded: &BTreeSet<PathBuf>,
    statuses: &BTreeMap<PathBuf, GitMarker>,
    ignored: &IgnoreFilter,
) -> Result<Vec<SidebarEntry>> {
    let mut entries = vec![SidebarEntry {
        path: root.to_path_buf(),
        depth: 0,
        label: root_label(root),
        is_dir: true,
        expanded: expanded.contains(root),
        marker: None,
    }];

    if expanded.contains(root) {
        append_children(&mut entries, root, expanded, statuses, ignored, 1)?;
    }

    Ok(entries)
}

fn append_children(
    entries: &mut Vec<SidebarEntry>,
    directory: &Path,
    expanded: &BTreeSet<PathBuf>,
    statuses: &BTreeMap<PathBuf, GitMarker>,
    ignored: &IgnoreFilter,
    depth: usize,
) -> Result<()> {
    let mut children = Vec::new();

    for entry in fs::read_dir(directory)
        .with_context(|| format!("failed to read {}", directory.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if ignored.contains(&path) {
            continue;
        }
        let metadata = entry.metadata()?;
        let is_dir = metadata.is_dir();
        let name = entry.file_name().to_string_lossy().into_owned();
        children.push((path, name, is_dir));
    }

    children.sort_by(|left, right| compare_entries((&left.1, left.2), (&right.1, right.2)));

    for (path, name, is_dir) in children {
        let is_expanded = is_dir && expanded.contains(&path);
        entries.push(SidebarEntry {
            path: path.clone(),
            depth,
            label: if is_dir { format!("{name}/") } else { name },
            is_dir,
            expanded: is_expanded,
            marker: statuses.get(&path).copied(),
        });

        if is_expanded {
            append_children(entries, &path, expanded, statuses, ignored, depth + 1)?;
        }
    }

    Ok(())
}

fn compare_entries(left: (&str, bool), right: (&str, bool)) -> Ordering {
    right
        .1
        .cmp(&left.1)
        .then_with(|| left.0.to_lowercase().cmp(&right.0.to_lowercase()))
}

fn load_git_statuses(root: &Path) -> BTreeMap<PathBuf, GitMarker> {
    let output = match Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=all"])
        .current_dir(root)
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return BTreeMap::new(),
    };

    let mut statuses = BTreeMap::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.len() < 4 {
            continue;
        }

        let status = &line[..2];
        let path = line[3..].trim();
        let path = path
            .rsplit_once(" -> ")
            .map(|(_, renamed)| renamed)
            .unwrap_or(path)
            .trim_matches('"');
        if path.is_empty() {
            continue;
        }

        let marker = if status == "??" {
            Some(GitMarker::Untracked)
        } else if status.contains('A') {
            Some(GitMarker::Added)
        } else if status.contains('M') {
            Some(GitMarker::Modified)
        } else {
            None
        };

        if let Some(marker) = marker {
            statuses.insert(root.join(path), marker);
        }
    }

    statuses
}

fn load_ignored_paths(root: &Path) -> IgnoreFilter {
    let output = match Command::new("git")
        .args([
            "ls-files",
            "--others",
            "--ignored",
            "--exclude-standard",
            "--directory",
        ])
        .current_dir(root)
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return IgnoreFilter::default(),
    };

    let mut ignored = IgnoreFilter::default();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        let path = line.trim().trim_matches('"');
        if path.is_empty() {
            continue;
        }

        if let Some(directory) = path.strip_suffix('/') {
            ignored.directories.push(root.join(directory));
        } else {
            ignored.exact.insert(root.join(path));
        }
    }

    ignored
}

fn root_label(root: &Path) -> String {
    match root.file_name().and_then(|name| name.to_str()) {
        Some(name) => format!("{name}/"),
        None => format!("{}/", root.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("implicit-sidebar-{name}-{unique}"))
    }

    fn git(root: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(root)
            .env("GIT_AUTHOR_NAME", "Implicit")
            .env("GIT_AUTHOR_EMAIL", "implicit@example.com")
            .env("GIT_COMMITTER_NAME", "Implicit")
            .env("GIT_COMMITTER_EMAIL", "implicit@example.com")
            .status()
            .expect("git command");
        assert!(status.success(), "git {:?} failed", args);
    }

    #[test]
    fn builds_rows_for_root_and_children() {
        let root = temp_dir("rows");
        fs::create_dir_all(root.join("src")).expect("mkdir");
        fs::write(root.join("Cargo.toml"), "[package]\nname = \"x\"\n").expect("file");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("file");

        let sidebar = SidebarState::new(root.clone()).expect("sidebar");
        let rows = sidebar.visible_rows(10);

        assert!(rows[0].label.contains(&root_label(&root)));
        assert!(rows.iter().any(|row| row.label.contains("src/")));
        assert!(rows.iter().any(|row| row.label.contains("Cargo.toml")));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn toggling_directory_reveals_nested_entries() {
        let root = temp_dir("toggle");
        fs::create_dir_all(root.join("src")).expect("mkdir");
        fs::write(root.join("src/main.rs"), "fn main() {}\n").expect("file");

        let mut sidebar = SidebarState::new(root.clone()).expect("sidebar");
        sidebar.move_down();
        sidebar.toggle_selected_dir().expect("toggle");

        let rows = sidebar.visible_rows(10);
        assert!(rows.iter().any(|row| row.label.contains("main.rs")));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn maps_git_status_markers_for_file_rows() {
        let root = temp_dir("git-markers");
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("tracked.md"), "first\n").expect("file");
        git(&root, &["init"]);
        git(&root, &["add", "tracked.md"]);
        git(&root, &["commit", "-m", "init"]);

        fs::write(root.join("tracked.md"), "changed\n").expect("file");
        fs::write(root.join("new.md"), "new\n").expect("file");

        let sidebar = SidebarState::new(root.clone()).expect("sidebar");
        let rows = sidebar.visible_rows(10);

        let tracked = rows
            .iter()
            .find(|row| row.label.contains("tracked.md"))
            .expect("tracked row");
        let new_file = rows
            .iter()
            .find(|row| row.label.contains("new.md"))
            .expect("new row");

        assert_eq!(tracked.marker, Some('M'));
        assert_eq!(new_file.marker, Some('?'));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn hides_gitignored_entries() {
        let root = temp_dir("gitignore");
        fs::create_dir_all(root.join("visible")).expect("mkdir");
        fs::create_dir_all(root.join("ignored-dir")).expect("mkdir");
        fs::write(root.join(".gitignore"), "ignored-dir/\n*.log\n").expect("ignore file");
        fs::write(root.join("visible/notes.md"), "visible\n").expect("visible file");
        fs::write(root.join("ignored-dir/secret.md"), "ignored\n").expect("ignored file");
        fs::write(root.join("debug.log"), "ignored\n").expect("ignored file");
        git(&root, &["init"]);

        let mut sidebar = SidebarState::new(root.clone()).expect("sidebar");
        sidebar.select_path(&root.join("visible"));
        sidebar.toggle_selected_dir().expect("toggle");
        let rows = sidebar.visible_rows(20);

        assert!(rows.iter().any(|row| row.label.contains("visible/")));
        assert!(rows.iter().any(|row| row.label.contains("notes.md")));
        assert!(!rows.iter().any(|row| row.label.contains("ignored-dir/")));
        assert!(!rows.iter().any(|row| row.label.contains("secret.md")));
        assert!(!rows.iter().any(|row| row.label.contains("debug.log")));

        fs::remove_dir_all(root).expect("cleanup");
    }
}
