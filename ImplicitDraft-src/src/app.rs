use std::{env, path::PathBuf, time::Duration};

use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::{
    buffer::Buffer,
    markdown,
    picker::{Picker, PickerAction, PickerEntry},
    preview, recents, render,
    theme::Theme,
    welcome::{BRAILLE_LOGO, SHORTCUTS, WelcomeState},
};

const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(250);
const EDITOR_HELP: &str = "ctrl+z undo | ctrl+r redo | ctrl+s save | ctrl+q quit";
const PREVIEW_HELP: &str = "ctrl+p source+hints | arrows/page move | preview is read-only";
const PICKER_HELP: &str = "enter/right open | left/backspace parent | a filter | esc home";
const HOME_HELP: &str = "o open | n new | enter recent | / search | q quit";
const SEARCH_HELP: &str = "type to filter | backspace delete | enter keep | esc clear";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorMode {
    SourceHints,
    Preview,
}

impl EditorMode {
    fn cycle(self) -> Self {
        match self {
            Self::SourceHints => Self::Preview,
            Self::Preview => Self::SourceHints,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SourceHints => "Source+Hints",
            Self::Preview => "Preview",
        }
    }

    fn help(self) -> &'static str {
        match self {
            Self::SourceHints => EDITOR_HELP,
            Self::Preview => PREVIEW_HELP,
        }
    }
}

#[derive(Debug)]
pub struct App {
    screen: Screen,
    should_quit: bool,
    status_message: String,
    search_mode: bool,
    theme: Theme,
}

#[derive(Debug)]
pub enum StartupTarget {
    Welcome,
    Browse(PathBuf),
    Open(PathBuf),
}

