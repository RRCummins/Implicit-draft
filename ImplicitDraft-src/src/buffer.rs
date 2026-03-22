use std::{fs, path::Path};

use anyhow::{Context, Result};

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

    pub fn visible_lines(&self, height: usize, width: usize) -> Vec<String> {
        let end = (self.scroll_row + height).min(self.lines.len());

        self.lines[self.scroll_row..end]
            .iter()
            .map(|line| clip_line(line, self.scroll_col, width))
            .collect()
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

    pub fn is_dirty(&self) -> bool {
        self.dirty
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

fn clip_line(line: &str, start: usize, width: usize) -> String {
    if width == 0 {
        return String::new();
    }

    line.chars().skip(start).take(width).collect()
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
}
