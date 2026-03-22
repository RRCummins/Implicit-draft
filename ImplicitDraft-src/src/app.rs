use std::{path::PathBuf, time::Duration};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::render;

const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug)]
pub struct App {
    file_path: Option<PathBuf>,
    should_quit: bool,
}

impl App {
    pub fn new(file_path: Option<PathBuf>) -> Self {
        Self {
            file_path,
            should_quit: false,
        }
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
            self.should_quit = true;
        }
    }

    pub fn buffer_name(&self) -> &str {
        match &self.file_path {
            Some(path) => path.to_str().unwrap_or("[non-utf8 path]"),
            None => "[no file]",
        }
    }
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
}