impl App {
    pub fn new(startup: StartupTarget) -> Self {
        let (screen, status_message) = match startup {
            StartupTarget::Browse(path) => match Picker::new(path.clone()) {
                Ok(picker) => (Screen::Picker(picker), String::from(PICKER_HELP)),
                Err(error) => (
                    Screen::Welcome(Self::load_welcome_state()),
                    format!("failed to browse {}: {error}", path.display()),
                ),
            },
            StartupTarget::Open(path) => match EditorState::open(path.clone()) {
                Ok(editor) => {
                    let _ = recents::remember(&path);
                    (Screen::Editor(editor), String::from(EDITOR_HELP))
                }
                Err(error) => (
                    Screen::Welcome(Self::load_welcome_state()),
                    format!("failed to open {}: {error}", path.display()),
                ),
            },
            StartupTarget::Welcome => (
                Screen::Welcome(Self::load_welcome_state()),
                String::from(HOME_HELP),
            ),
        };

        Self {
            screen,
            should_quit: false,
            status_message,
            search_mode: false,
            theme: Theme::load_named("dark").unwrap_or_else(|_| Theme::source_hints_default()),
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

    pub fn theme(&self) -> Theme {
        self.theme
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

        if self.search_mode {
            self.handle_search_input(key);
            return;
        }

        let mut next_status = None;
        let mut next_screen = None;
        let mut should_quit_now = false;

        match &mut self.screen {
            Screen::Editor(editor) => {
                if editor.quit_dialog_open {
                    match key.code {
                        KeyCode::Enter | KeyCode::Char('y') => should_quit_now = true,
                        KeyCode::Esc | KeyCode::Char('n') => {
                            editor.quit_dialog_open = false;
                            next_status = Some(String::from("quit canceled"));
                        }
                        KeyCode::Char('q') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            should_quit_now = true;
                        }
                        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            match Self::save_editor(editor) {
                                Ok(()) => next_status = Some(String::from("saved")),
                                Err(error) => next_status = Some(error.to_string()),
                            }
                        }
                        _ => {}
                    }
                } else {
                    if key.code == KeyCode::Char('p')
                        && key.modifiers.contains(KeyModifiers::CONTROL)
                    {
                        editor.mode = editor.mode.cycle();
                        next_status = Some(String::from(editor.mode.help()));
                    } else if editor.mode == EditorMode::Preview {
                        match key.code {
                            KeyCode::Left => editor.buffer.move_left(),
                            KeyCode::Right => editor.buffer.move_right(),
                            KeyCode::Up => editor.buffer.move_up(),
                            KeyCode::Down => editor.buffer.move_down(),
                            KeyCode::Home => editor.buffer.move_home(),
                            KeyCode::End => editor.buffer.move_end(),
                            KeyCode::PageUp => editor.buffer.page_up(editor.viewport_height),
                            KeyCode::PageDown => editor.buffer.page_down(editor.viewport_height),
                            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                match Self::save_editor(editor) {
                                    Ok(()) => next_status = Some(String::from("saved")),
                                    Err(error) => next_status = Some(error.to_string()),
                                }
                            }
                            KeyCode::Backspace
                            | KeyCode::Delete
                            | KeyCode::Enter
                            | KeyCode::Tab
                            | KeyCode::Char(_)
                                if is_insertable(key.modifiers)
                                    || key.modifiers.contains(KeyModifiers::CONTROL) =>
                            {
                                next_status = Some(String::from(PREVIEW_HELP));
                            }
                            _ => {}
                        }
                    } else {
                        match key.code {
                            KeyCode::Left => editor.buffer.move_left(),
                            KeyCode::Right => editor.buffer.move_right(),
                            KeyCode::Up => editor.buffer.move_up(),
                            KeyCode::Down => editor.buffer.move_down(),
                            KeyCode::Home => editor.buffer.move_home(),
                            KeyCode::End => editor.buffer.move_end(),
                            KeyCode::PageUp => editor.buffer.page_up(editor.viewport_height),
                            KeyCode::PageDown => editor.buffer.page_down(editor.viewport_height),
                            KeyCode::Backspace => {
                                Self::apply_edit(editor, Buffer::backspace);
                                next_status = Some(String::from("editing"));
                            }
                            KeyCode::Delete => {
                                Self::apply_edit(editor, Buffer::delete_forward);
                                next_status = Some(String::from("editing"));
                            }
                            KeyCode::Enter => {
                                Self::apply_edit(editor, Buffer::insert_newline);
                                next_status = Some(String::from("editing"));
                            }
                            KeyCode::Tab => {
                                Self::apply_edit(editor, |buffer| buffer.insert_spaces(4));
                                next_status = Some(String::from("editing"));
                            }
                            KeyCode::Char('z') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                next_status = Some(if editor.buffer.undo() {
                                    String::from("undo")
                                } else {
                                    String::from("nothing to undo")
                                });
                            }
                            KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                next_status = Some(if editor.buffer.redo() {
                                    String::from("redo")
                                } else {
                                    String::from("nothing to redo")
                                });
                            }
                            KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                match Self::save_editor(editor) {
                                    Ok(()) => next_status = Some(String::from("saved")),
                                    Err(error) => next_status = Some(error.to_string()),
                                }
                            }
                            KeyCode::Char(ch) if is_insertable(key.modifiers) => {
                                Self::apply_edit(editor, |buffer| buffer.insert_char(ch));
                                next_status = Some(String::from("editing"));
                            }
                            _ => {}
                        }
                    }
                }
            }
            Screen::Picker(picker) => {
                let action = match key.code {
                    KeyCode::Up => {
                        picker.move_up();
                        next_status = Some(String::from("browse"));
                        Ok(PickerAction::None)
                    }
                    KeyCode::Down => {
                        picker.move_down();
                        next_status = Some(String::from("browse"));
                        Ok(PickerAction::None)
                    }
                    KeyCode::PageUp => {
                        picker.page_up(10);
                        next_status = Some(String::from("browse"));
                        Ok(PickerAction::None)
                    }
                    KeyCode::PageDown => {
                        picker.page_down(10);
                        next_status = Some(String::from("browse"));
                        Ok(PickerAction::None)
                    }
                    KeyCode::Left | KeyCode::Backspace => {
                        let result = picker.go_parent().map(|_| PickerAction::None);
                        next_status = Some(String::from("parent"));
                        result
                    }
                    KeyCode::Enter | KeyCode::Right => picker.open_selected(),
                    KeyCode::Esc => {
                        next_screen = Some(Screen::Welcome(Self::load_welcome_state()));
                        next_status = Some(String::from(HOME_HELP));
                        Ok(PickerAction::None)
                    }
                    KeyCode::Char('a') => {
                        let result = picker.toggle_show_all().map(|_| PickerAction::None);
                        next_status = Some(String::from("toggle filter"));
                        result
                    }
                    KeyCode::Char('/') => {
                        self.search_mode = true;
                        next_status = Some(String::from(SEARCH_HELP));
                        Ok(PickerAction::None)
                    }
                    _ => Ok(PickerAction::None),
                };

                match action {
                    Ok(PickerAction::None) => {}
                    Ok(PickerAction::OpenFile(path)) => match EditorState::open(path.clone()) {
                        Ok(editor) => {
                            let _ = recents::remember(&path);
                            next_screen = Some(Screen::Editor(editor));
                            next_status = Some(String::from(
                                "opened from picker | ctrl+s save | ctrl+q quit",
                            ));
                        }
                        Err(error) => next_status = Some(error.to_string()),
                    },
                    Err(error) => next_status = Some(error.to_string()),
                }
            }
            Screen::Welcome(welcome) => match key.code {
                KeyCode::Up => {
                    welcome.move_up();
                    next_status = Some(String::from("browse recents"));
                }
                KeyCode::Down => {
                    welcome.move_down();
                    next_status = Some(String::from("browse recents"));
                }
                KeyCode::Enter => {
                    if let Some(path) = welcome.selected_path() {
                        match EditorState::open(path.clone()) {
                            Ok(editor) => {
                                let _ = recents::remember(&path);
                                next_screen = Some(Screen::Editor(editor));
                                next_status =
                                    Some(String::from("opened recent | ctrl+s save | ctrl+q quit"));
                            }
                            Err(error) => next_status = Some(error.to_string()),
                        }
                    }
                }
                KeyCode::Char('o') | KeyCode::Char('O') => {
                    match Picker::new(env::current_dir().unwrap_or_else(|_| PathBuf::from("."))) {
                        Ok(picker) => {
                            next_screen = Some(Screen::Picker(picker));
                            next_status = Some(String::from(PICKER_HELP));
                        }
                        Err(error) => next_status = Some(error.to_string()),
                    }
                }
                KeyCode::Char('n') | KeyCode::Char('N') => {
                    next_screen = Some(Screen::Editor(EditorState::empty()));
                    next_status = Some(String::from("untitled buffer | ctrl+s save | ctrl+q quit"));
                }
                KeyCode::Char('/') => {
                    match Picker::new(env::current_dir().unwrap_or_else(|_| PathBuf::from("."))) {
                        Ok(picker) => {
                            next_screen = Some(Screen::Picker(picker));
                            self.search_mode = true;
                            next_status = Some(String::from(SEARCH_HELP));
                        }
                        Err(error) => next_status = Some(error.to_string()),
                    }
                }
                KeyCode::Char('q') | KeyCode::Char('Q') => should_quit_now = true,
                _ => {}
            },
        }

        if let Some(screen) = next_screen {
            self.screen = screen;
        }

        if let Some(status_message) = next_status {
            self.status_message = status_message;
        }

        if should_quit_now {
            self.should_quit = true;
        }
    }

    pub fn sync_viewport(&mut self, height: usize, width: usize) {
        match &mut self.screen {
            Screen::Editor(editor) => {
                editor.viewport_height = height;
                editor.buffer.sync_viewport(height, width);
            }
            Screen::Picker(picker) => picker.sync_viewport(height),
            Screen::Welcome(_) => {}
        }
    }

    pub fn current_view(&self, list_height: usize, list_width: usize) -> ViewModel {
        match &self.screen {
            Screen::Editor(editor) => ViewModel::Editor {
                lines: match editor.mode {
                    EditorMode::SourceHints => {
                        markdown::style_document(editor.buffer.lines(), &self.theme)
                    }
                    EditorMode::Preview => {
                        preview::render_document(editor.buffer.lines(), &self.theme, list_width)
                    }
                },
                cursor: if editor.quit_dialog_open || editor.mode == EditorMode::Preview {
                    None
                } else {
                    editor.buffer.cursor_screen_position()
                },
                scroll: editor.buffer.scroll_offset(),
                dialog: editor.quit_dialog_open.then_some([
                    String::from("Save before quitting?"),
                    String::from("Enter/y/ctrl+q: discard   ctrl+s: save and stay"),
                    String::from("Esc or n: cancel"),
                ]),
            },
            Screen::Picker(picker) => ViewModel::Picker {
                cwd: picker.cwd_display(),
                filter: picker.filter_label().to_owned(),
                query: picker.query().to_owned(),
                entries: picker.visible_entries(list_height),
                selected_row: picker.selected_screen_row(),
                metadata: picker
                    .selected_entry()
                    .map(|entry| entry.metadata_lines())
                    .unwrap_or([
                        String::from("path: -"),
                        String::from("type: -"),
                        String::from("size: -"),
                        String::from("modified: -"),
                    ]),
            },
            Screen::Welcome(welcome) => ViewModel::Welcome {
                logo: BRAILLE_LOGO.iter().map(|line| (*line).to_owned()).collect(),
                shortcuts: SHORTCUTS
                    .iter()
                    .map(|(label, value)| ((*label).to_owned(), (*value).to_owned()))
                    .collect(),
                recents: welcome
                    .recents()
                    .iter()
                    .map(|entry| (entry.display_path(), entry.relative_age()))
                    .collect(),
                selected_row: welcome.selected_index(),
                search_active: self.search_mode,
            },
        }
    }

    pub fn status_line(&self) -> String {
        match &self.screen {
            Screen::Editor(editor) => {
                let (row, col) = editor.buffer.cursor();
                let modified_flag = if editor.buffer.is_dirty() {
                    "[+]"
                } else {
                    "[ ]"
                };
                format!(
                    " {} [{}] {}  Ln {}, Col {}  {} ",
                    editor.file_name(),
                    editor.mode.label(),
                    modified_flag,
                    row + 1,
                    col + 1,
                    self.status_message
                )
            }
            Screen::Picker(picker) => format!(
                " {} [Picker]  filter: {}  {} ",
                picker.cwd_display(),
                picker.filter_label(),
                self.status_message
            ),
            Screen::Welcome(_) => format!(" welcome [Home]  {} ", self.status_message),
        }
    }

    fn request_quit(&mut self) {
        match &mut self.screen {
            Screen::Editor(editor) => {
                if editor.buffer.is_dirty() {
                    editor.quit_dialog_open = true;
                    self.status_message = String::from("unsaved changes");
                    return;
                }
            }
            Screen::Picker(_) | Screen::Welcome(_) => {}
        }

        self.should_quit = true;
    }

    fn save_editor(editor: &mut EditorState) -> Result<()> {
        let Some(path) = editor.file_path.as_deref() else {
            return Err(anyhow!("save-as flow not implemented yet"));
        };

        editor.buffer.save_to_path(path)?;
        editor.quit_dialog_open = false;
        Ok(())
    }

    fn apply_edit<F>(editor: &mut EditorState, edit: F)
    where
        F: FnOnce(&mut Buffer),
    {
        edit(&mut editor.buffer);
    }

    fn load_welcome_state() -> WelcomeState {
        match recents::load() {
            Ok(recents) => WelcomeState::new(recents),
            Err(_) => WelcomeState::default(),
        }
    }

    fn handle_search_input(&mut self, key: KeyEvent) {
        let Screen::Picker(picker) = &mut self.screen else {
            self.search_mode = false;
            return;
        };

        match key.code {
            KeyCode::Esc => {
                let _ = picker.clear_query();
                self.search_mode = false;
                self.status_message = String::from(PICKER_HELP);
            }
            KeyCode::Enter => {
                self.search_mode = false;
                self.status_message = if picker.query().is_empty() {
                    String::from(PICKER_HELP)
                } else {
                    format!("filter: {}", picker.query())
                };
            }
            KeyCode::Backspace => match picker.pop_query() {
                Ok(()) => {
                    self.status_message = format!("search: {}", picker.query());
                }
                Err(error) => {
                    self.status_message = error.to_string();
                    self.search_mode = false;
                }
            },
            KeyCode::Char(ch) if is_insertable(key.modifiers) => match picker.append_query(ch) {
                Ok(()) => {
                    self.status_message = format!("search: {}", picker.query());
                }
                Err(error) => {
                    self.status_message = error.to_string();
                    self.search_mode = false;
                }
            },
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

#[derive(Debug)]
pub struct EditorState {
    buffer: Buffer,
    file_path: Option<PathBuf>,
    quit_dialog_open: bool,
    viewport_height: usize,
    mode: EditorMode,
}

impl EditorState {
    fn open(path: PathBuf) -> Result<Self> {
        let buffer = Buffer::from_path(&path)?;

        Ok(Self {
            buffer,
            file_path: Some(path),
            quit_dialog_open: false,
            viewport_height: 1,
            mode: EditorMode::SourceHints,
        })
    }

    fn empty() -> Self {
        Self {
            buffer: Buffer::empty(),
            file_path: None,
            quit_dialog_open: false,
            viewport_height: 1,
            mode: EditorMode::SourceHints,
        }
    }

    fn file_name(&self) -> &str {
        match &self.file_path {
            Some(path) => path.to_str().unwrap_or("[non-utf8 path]"),
            None => "[untitled]",
        }
    }
}

#[derive(Debug)]
enum Screen {
    Editor(EditorState),
    Picker(Picker),
    Welcome(WelcomeState),
}

#[derive(Debug)]
pub enum ViewModel {
    Editor {
        lines: Vec<ratatui::text::Line<'static>>,
        cursor: Option<(usize, usize)>,
        scroll: (usize, usize),
        dialog: Option<[String; 3]>,
    },
    Picker {
        cwd: String,
        filter: String,
        query: String,
        entries: Vec<PickerEntry>,
        selected_row: Option<usize>,
        metadata: [String; 4],
    },
    Welcome {
        logo: Vec<String>,
        shortcuts: Vec<(String, String)>,
        recents: Vec<(String, String)>,
        selected_row: Option<usize>,
        search_active: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{Event, KeyEventState};

    fn editor_app() -> App {
        App {
            screen: Screen::Editor(EditorState::empty()),
            should_quit: false,
            status_message: String::from(EDITOR_HELP),
            search_mode: false,
            theme: Theme::source_hints_default(),
        }
    }

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

    #[test]
    fn ctrl_p_cycles_editor_mode() {
        let mut app = editor_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert_eq!(editor.mode, EditorMode::Preview);
    }

    #[test]
    fn preview_mode_is_read_only_for_text_input() {
        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.mode = EditorMode::Preview;

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('x'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert_eq!(editor.buffer.lines(), &[String::new()]);
        assert_eq!(app.status_message, PREVIEW_HELP);
    }

    #[test]
    fn startup_open_failure_falls_back_to_welcome() {
        let app = App::new(StartupTarget::Open(PathBuf::from("missing-file.md")));

        let Screen::Welcome(_) = app.screen else {
            panic!("welcome screen");
        };
        assert!(app.status_message.contains("failed to open"));
        assert!(app.status_message.contains("missing-file.md"));
    }

    #[test]
    fn startup_browse_failure_falls_back_to_welcome() {
        let app = App::new(StartupTarget::Browse(PathBuf::from("missing-folder")));

        let Screen::Welcome(_) = app.screen else {
            panic!("welcome screen");
        };
        assert!(app.status_message.contains("failed to browse"));
        assert!(app.status_message.contains("missing-folder"));
    }

    #[test]
    fn startup_welcome_uses_home_status() {
        let app = App::new(StartupTarget::Welcome);

        let Screen::Welcome(_) = app.screen else {
            panic!("welcome screen");
        };
        assert_eq!(app.status_message, HOME_HELP);
    }
}
