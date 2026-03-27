use std::{fs, path::Path};

use anyhow::{Context, Result};
use regex::{Regex, RegexBuilder};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SearchMatch {
    pub row: usize,
    pub col: usize,
    pub len: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BufferViewState {
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub desired_col: usize,
    pub scroll_row: usize,
    pub scroll_col: usize,
}

#[derive(Clone, Debug)]
struct Snapshot {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    desired_col: usize,
    scroll_row: usize,
    scroll_col: usize,
    dirty: bool,
}

#[derive(Debug)]
pub struct Buffer {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    desired_col: usize,
    scroll_row: usize,
    scroll_col: usize,
    dirty: bool,
    undo_stack: Vec<Snapshot>,
    redo_stack: Vec<Snapshot>,
}

impl Buffer {
    pub fn empty() -> Self {
        Self {
            lines: vec![String::new()],
            cursor_row: 0,
            cursor_col: 0,
            desired_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            dirty: false,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
        }
    }

    pub fn from_path(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;

        Ok(Self::from_text(&contents))
    }

    pub fn from_text(text: &str) -> Self {
        let mut lines = text
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line).to_owned())
            .collect::<Vec<_>>();

        if lines.is_empty() {
            lines.push(String::new());
        }

        Self {
            lines,
            ..Self::empty()
        }
    }

    pub fn save_to_path(&mut self, path: &Path) -> Result<()> {
        fs::write(path, self.lines.join("\n"))
            .with_context(|| format!("failed to write {}", path.display()))?;
        self.dirty = false;
        Ok(())
    }

    pub fn sync_viewport(&mut self, height: usize, width: usize) {
        if height == 0 || width == 0 {
            return;
        }

        if self.cursor_row < self.scroll_row {
            self.scroll_row = self.cursor_row;
        } else if self.cursor_row >= self.scroll_row + height {
            self.scroll_row = self.cursor_row + 1 - height;
        }

        if self.cursor_col < self.scroll_col {
            self.scroll_col = self.cursor_col;
        } else if self.cursor_col >= self.scroll_col + width {
            self.scroll_col = self.cursor_col + 1 - width;
        }
    }

    pub fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.current_line_len();
        }

        self.desired_col = self.cursor_col;
    }

    pub fn move_right(&mut self) {
        if self.cursor_col < self.current_line_len() {
            self.cursor_col += 1;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }

        self.desired_col = self.cursor_col;
    }

    pub fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.desired_col.min(self.current_line_len());
        }
    }

    pub fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = self.desired_col.min(self.current_line_len());
        }
    }

    pub fn move_home(&mut self) {
        self.cursor_col = 0;
        self.desired_col = 0;
    }

    pub fn move_end(&mut self) {
        self.cursor_col = self.current_line_len();
        self.desired_col = self.cursor_col;
    }

    pub fn move_doc_start(&mut self) {
        self.cursor_row = 0;
        self.cursor_col = 0;
        self.desired_col = 0;
    }

    pub fn move_doc_end(&mut self) {
        self.cursor_row = self.lines.len().saturating_sub(1);
        self.cursor_col = self.current_line_len();
        self.desired_col = self.cursor_col;
    }

    pub fn goto_line(&mut self, line_number: usize) -> bool {
        if line_number == 0 || line_number > self.lines.len() {
            return false;
        }

        self.cursor_row = line_number - 1;
        self.cursor_col = 0;
        self.desired_col = 0;
        true
    }

    pub fn page_up(&mut self, height: usize) {
        let step = height.max(1);
        self.cursor_row = self.cursor_row.saturating_sub(step);
        self.cursor_col = self.desired_col.min(self.current_line_len());
    }

    pub fn page_down(&mut self, height: usize) {
        let step = height.max(1);
        let max_row = self.lines.len().saturating_sub(1);
        self.cursor_row = (self.cursor_row + step).min(max_row);
        self.cursor_col = self.desired_col.min(self.current_line_len());
    }

    pub fn insert_char(&mut self, ch: char) {
        self.record_edit(|buffer| {
            buffer.insert_char_raw(ch);
            true
        });
    }

    pub fn insert_spaces(&mut self, count: usize) {
        self.record_edit(|buffer| {
            if count == 0 {
                return false;
            }

            for _ in 0..count {
                buffer.insert_char_raw(' ');
            }

            true
        });
    }

    pub fn insert_newline(&mut self) {
        self.record_edit(|buffer| {
            buffer.insert_newline_raw();
            true
        });
    }

    pub fn backspace(&mut self) {
        self.record_edit(Self::backspace_raw);
    }

    pub fn delete_forward(&mut self) {
        self.record_edit(Self::delete_forward_raw);
    }

    pub fn undo(&mut self) -> bool {
        let Some(snapshot) = self.undo_stack.pop() else {
            return false;
        };

        self.redo_stack.push(self.snapshot());
        self.restore_snapshot(snapshot);
        true
    }

    pub fn redo(&mut self) -> bool {
        let Some(snapshot) = self.redo_stack.pop() else {
            return false;
        };

        self.undo_stack.push(self.snapshot());
        self.restore_snapshot(snapshot);
        true
    }

    pub fn cursor_screen_position(&self) -> Option<(usize, usize)> {
        if self.cursor_row < self.scroll_row || self.cursor_col < self.scroll_col {
            return None;
        }

        Some((
            self.cursor_col - self.scroll_col,
            self.cursor_row - self.scroll_row,
        ))
    }

    pub fn cursor(&self) -> (usize, usize) {
        (self.cursor_row, self.cursor_col)
    }

    pub fn view_state(&self) -> BufferViewState {
        BufferViewState {
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
            desired_col: self.desired_col,
            scroll_row: self.scroll_row,
            scroll_col: self.scroll_col,
        }
    }

    pub fn set_view_state(&mut self, state: BufferViewState) {
        self.cursor_row = state.cursor_row.min(self.lines.len().saturating_sub(1));
        self.cursor_col = state.cursor_col.min(self.current_line_len());
        self.desired_col = state.desired_col.min(self.current_line_len());
        self.scroll_row = state.scroll_row;
        self.scroll_col = state.scroll_col;
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn lines(&self) -> &[String] {
        &self.lines
    }

    pub fn line_count(&self) -> usize {
        self.lines.len()
    }

    pub fn total_char_count(&self) -> usize {
        self.lines.iter().map(|line| line.chars().count()).sum()
    }

    pub fn current_line_char_count(&self) -> usize {
        self.current_line_len()
    }

    pub fn scroll_offset(&self) -> (usize, usize) {
        (self.scroll_row, self.scroll_col)
    }

    pub fn search_matches_with_case(&self, query: &str, case_sensitive: bool) -> Vec<SearchMatch> {
        if query.is_empty() {
            return Vec::new();
        }

        let mut matches = Vec::new();
        for (row, line) in self.lines.iter().enumerate() {
            for column in line_match_columns(line, query, case_sensitive) {
                matches.push(SearchMatch {
                    row,
                    col: column,
                    len: query.chars().count(),
                });
            }
        }

        matches
    }

    pub fn search_matches_regex(
        &self,
        query: &str,
        case_sensitive: bool,
    ) -> Result<Vec<SearchMatch>, regex::Error> {
        if query.is_empty() {
            return Ok(Vec::new());
        }

        let regex = build_regex(query, case_sensitive)?;
        let mut matches = Vec::new();
        for (row, line) in self.lines.iter().enumerate() {
            for matched in regex.find_iter(line) {
                matches.push(SearchMatch {
                    row,
                    col: line[..matched.start()].chars().count(),
                    len: line[matched.start()..matched.end()].chars().count(),
                });
            }
        }

        Ok(matches)
    }

    pub fn replace_match(&mut self, search_match: SearchMatch, replacement: &str) -> bool {
        let mut replaced = false;
        self.record_edit(|buffer| {
            replaced = buffer.replace_match_raw(search_match, replacement);
            replaced
        });
        replaced
    }

    pub fn replace_match_regex(
        &mut self,
        query: &str,
        case_sensitive: bool,
        search_match: SearchMatch,
        replacement: &str,
    ) -> Result<bool, regex::Error> {
        let regex = build_regex(query, case_sensitive)?;
        let replacement = replacement.to_owned();
        let mut replaced = false;
        self.record_edit(|buffer| {
            replaced = buffer.replace_match_regex_raw(&regex, search_match, &replacement);
            replaced
        });
        Ok(replaced)
    }

    pub fn replace_all(&mut self, query: &str, replacement: &str, case_sensitive: bool) -> usize {
        let matches = self.search_matches_with_case(query, case_sensitive);
        if matches.is_empty() {
            return 0;
        }

        let replacement = replacement.to_owned();
        let match_count = matches.len();
        self.record_edit(|buffer| {
            for search_match in matches.iter().rev().copied() {
                let _ = buffer.replace_match_raw(search_match, &replacement);
            }
            true
        });
        match_count
    }

    pub fn replace_all_regex(
        &mut self,
        query: &str,
        replacement: &str,
        case_sensitive: bool,
    ) -> Result<usize, regex::Error> {
        let regex = build_regex(query, case_sensitive)?;
        let replacement = replacement.to_owned();
        let match_count = self
            .lines
            .iter()
            .map(|line| regex.find_iter(line).count())
            .sum::<usize>();
        if match_count == 0 {
            return Ok(0);
        }

        self.record_edit(|buffer| {
            for line in &mut buffer.lines {
                if regex.is_match(line) {
                    *line = regex.replace_all(line, replacement.as_str()).into_owned();
                }
            }
            true
        });
        Ok(match_count)
    }

    pub fn move_to_search_match(&mut self, search_match: SearchMatch) {
        self.cursor_row = search_match.row.min(self.lines.len().saturating_sub(1));
        self.cursor_col = search_match.col.min(self.current_line_len());
        self.desired_col = self.cursor_col;
    }

    fn current_line(&self) -> &str {
        &self.lines[self.cursor_row]
    }

    fn current_line_mut(&mut self) -> &mut String {
        &mut self.lines[self.cursor_row]
    }

    fn current_line_len(&self) -> usize {
        line_len(self.current_line())
    }

    fn insert_char_raw(&mut self, ch: char) {
        let column = self.cursor_col;
        let line = self.current_line_mut();
        let index = byte_index(line, column);
        line.insert(index, ch);
        self.cursor_col += 1;
        self.desired_col = self.cursor_col;
    }

    fn insert_newline_raw(&mut self) {
        let column = self.cursor_col;
        let line = self.current_line_mut();
        let split_index = byte_index(line, column);
        let tail = line.split_off(split_index);

        self.cursor_row += 1;
        self.cursor_col = 0;
        self.desired_col = 0;
        self.lines.insert(self.cursor_row, tail);
    }

    fn backspace_raw(&mut self) -> bool {
        if self.cursor_col > 0 {
            let column = self.cursor_col;
            let line = self.current_line_mut();
            let start = byte_index(line, column - 1);
            let end = byte_index(line, column);
            line.replace_range(start..end, "");
            self.cursor_col -= 1;
        } else if self.cursor_row > 0 {
            let current = self.lines.remove(self.cursor_row);
            self.cursor_row -= 1;
            self.cursor_col = self.current_line_len();
            self.current_line_mut().push_str(&current);
        } else {
            return false;
        }

        self.desired_col = self.cursor_col;
        true
    }

    fn delete_forward_raw(&mut self) -> bool {
        let line_len = self.current_line_len();

        if self.cursor_col < line_len {
            let column = self.cursor_col;
            let line = self.current_line_mut();
            let start = byte_index(line, column);
            let end = byte_index(line, column + 1);
            line.replace_range(start..end, "");
        } else if self.cursor_row + 1 < self.lines.len() {
            let next_line = self.lines.remove(self.cursor_row + 1);
            self.current_line_mut().push_str(&next_line);
        } else {
            return false;
        }

        true
    }

    fn replace_match_raw(&mut self, search_match: SearchMatch, replacement: &str) -> bool {
        if search_match.row >= self.lines.len() {
            return false;
        }

        let line = &mut self.lines[search_match.row];
        let start = byte_index(line, search_match.col);
        let end = byte_index(line, search_match.col + search_match.len);
        if start > end || end > line.len() {
            return false;
        }

        line.replace_range(start..end, replacement);
        self.cursor_row = search_match.row;
        self.cursor_col = search_match.col + replacement.chars().count();
        self.desired_col = self.cursor_col;
        true
    }

    fn replace_match_regex_raw(
        &mut self,
        regex: &Regex,
        search_match: SearchMatch,
        replacement: &str,
    ) -> bool {
        if search_match.row >= self.lines.len() {
            return false;
        }

        let target = {
            let line = &self.lines[search_match.row];
            regex.captures_iter(line).find_map(|captures| {
                let matched = captures.get(0)?;
                let col = line[..matched.start()].chars().count();
                let len = line[matched.start()..matched.end()].chars().count();
                if col != search_match.col || len != search_match.len {
                    return None;
                }

                let mut expanded = String::new();
                captures.expand(replacement, &mut expanded);
                Some((matched.start(), matched.end(), expanded))
            })
        };

        let Some((start, end, expanded)) = target else {
            return false;
        };

        let line = &mut self.lines[search_match.row];
        line.replace_range(start..end, &expanded);
        self.cursor_row = search_match.row;
        self.cursor_col = search_match.col + expanded.chars().count();
        self.desired_col = self.cursor_col;
        true
    }

    fn record_edit<F>(&mut self, edit: F)
    where
        F: FnOnce(&mut Self) -> bool,
    {
        let snapshot = self.snapshot();

        if edit(self) {
            self.undo_stack.push(snapshot);
            self.redo_stack.clear();
            self.dirty = true;
        }
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            lines: self.lines.clone(),
            cursor_row: self.cursor_row,
            cursor_col: self.cursor_col,
            desired_col: self.desired_col,
            scroll_row: self.scroll_row,
            scroll_col: self.scroll_col,
            dirty: self.dirty,
        }
    }

    fn restore_snapshot(&mut self, snapshot: Snapshot) {
        self.lines = snapshot.lines;
        self.cursor_row = snapshot.cursor_row;
        self.cursor_col = snapshot.cursor_col;
        self.desired_col = snapshot.desired_col;
        self.scroll_row = snapshot.scroll_row;
        self.scroll_col = snapshot.scroll_col;
        self.dirty = snapshot.dirty;
    }
}

