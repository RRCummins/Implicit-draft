use std::{path::PathBuf, time::Duration};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::{buffer::Buffer, render};

const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug)]
pub struct App {
    buffer: Buffer,
    file_path: Option<PathBuf>,
    should_quit: bool,
    quit_dialog_open: bool,
    status_message: String,
    viewport_height: usize,
}

impl App {
    pub fn new(file_path: Option<PathBuf>) -> Result<Self> {
        let buffer = match file_path.as_deref() {
            Some(path) => Buffer::from_path(path)?,
            None => Buffer::empty(),
        };

        Ok(Self {
            buffer,
            file_path,
            should_quit: false,
            quit_dialog_open: false,
            status_message: String::from("ctrl+z undo | ctrl+r redo | ctrl+s save | ctrl+q quit"),
            viewport_height: 1,
        })
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| render::draw(frame, self))?;

            if event::poll(FRAME_POLL_INTERVAL)? {
                self.handle_event(event::read()?);
            }
        }

        Ok(())
    }

    fn handle_event(&mut self, event: Event) {
        let Event::Key(key) = event else {
            return;
        };

        if key.kind != KeyEventKind::Press {
            return;
        }

        if should_quit(key) {
            self.request_quit();
            return;
        }

        if self.is_confirming_quit() {
            self.handle_quit_dialog(key);
            return;
        }

        match key.code {
            KeyCode::Left => self.buffer.move_left(),
            KeyCode::Right => self.buffer.move_right(),
            KeyCode::Up => self.buffer.move_up(),
            KeyCode::Down => self.buffer.move_down(),
            KeyCode::Home => self.buffer.move_home(),
            KeyCode::End => self.buffer.move_end(),
            KeyCode::PageUp => self.buffer.page_up(self.viewport_height),
            KeyCode::PageDown => self.buffer.page_down(self.viewport_height),
            KeyCode::Backspace => self.apply_edit(Buffer::backspace),
            KeyCode::Delete => self.apply_edit(Buffer::delete_forward),
            KeyCode::Enter => self.apply_edit(Buffer::insert_newline),
            KeyCode::Tab => self.insert_tab(),
            KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => self.undo(),
            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => self.redo(),
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Err(error) = self.save() {
                    self.status_message = error.to_string();
                }
            }
            KeyCode::Char(ch) if is_insertable(key.modifiers) => self.insert_char(ch),
            _ => {}
        }
    }

    pub fn sync_viewport(&mut self, height: usize, width: usize) {
        self.viewport_height = height;
        self.buffer.sync_viewport(height, width);
    }

    pub fn visible_lines(&self, height: usize, width: usize) -> Vec<String> {
        self.buffer.visible_lines(height, width)
    }

    pub fn cursor_screen_position(&self) -> Option<(usize, usize)> {
        if self.is_confirming_quit() {
            return None;
        }

        self.buffer.cursor_screen_position()
    }

    pub fn buffer_name(&self) -> &str {
        match &self.file_path {
            Some(path) => path.to_str().unwrap_or("[non-utf8 path]"),
            None => "[picker pending]",
        }
    }

    pub fn status_line(&self) -> String {
        let (row, col) = self.buffer.cursor();
        let modified_flag = if self.buffer.is_dirty() { "[+]" } else { "[ ]" };
        format!(
            " {} {}  Ln {}, Col {}  {} ",
            self.buffer_name(),
            modified_flag,
            row + 1,
            col + 1,
            self.status_message
        )
    }

    pub fn quit_dialog_lines(&self) -> Option<[String; 3]> {
        self.is_confirming_quit().then(|| {
            [
                String::from("Save before quitting?"),
                String::from("Enter/y/ctrl+q: discard   ctrl+s: save and stay"),
                String::from("Esc or n: cancel"),
            ]
        })
    }

    fn request_quit(&mut self) {
        if self.buffer.is_dirty() {
            self.quit_dialog_open = true;
            self.status_message = String::from("unsaved changes");
            return;
        }

        self.should_quit = true;
    }

    fn save(&mut self) -> Result<()> {
        let Some(path) = self.file_path.as_deref() else {
            self.status_message = String::from("save-as flow not implemented yet");
            return Ok(());
        };

        self.buffer.save_to_path(path)?;
        self.quit_dialog_open = false;
        self.status_message = String::from("saved");
        Ok(())
    }

    fn insert_char(&mut self, ch: char) {
        self.apply_edit(|buffer| buffer.insert_char(ch));
    }

    fn insert_tab(&mut self) {
        self.apply_edit(|buffer| buffer.insert_spaces(4));
    }

    fn apply_edit<F>(&mut self, edit: F)
    where
        F: FnOnce(&mut Buffer),
    {
        edit(&mut self.buffer);
        self.status_message = String::from("editing");
    }

    fn undo(&mut self) {
        if self.buffer.undo() {
            self.status_message = String::from("undo");
        } else {
            self.status_message = String::from("nothing to undo");
        }
    }

    fn redo(&mut self) {
        if self.buffer.redo() {
            self.status_message = String::from("redo");
        } else {
            self.status_message = String::from("nothing to redo");
        }
    }

    fn is_confirming_quit(&self) -> bool {
        self.quit_dialog_open
    }

    fn handle_quit_dialog(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Enter => self.should_quit = true,
            KeyCode::Esc | KeyCode::Char('n') => {
                self.quit_dialog_open = false;
                self.status_message = String::from("quit canceled");
            }
            KeyCode::Char('y') => self.should_quit = true,
            KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if let Err(error) = self.save() {
                    self.status_message = error.to_string();
                }
            }
            _ => {}
        }
    }
}

fn is_insertable(modifiers: KeyModifiers) -> bool {
    matches!(modifiers, KeyModifiers::NONE | KeyModifiers::SHIFT)
}

fn should_quit(key: KeyEvent) -> bool {
    key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quits_on_ctrl_q() {
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);

        assert!(should_quit(key));
    }

    #[test]
    fn ignores_plain_q() {
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);

        assert!(!should_quit(key));
    }

    #[test]
    fn plain_and_shifted_chars_are_insertable() {
        assert!(is_insertable(KeyModifiers::NONE));
        assert!(is_insertable(KeyModifiers::SHIFT));
        assert!(!is_insertable(KeyModifiers::CONTROL));
    }
}
