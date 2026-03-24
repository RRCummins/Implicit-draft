use std::{
    env, fs,
    path::{Component, Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LineChange {
    Added,
    Modified,
    Deleted,
}

pub fn markers_for_buffer(path: Option<&Path>, lines: &[String]) -> Vec<Option<LineChange>> {
    let mut markers = vec![None; lines.len()];
    let Some(path) = path else {
        return markers;
    };

    let resolved_path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let Some(repo_root) = repo_root_for(&resolved_path) else {
        return markers;
    };
    let resolved_root = fs::canonicalize(&repo_root).unwrap_or(repo_root);
    let Ok(relative_path) = resolved_path.strip_prefix(&resolved_root) else {
        return markers;
    };

    let Some(base_lines) = load_head_lines(&resolved_root, relative_path) else {
        if path.exists() {
            markers.fill(Some(LineChange::Added));
        }
        return markers;
    };

    apply_diff_markers(&mut markers, &base_lines, lines);
    markers
}

fn apply_diff_markers(
    markers: &mut [Option<LineChange>],
    base_lines: &[String],
    current_lines: &[String],
) {
    let diff = match unified_diff(base_lines, current_lines) {
        Some(diff) => diff,
        None => return,
    };

    for line in diff.lines() {
        let Some(hunk) = line.strip_prefix("@@ ") else {
            continue;
        };
        let Some((old_range, new_range)) = parse_hunk_ranges(hunk) else {
            continue;
        };

        if new_range.1 == 0 {
            if markers.is_empty() {
                continue;
            }
            let row = new_range
                .0
                .saturating_sub(1)
                .min(markers.len().saturating_sub(1));
            merge_marker(&mut markers[row], LineChange::Deleted);
            continue;
        }

        let marker = if old_range.1 == 0 {
            LineChange::Added
        } else {
            LineChange::Modified
        };
        let start = new_range.0.saturating_sub(1);
        let end = start.saturating_add(new_range.1).min(markers.len());
        for slot in markers.iter_mut().take(end).skip(start) {
            merge_marker(slot, marker);
        }
    }
}

fn merge_marker(slot: &mut Option<LineChange>, next: LineChange) {
    *slot = Some(match (*slot, next) {
        (Some(LineChange::Modified), _) | (_, LineChange::Modified) => LineChange::Modified,
        (Some(LineChange::Deleted), _) | (_, LineChange::Deleted) => LineChange::Deleted,
        _ => LineChange::Added,
    });
}

fn unified_diff(base_lines: &[String], current_lines: &[String]) -> Option<String> {
    let tmp_dir = env::temp_dir().join(format!(
        "implicit-gitdiff-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos()
    ));
    fs::create_dir_all(&tmp_dir).ok()?;

    let base_path = tmp_dir.join("base.tmp");
    let current_path = tmp_dir.join("current.tmp");
    fs::write(&base_path, base_lines.join("\n")).ok()?;
    fs::write(&current_path, current_lines.join("\n")).ok()?;

    let output = Command::new("git")
        .args(["diff", "--no-index", "--unified=0", "--no-color", "--"])
        .arg(&base_path)
        .arg(&current_path)
        .output()
        .ok()?;

    let _ = fs::remove_file(&base_path);
    let _ = fs::remove_file(&current_path);
    let _ = fs::remove_dir(&tmp_dir);

    if !(output.status.success() || output.status.code() == Some(1)) {
        return None;
    }

    String::from_utf8(output.stdout).ok()
}

fn parse_hunk_ranges(hunk: &str) -> Option<((usize, usize), (usize, usize))> {
    let ranges = hunk
        .trim()
        .trim_start_matches("@@")
        .trim_end_matches("@@")
        .trim();
    let mut parts = ranges.split_whitespace();
    let old_range = parse_hunk_range(parts.next()?)?;
    let new_range = parse_hunk_range(parts.next()?)?;
    Some((old_range, new_range))
}

fn parse_hunk_range(range: &str) -> Option<(usize, usize)> {
    let range = range.strip_prefix(['-', '+'])?;
    let (start, count) = match range.split_once(',') {
        Some((start, count)) => (start.parse().ok()?, count.parse().ok()?),
        None => (range.parse().ok()?, 1),
    };
    Some((start, count))
}

fn repo_root_for(path: &Path) -> Option<PathBuf> {
    let cwd = path.parent().unwrap_or_else(|| Path::new("."));
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(cwd)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let root = String::from_utf8(output.stdout).ok()?;
    let root = root.trim();
    (!root.is_empty()).then(|| PathBuf::from(root))
}

fn load_head_lines(repo_root: &Path, relative_path: &Path) -> Option<Vec<String>> {
    let git_path = git_relative_path(relative_path);
    let output = Command::new("git")
        .args(["show", &format!("HEAD:{git_path}")])
        .current_dir(repo_root)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let text = String::from_utf8(output.stdout).ok()?;
    Some(split_lines(&text))
}

fn split_lines(text: &str) -> Vec<String> {
    let mut lines = text
        .split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn git_relative_path(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hunk_ranges_with_counts() {
        let ranges = parse_hunk_ranges("@@ -4,0 +5,3 @@").expect("ranges");
        assert_eq!(ranges, ((4, 0), (5, 3)));
    }

    #[test]
    fn parses_hunk_ranges_without_counts() {
        let ranges = parse_hunk_ranges("@@ -7 +8 @@").expect("ranges");
        assert_eq!(ranges, ((7, 1), (8, 1)));
    }

    #[test]
    fn marks_added_and_modified_rows() {
        let mut markers = vec![None; 4];
        apply_diff_markers(
            &mut markers,
            &[String::from("a"), String::from("b"), String::from("d")],
            &[
                String::from("a"),
                String::from("b changed"),
                String::from("c"),
                String::from("d"),
            ],
        );

        assert_eq!(
            markers,
            vec![
                None,
                Some(LineChange::Modified),
                Some(LineChange::Modified),
                None,
            ]
        );
    }

    #[test]
    fn marks_deleted_rows() {
        let mut markers = vec![None; 3];
        apply_diff_markers(
            &mut markers,
            &[
                String::from("a"),
                String::from("b"),
                String::from("c"),
                String::new(),
            ],
            &[String::from("a"), String::from("c"), String::new()],
        );

        assert_eq!(markers, vec![Some(LineChange::Deleted), None, None]);
    }

    #[test]
    fn marks_deleted_rows_at_end_of_file() {
        let mut markers = vec![None; 3];
        apply_diff_markers(
            &mut markers,
            &[
                String::from("a"),
                String::from("b"),
                String::from("c"),
                String::new(),
            ],
            &[String::from("a"), String::from("b"), String::new()],
        );

        assert_eq!(markers, vec![None, Some(LineChange::Deleted), None]);
    }
}