fn line_len(line: &str) -> usize {
    line.chars().count()
}

fn byte_index(line: &str, column: usize) -> usize {
    line.char_indices()
        .nth(column)
        .map(|(index, _)| index)
        .unwrap_or(line.len())
}

fn line_match_columns(line: &str, query: &str, case_sensitive: bool) -> Vec<usize> {
    let haystack = line.chars().collect::<Vec<_>>();
    let needle = query.chars().collect::<Vec<_>>();
    if needle.is_empty() || needle.len() > haystack.len() {
        return Vec::new();
    }

    let mut columns = Vec::new();
    let mut index = 0;
    while index + needle.len() <= haystack.len() {
        let is_match = needle.iter().enumerate().all(|(offset, needle_char)| {
            chars_equal(haystack[index + offset], *needle_char, case_sensitive)
        });
        if is_match {
            columns.push(index);
            index += needle.len();
        } else {
            index += 1;
        }
    }

    columns
}

fn chars_equal(left: char, right: char, case_sensitive: bool) -> bool {
    if case_sensitive {
        left == right
    } else {
        left.to_lowercase().to_string() == right.to_lowercase().to_string()
    }
}

fn build_regex(query: &str, case_sensitive: bool) -> Result<Regex, regex::Error> {
    RegexBuilder::new(query)
        .case_insensitive(!case_sensitive)
        .build()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_trailing_empty_line_when_loading() {
        let buffer = Buffer::from_text("alpha\n");

        assert_eq!(buffer.lines, vec!["alpha".to_owned(), String::new()]);
    }

    #[test]
    fn splits_line_on_newline_insert() {
        let mut buffer = Buffer::from_text("hello");
        buffer.cursor_col = 2;
        buffer.desired_col = 2;

        buffer.insert_newline();

        assert_eq!(buffer.lines, vec!["he".to_owned(), "llo".to_owned()]);
        assert_eq!(buffer.cursor(), (1, 0));
    }

    #[test]
    fn joins_lines_on_backspace_at_line_start() {
        let mut buffer = Buffer::from_text("abc\ndef");
        buffer.cursor_row = 1;
        buffer.cursor_col = 0;

        buffer.backspace();

        assert_eq!(buffer.lines, vec!["abcdef".to_owned()]);
        assert_eq!(buffer.cursor(), (0, 3));
    }

    #[test]
    fn undo_restores_previous_content() {
        let mut buffer = Buffer::from_text("ab");
        buffer.cursor_col = 2;

        buffer.insert_char('c');

        assert!(buffer.undo());
        assert_eq!(buffer.lines, vec!["ab".to_owned()]);
        assert_eq!(buffer.cursor(), (0, 2));
        assert!(!buffer.is_dirty());
    }

    #[test]
    fn redo_reapplies_undone_edit() {
        let mut buffer = Buffer::from_text("ab");
        buffer.cursor_col = 2;

        buffer.insert_char('c');
        assert!(buffer.undo());
        assert!(buffer.redo());

        assert_eq!(buffer.lines, vec!["abc".to_owned()]);
        assert_eq!(buffer.cursor(), (0, 3));
        assert!(buffer.is_dirty());
    }

    #[test]
    fn new_edit_clears_redo_history() {
        let mut buffer = Buffer::from_text("ab");
        buffer.cursor_col = 2;

        buffer.insert_char('c');
        assert!(buffer.undo());
        buffer.insert_char('d');

        assert!(!buffer.redo());
        assert_eq!(buffer.lines, vec!["abd".to_owned()]);
    }

    #[test]
    fn document_jumps_move_to_start_and_end() {
        let mut buffer = Buffer::from_text("alpha\nbeta");
        buffer.move_doc_end();

        assert_eq!(buffer.cursor(), (1, 4));

        buffer.move_doc_start();
        assert_eq!(buffer.cursor(), (0, 0));
    }

    #[test]
    fn reports_total_and_line_char_counts() {
        let mut buffer = Buffer::from_text("alpha\nbeta");
        buffer.move_down();

        assert_eq!(buffer.line_count(), 2);
        assert_eq!(buffer.current_line_char_count(), 4);
        assert_eq!(buffer.total_char_count(), 9);
    }

    #[test]
    fn goto_line_moves_to_requested_line() {
        let mut buffer = Buffer::from_text("alpha\nbeta\ngamma");

        assert!(buffer.goto_line(3));
        assert_eq!(buffer.cursor(), (2, 0));
        assert!(!buffer.goto_line(0));
        assert!(!buffer.goto_line(4));
    }

    #[test]
    fn search_matches_reports_document_hits() {
        let buffer = Buffer::from_text("alpha beta\nbeta gamma\nalphabet");
        let matches = buffer.search_matches_with_case("beta", false);

        assert_eq!(
            matches,
            vec![
                SearchMatch {
                    row: 0,
                    col: 6,
                    len: 4
                },
                SearchMatch {
                    row: 1,
                    col: 0,
                    len: 4
                },
            ]
        );
    }

    #[test]
    fn search_matches_can_ignore_case() {
        let buffer = Buffer::from_text("Hello\nheLLo");
        let matches = buffer.search_matches_with_case("hello", false);

        assert_eq!(
            matches,
            vec![
                SearchMatch {
                    row: 0,
                    col: 0,
                    len: 5
                },
                SearchMatch {
                    row: 1,
                    col: 0,
                    len: 5
                },
            ]
        );
    }

    #[test]
    fn search_matches_support_regex() {
        let buffer = Buffer::from_text("hello\nhallo\nhullo");
        let matches = buffer.search_matches_regex("h.llo", false).unwrap();

        assert_eq!(
            matches,
            vec![
                SearchMatch {
                    row: 0,
                    col: 0,
                    len: 5
                },
                SearchMatch {
                    row: 1,
                    col: 0,
                    len: 5
                },
                SearchMatch {
                    row: 2,
                    col: 0,
                    len: 5
                },
            ]
        );
    }

    #[test]
    fn replace_all_is_a_single_undoable_edit() {
        let mut buffer = Buffer::from_text("hello\nhello");

        let replaced = buffer.replace_all("hello", "world", false);

        assert_eq!(replaced, 2);
        assert_eq!(buffer.lines, vec!["world".to_owned(), "world".to_owned()]);
        assert!(buffer.undo());
        assert_eq!(buffer.lines, vec!["hello".to_owned(), "hello".to_owned()]);
    }

    #[test]
    fn replace_all_regex_is_a_single_undoable_edit() {
        let mut buffer = Buffer::from_text("cat\nbat");

        let replaced = buffer.replace_all_regex("[cb]at", "pet", false).unwrap();

        assert_eq!(replaced, 2);
        assert_eq!(buffer.lines, vec!["pet".to_owned(), "pet".to_owned()]);
        assert!(buffer.undo());
        assert_eq!(buffer.lines, vec!["cat".to_owned(), "bat".to_owned()]);
    }
}
