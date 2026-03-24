use std::{env, fs, path::PathBuf, time::Duration};

use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::{
    buffer::{Buffer, SearchMatch},
    code,
    config::{AppConfig, DefaultMode, KeyBindings},
    filetype::FileType,
    gitdiff::LineChange,
    markdown,
    picker::{Picker, PickerAction, PickerEntry},
    preview, recents, render,
    session::SessionState,
    settings::{ConfigPane, ConfigState},
    sidebar::{SidebarAction, SidebarCreateKind, SidebarRow, SidebarSelection, SidebarState},
    theme::Theme,
    welcome::{BRAILLE_LOGO, SHORTCUTS, WelcomeState},
};

const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(80);
const EDITOR_HELP: &str = "ctrl+f find | alt+n next | alt+p prev | ctrl+g goto | ctrl+s save";
const PREVIEW_HELP: &str = "ctrl+f find | alt+n next | alt+p prev | ctrl+g goto | ctrl+p source";
const SOURCE_HELP: &str =
    "ctrl+f find | alt+n next | alt+p prev | ctrl+g goto | ctrl+p source+hints";
const PICKER_HELP: &str =
    "enter/right open | left/backspace parent | a notes/code filter | esc home";
const HOME_HELP: &str = "o open | n new | c settings | enter recent | / search | q quit";
const SEARCH_HELP: &str = "type to filter | backspace delete | enter keep | esc clear";
const CONFIG_HELP: &str = "tab switch pane | enter apply | ctrl+, close | s save | esc cancel";
const SIDEBAR_HELP: &str =
    "sidebar: enter open | space toggle | n/e/d ops | ctrl+[ ] resize | tab editor";
const DEFAULT_SAVE_AS_PATH: &str = "untitled.md";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorMode {
    SourceHints,
    Preview,
    Source,
}

impl EditorMode {
    fn cycle(self) -> Self {
        match self {
            Self::SourceHints => Self::Preview,
            Self::Preview => Self::Source,
            Self::Source => Self::SourceHints,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::SourceHints => "Source+Hints",
            Self::Preview => "Preview",
            Self::Source => "Source",
        }
    }

    fn help(self) -> &'static str {
        match self {
            Self::SourceHints => EDITOR_HELP,
            Self::Preview => PREVIEW_HELP,
            Self::Source => SOURCE_HELP,
        }
    }
}

impl From<DefaultMode> for EditorMode {
    fn from(value: DefaultMode) -> Self {
        match value {
            DefaultMode::SourceHints => Self::SourceHints,
            DefaultMode::Preview => Self::Preview,
            DefaultMode::Source => Self::Source,
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
    config: AppConfig,
    keybindings: KeyBindings,
    session: SessionState,
    persist_session: bool,
    config_return: Option<Box<Screen>>,
    overlay: Option<Overlay>,
    tick: u64,
}

#[derive(Debug)]
pub enum StartupTarget {
    Welcome,
    Config,
    Browse(PathBuf),
    Open(PathBuf),
}

impl App {
    pub fn new(startup: StartupTarget, config: AppConfig, keybindings: KeyBindings) -> Self {
        let launch_into_config = matches!(startup, StartupTarget::Config);
        let default_mode = EditorMode::from(config.default_mode);
        let session = SessionState::load().unwrap_or_default();
        let theme = Theme::load_named(&config.theme).unwrap_or_else(|_| {
            Theme::load_named("dark").unwrap_or_else(|_| Theme::source_hints_default())
        });
        let (screen, status_message) = match startup {
            StartupTarget::Browse(path) => match Picker::new(path.clone()) {
                Ok(picker) => (Screen::Picker(picker), String::from(PICKER_HELP)),
                Err(error) => (
                    Screen::Welcome(Self::load_welcome_state()),
                    format!("failed to browse {}: {error}", path.display()),
                ),
            },
            StartupTarget::Open(path) => {
                match Self::open_editor(path.clone(), default_mode, &session) {
                    Ok(editor) => {
                        let _ = recents::remember(&path);
                        let status = String::from(editor.mode.help());
                        (Screen::Editor(editor), status)
                    }
                    Err(error) => (
                        Screen::Welcome(Self::load_welcome_state()),
                        format!("failed to open {}: {error}", path.display()),
                    ),
                }
            }
            StartupTarget::Welcome => (
                Screen::Welcome(Self::load_welcome_state()),
                String::from(HOME_HELP),
            ),
            StartupTarget::Config => match ConfigState::new(&config) {
                Ok(config_state) => (Screen::Config(config_state), String::from(CONFIG_HELP)),
                Err(error) => (
                    Screen::Welcome(Self::load_welcome_state()),
                    error.to_string(),
                ),
            },
        };

        Self {
            screen,
            should_quit: false,
            status_message,
            search_mode: false,
            theme,
            config,
            keybindings,
            session,
            persist_session: true,
            config_return: launch_into_config
                .then(|| Box::new(Screen::Welcome(Self::load_welcome_state()))),
            overlay: None,
            tick: 0,
        }
    }

    pub fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        while !self.should_quit {
            terminal.draw(|frame| render::draw(frame, self))?;

            if event::poll(FRAME_POLL_INTERVAL)? {
                self.handle_event(event::read()?);
            }

            self.tick = self.tick.wrapping_add(1);
        }

        Ok(())
    }

    pub fn theme(&self) -> Theme {
        self.theme
    }

    pub fn overlay(&self) -> Option<OverlayView> {
        self.overlay.map(|overlay| OverlayView {
            title: overlay.title().to_owned(),
            lines: overlay.lines().into_iter().map(str::to_owned).collect(),
        })
    }

    fn handle_event(&mut self, event: Event) {
        let Event::Key(key) = event else {
            return;
        };
        let keybindings = self.keybindings.clone();

        if key.kind != KeyEventKind::Press {
            return;
        }

        if matches!(self.screen, Screen::Config(_)) && keybindings.config.close.matches(key) {
            self.finish_config(ConfigExit::Close);
            return;
        }

        if keybindings.global.settings.matches(key) {
            self.open_config();
            return;
        }

        if self.search_mode {
            self.handle_search_input(key);
            return;
        }

        if self.overlay.is_some() {
            match key.code {
                KeyCode::Esc | KeyCode::Enter | KeyCode::Char('?') => {
                    self.overlay = None;
                    self.status_message = match self.screen {
                        Screen::Editor(ref editor) => String::from(editor.mode.help()),
                        Screen::Picker(_) => String::from(PICKER_HELP),
                        Screen::Config(_) => String::from(CONFIG_HELP),
                        Screen::Welcome(_) => String::from(HOME_HELP),
                    };
                }
                _ => {}
            }
            return;
        }

        let mut next_status = None;
        let mut next_screen = None;
        let mut should_quit_now = false;
        let mut clear_overlay = false;
        let mut config_exit = None;
        let mut reload_theme = false;
        let mut persist_sidebar = false;
        let default_mode = self.default_mode();

        match &mut self.screen {
            Screen::Editor(editor) => {
                if let Some(dialog) = editor.dialog.clone() {
                    match dialog {
                        EditorDialog::SaveAs(_) => match Self::handle_save_as_input(editor, key) {
                            SaveDialogOutcome::None => {}
                            SaveDialogOutcome::Saved(after_save) => match after_save {
                                SaveAfterAction::Stay => {
                                    next_status = Some(String::from("saved"));
                                }
                                SaveAfterAction::Quit => should_quit_now = true,
                                SaveAfterAction::ReturnHome => {
                                    next_screen = Some(Screen::Welcome(Self::load_welcome_state()));
                                    next_status = Some(String::from(HOME_HELP));
                                }
                            },
                            SaveDialogOutcome::Canceled => {
                                next_status = Some(String::from("save canceled"));
                            }
                            SaveDialogOutcome::Status(message) => {
                                next_status = Some(message);
                            }
                            SaveDialogOutcome::Error(error) => {
                                next_status = Some(error.to_string());
                            }
                        },
                        EditorDialog::Find(_) => match Self::handle_find_input(editor, key) {
                            FindDialogOutcome::None => {}
                            FindDialogOutcome::Moved { current, total } => {
                                next_status = Some(format!("find {current}/{total}"));
                            }
                            FindDialogOutcome::NoMatches => {
                                next_status = Some(String::from("no matches"));
                            }
                            FindDialogOutcome::Closed => {
                                next_status = Some(String::from(editor.mode.help()));
                            }
                        },
                        EditorDialog::GotoLine(_) => {
                            match Self::handle_goto_line_input(editor, key) {
                                GotoLineOutcome::None => {}
                                GotoLineOutcome::Moved(line) => {
                                    next_status = Some(format!("line {line}"));
                                }
                                GotoLineOutcome::Canceled => {
                                    next_status = Some(String::from(editor.mode.help()));
                                }
                                GotoLineOutcome::Error(error) => {
                                    next_status = Some(error.to_string());
                                }
                            }
                        }
                        EditorDialog::SidebarCreate(_) => {
                            match Self::handle_sidebar_create_input(editor, key) {
                                SidebarCreateOutcome::None => {}
                                SidebarCreateOutcome::Created(path) => {
                                    next_status = Some(format!("created {}", path.display()));
                                }
                                SidebarCreateOutcome::Canceled => {
                                    next_status = Some(String::from(SIDEBAR_HELP));
                                }
                                SidebarCreateOutcome::Error(error) => {
                                    next_status = Some(error.to_string());
                                }
                            }
                        }
                        EditorDialog::SidebarRename(_) => {
                            match Self::handle_sidebar_rename_input(editor, key) {
                                SidebarRenameOutcome::None => {}
                                SidebarRenameOutcome::Renamed { from, to } => {
                                    next_status = Some(format!(
                                        "renamed {} -> {}",
                                        from.display(),
                                        to.display()
                                    ));
                                }
                                SidebarRenameOutcome::Canceled => {
                                    next_status = Some(String::from(SIDEBAR_HELP));
                                }
                                SidebarRenameOutcome::Error(error) => {
                                    next_status = Some(error.to_string());
                                }
                            }
                        }
                        EditorDialog::SidebarDelete(_) => {
                            match Self::handle_sidebar_delete_input(editor, key) {
                                SidebarDeleteOutcome::None => {}
                                SidebarDeleteOutcome::Deleted {
                                    path,
                                    detached_open_buffer,
                                } => {
                                    next_status = Some(if detached_open_buffer {
                                        format!(
                                            "deleted {} (open buffer kept as untitled)",
                                            path.display()
                                        )
                                    } else {
                                        format!("deleted {}", path.display())
                                    });
                                }
                                SidebarDeleteOutcome::Canceled => {
                                    next_status = Some(String::from(SIDEBAR_HELP));
                                }
                                SidebarDeleteOutcome::Error(error) => {
                                    next_status = Some(error.to_string());
                                }
                            }
                        }
                        EditorDialog::Quit | EditorDialog::ReturnHome => match key.code {
                            KeyCode::Enter | KeyCode::Char('y') => match dialog {
                                EditorDialog::Quit => should_quit_now = true,
                                EditorDialog::ReturnHome => {
                                    next_screen = Some(Screen::Welcome(Self::load_welcome_state()));
                                    next_status = Some(String::from(HOME_HELP));
                                }
                                _ => {}
                            },
                            KeyCode::Esc | KeyCode::Char('n') => {
                                editor.dialog = None;
                                next_status = Some(String::from("quit canceled"));
                            }
                            _ if keybindings.editor.quit.matches(key) => {
                                should_quit_now = true;
                            }
                            _ if keybindings.editor.save.matches(key) => {
                                match Self::save_editor(editor) {
                                    Ok(SaveOutcome::Saved) => {
                                        editor.dialog = None;
                                        match dialog {
                                            EditorDialog::Quit => {
                                                next_status = Some(String::from("saved"));
                                            }
                                            EditorDialog::ReturnHome => {
                                                next_screen = Some(Screen::Welcome(
                                                    Self::load_welcome_state(),
                                                ));
                                                next_status = Some(String::from(HOME_HELP));
                                            }
                                            _ => {}
                                        }
                                    }
                                    Ok(SaveOutcome::NeedsPath) => {
                                        editor.dialog = Some(EditorDialog::SaveAs(
                                            SaveAsState::new(match dialog {
                                                EditorDialog::Quit => SaveAfterAction::Quit,
                                                EditorDialog::ReturnHome => {
                                                    SaveAfterAction::ReturnHome
                                                }
                                                _ => SaveAfterAction::Stay,
                                            }),
                                        ));
                                        next_status = Some(String::from("save as"));
                                    }
                                    Err(error) => next_status = Some(error.to_string()),
                                }
                            }
                            _ => {}
                        },
                    }
                } else {
                    if keybindings.editor.quit.matches(key) {
                        self.request_quit();
                        return;
                    } else if keybindings.editor.controls.matches(key) {
                        self.overlay = Some(Overlay::Editor(editor.mode));
                        next_status = Some(String::from("controls"));
                    } else if keybindings.editor.find.matches(key) {
                        editor.dialog = Some(EditorDialog::Find(FindState::for_reopen(editor)));
                        next_status = Some(String::from("find"));
                    } else if keybindings.editor.find_next.matches(key) {
                        next_status = Some(Self::step_editor_search(editor, true));
                    } else if keybindings.editor.find_prev.matches(key) {
                        next_status = Some(Self::step_editor_search(editor, false));
                    } else if keybindings.editor.goto_line.matches(key) {
                        editor.dialog = Some(EditorDialog::GotoLine(GotoLineState::new(
                            editor.buffer.cursor().0 + 1,
                        )));
                        next_status = Some(String::from("goto line"));
                    } else if keybindings.editor.home.matches(key) {
                        if editor.buffer.is_dirty() {
                            editor.dialog = Some(EditorDialog::ReturnHome);
                            next_status = Some(String::from("unsaved changes"));
                        } else {
                            next_screen = Some(Screen::Welcome(Self::load_welcome_state()));
                            next_status = Some(String::from(HOME_HELP));
                        }
                    } else if keybindings.editor.toggle_sidebar.matches(key) {
                        if editor.sidebar.is_open() {
                            editor.sidebar.close();
                            editor.focus = EditorFocus::Editor;
                            next_status = Some(String::from("sidebar closed"));
                            persist_sidebar = true;
                        } else {
                            match editor.sidebar.open() {
                                Ok(()) => {
                                    editor.focus = EditorFocus::Sidebar;
                                    next_status = Some(String::from(SIDEBAR_HELP));
                                    persist_sidebar = true;
                                }
                                Err(error) => next_status = Some(error.to_string()),
                            }
                        }
                    } else if editor.sidebar.is_open()
                        && keybindings.editor.sidebar_narrower.matches(key)
                    {
                        editor.sidebar.resize_narrower();
                        next_status = Some(String::from("sidebar narrower"));
                        persist_sidebar = true;
                    } else if editor.sidebar.is_open()
                        && keybindings.editor.sidebar_wider.matches(key)
                    {
                        editor.sidebar.resize_wider();
                        next_status = Some(String::from("sidebar wider"));
                        persist_sidebar = true;
                    } else if keybindings.editor.cycle_mode.matches(key) {
                        editor.mode = editor.mode.cycle();
                        next_status = Some(String::from(editor.mode.help()));
                    } else if editor.sidebar.is_open() && key.code == KeyCode::Tab {
                        editor.focus = match editor.focus {
                            EditorFocus::Editor => EditorFocus::Sidebar,
                            EditorFocus::Sidebar => EditorFocus::Editor,
                        };
                        next_status = Some(String::from(if editor.focus == EditorFocus::Sidebar {
                            SIDEBAR_HELP
                        } else {
                            editor.mode.help()
                        }));
                    } else if editor.focus == EditorFocus::Sidebar {
                        match key.code {
                            KeyCode::Esc => {
                                editor.focus = EditorFocus::Editor;
                                next_status = Some(String::from(editor.mode.help()));
                            }
                            KeyCode::Up => {
                                editor.sidebar.move_up();
                                next_status = Some(String::from(SIDEBAR_HELP));
                            }
                            KeyCode::Down => {
                                editor.sidebar.move_down();
                                next_status = Some(String::from(SIDEBAR_HELP));
                            }
                            KeyCode::PageUp => {
                                editor.sidebar.page_up(editor.viewport_height);
                                next_status = Some(String::from(SIDEBAR_HELP));
                            }
                            KeyCode::PageDown => {
                                editor.sidebar.page_down(editor.viewport_height);
                                next_status = Some(String::from(SIDEBAR_HELP));
                            }
                            KeyCode::Left => match editor.sidebar.move_left() {
                                Ok(()) => next_status = Some(String::from(SIDEBAR_HELP)),
                                Err(error) => next_status = Some(error.to_string()),
                            },
                            KeyCode::Right | KeyCode::Char(' ') => {
                                match editor.sidebar.toggle_selected_dir() {
                                    Ok(()) => next_status = Some(String::from(SIDEBAR_HELP)),
                                    Err(error) => next_status = Some(error.to_string()),
                                }
                            }
                            KeyCode::Enter => match editor.sidebar.open_selected() {
                                Ok(SidebarAction::None) => {
                                    next_status = Some(String::from(SIDEBAR_HELP));
                                }
                                Ok(SidebarAction::OpenFile(path)) => {
                                    match Self::open_editor(
                                        path.clone(),
                                        default_mode,
                                        &self.session,
                                    ) {
                                        Ok(next_editor) => {
                                            let _ = recents::remember(&path);
                                            let status = String::from(next_editor.mode.help());
                                            next_screen = Some(Screen::Editor(next_editor));
                                            next_status = Some(status);
                                        }
                                        Err(error) => next_status = Some(error.to_string()),
                                    }
                                }
                                Err(error) => next_status = Some(error.to_string()),
                            },
                            KeyCode::Char('r') if is_insertable(key.modifiers) => {
                                match editor.sidebar.refresh() {
                                    Ok(()) => next_status = Some(String::from("sidebar refreshed")),
                                    Err(error) => next_status = Some(error.to_string()),
                                }
                            }
                            KeyCode::Char('n') if key.modifiers == KeyModifiers::NONE => {
                                editor.dialog =
                                    Some(EditorDialog::SidebarCreate(SidebarCreateState::new(
                                        SidebarCreateKind::File,
                                        editor.sidebar.creation_root(),
                                    )));
                                next_status = Some(String::from("new file"));
                            }
                            KeyCode::Char('N') if key.modifiers == KeyModifiers::SHIFT => {
                                editor.dialog =
                                    Some(EditorDialog::SidebarCreate(SidebarCreateState::new(
                                        SidebarCreateKind::Directory,
                                        editor.sidebar.creation_root(),
                                    )));
                                next_status = Some(String::from("new folder"));
                            }
                            KeyCode::Char('e') if key.modifiers == KeyModifiers::NONE => {
                                match editor.sidebar.selected_entry() {
                                    Some(selection) => {
                                        editor.dialog = Some(EditorDialog::SidebarRename(
                                            SidebarRenameState::new(selection),
                                        ));
                                        next_status = Some(String::from("rename"));
                                    }
                                    None => {
                                        next_status = Some(String::from("nothing selected"));
                                    }
                                }
                            }
                            KeyCode::Char('d') if key.modifiers == KeyModifiers::NONE => {
                                match editor.sidebar.selected_entry() {
                                    Some(selection) => {
                                        editor.dialog = Some(EditorDialog::SidebarDelete(
                                            SidebarDeleteState::new(selection),
                                        ));
                                        next_status = Some(String::from("delete"));
                                    }
                                    None => {
                                        next_status = Some(String::from("nothing selected"));
                                    }
                                }
                            }
                            _ => {}
                        }
                    } else if editor.mode == EditorMode::Preview {
                        match key.code {
                            KeyCode::Left => editor.buffer.move_left(),
                            KeyCode::Right => editor.buffer.move_right(),
                            KeyCode::Up => editor.buffer.move_up(),
                            KeyCode::Down => editor.buffer.move_down(),
                            KeyCode::Home if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                editor.buffer.move_doc_start()
                            }
                            KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                editor.buffer.move_doc_end()
                            }
                            KeyCode::Home => editor.buffer.move_home(),
                            KeyCode::End => editor.buffer.move_end(),
                            KeyCode::PageUp => editor.buffer.page_up(editor.viewport_height),
                            KeyCode::PageDown => editor.buffer.page_down(editor.viewport_height),
                            _ if keybindings.editor.save.matches(key) => {
                                match Self::save_editor(editor) {
                                    Ok(SaveOutcome::Saved) => {
                                        next_status = Some(String::from("saved"))
                                    }
                                    Ok(SaveOutcome::NeedsPath) => {
                                        editor.dialog = Some(EditorDialog::SaveAs(
                                            SaveAsState::new(SaveAfterAction::Stay),
                                        ));
                                        next_status = Some(String::from("save as"));
                                    }
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
                            KeyCode::Home if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                editor.buffer.move_doc_start()
                            }
                            KeyCode::End if key.modifiers.contains(KeyModifiers::CONTROL) => {
                                editor.buffer.move_doc_end()
                            }
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
                                let tab_width = self.config.tab_width();
                                Self::apply_edit(editor, |buffer| buffer.insert_spaces(tab_width));
                                next_status = Some(String::from("editing"));
                            }
                            _ if keybindings.editor.undo.matches(key) => {
                                next_status = Some(if editor.buffer.undo() {
                                    editor.refresh_git_changes();
                                    String::from("undo")
                                } else {
                                    String::from("nothing to undo")
                                });
                            }
                            _ if keybindings.editor.redo.matches(key) => {
                                next_status = Some(if editor.buffer.redo() {
                                    editor.refresh_git_changes();
                                    String::from("redo")
                                } else {
                                    String::from("nothing to redo")
                                });
                            }
                            _ if keybindings.editor.save.matches(key) => {
                                match Self::save_editor(editor) {
                                    Ok(SaveOutcome::Saved) => {
                                        next_status = Some(String::from("saved"))
                                    }
                                    Ok(SaveOutcome::NeedsPath) => {
                                        editor.dialog = Some(EditorDialog::SaveAs(
                                            SaveAsState::new(SaveAfterAction::Stay),
                                        ));
                                        next_status = Some(String::from("save as"));
                                    }
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
                    _ if keybindings.picker.controls.matches(key) => {
                        self.overlay = Some(Overlay::Picker);
                        next_status = Some(String::from("controls"));
                        Ok(PickerAction::None)
                    }
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
                    _ if keybindings.picker.back_home.matches(key) => {
                        next_screen = Some(Screen::Welcome(Self::load_welcome_state()));
                        next_status = Some(String::from(HOME_HELP));
                        Ok(PickerAction::None)
                    }
                    _ if keybindings.picker.toggle_filter.matches(key) => {
                        let result = picker.toggle_show_all().map(|_| PickerAction::None);
                        next_status = Some(String::from("toggle filter"));
                        result
                    }
                    _ if keybindings.picker.search.matches(key) => {
                        self.search_mode = true;
                        next_status = Some(String::from(SEARCH_HELP));
                        Ok(PickerAction::None)
                    }
                    _ => Ok(PickerAction::None),
                };

                match action {
                    Ok(PickerAction::None) => {}
                    Ok(PickerAction::OpenFile(path)) => {
                        match Self::open_editor(path.clone(), default_mode, &self.session) {
                            Ok(editor) => {
                                let _ = recents::remember(&path);
                                let status = String::from(editor.mode.help());
                                next_screen = Some(Screen::Editor(editor));
                                next_status = Some(status);
                            }
                            Err(error) => next_status = Some(error.to_string()),
                        }
                    }
                    Err(error) => next_status = Some(error.to_string()),
                }
            }
            Screen::Welcome(welcome) => match key.code {
                _ if keybindings.home.controls.matches(key) => {
                    self.overlay = Some(Overlay::Home);
                    next_status = Some(String::from("controls"));
                }
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
                        match Self::open_editor(path.clone(), default_mode, &self.session) {
                            Ok(editor) => {
                                let _ = recents::remember(&path);
                                let status = String::from(editor.mode.help());
                                next_screen = Some(Screen::Editor(editor));
                                next_status = Some(status);
                            }
                            Err(error) => next_status = Some(error.to_string()),
                        }
                    }
                }
                _ if keybindings.home.open_picker.matches(key) => {
                    match Picker::new(env::current_dir().unwrap_or_else(|_| PathBuf::from("."))) {
                        Ok(picker) => {
                            next_screen = Some(Screen::Picker(picker));
                            next_status = Some(String::from(PICKER_HELP));
                        }
                        Err(error) => next_status = Some(error.to_string()),
                    }
                }
                _ if keybindings.home.new_buffer.matches(key) => {
                    next_screen = Some(Screen::Editor(Self::empty_editor(
                        default_mode,
                        &self.session,
                    )));
                    next_status = Some(String::from(default_mode.help()));
                }
                _ if keybindings.home.settings.matches(key) => {
                    self.open_config();
                    return;
                }
                _ if keybindings.home.search.matches(key) => {
                    match Picker::new(env::current_dir().unwrap_or_else(|_| PathBuf::from("."))) {
                        Ok(picker) => {
                            next_screen = Some(Screen::Picker(picker));
                            self.search_mode = true;
                            next_status = Some(String::from(SEARCH_HELP));
                        }
                        Err(error) => next_status = Some(error.to_string()),
                    }
                }
                _ if keybindings.home.quit.matches(key) => should_quit_now = true,
                _ => {}
            },
            Screen::Config(config_state) => match key.code {
                _ if keybindings.config.controls.matches(key) => {
                    self.overlay = Some(Overlay::Config);
                    next_status = Some(String::from("controls"));
                }
                _ if keybindings.config.cancel.matches(key) => {
                    config_exit = Some(ConfigExit::Cancel)
                }
                _ if keybindings.config.switch_pane.matches(key) => {
                    config_state.toggle_pane();
                    next_status = Some(String::from(CONFIG_HELP));
                }
                KeyCode::Up => {
                    config_state.move_up();
                    if config_state.active_pane() == ConfigPane::Theme {
                        next_status = Some(String::from("previewing theme"));
                    }
                }
                KeyCode::Down => {
                    config_state.move_down();
                    if config_state.active_pane() == ConfigPane::Theme {
                        next_status = Some(String::from("previewing theme"));
                    }
                }
                KeyCode::Left => {
                    if config_state.active_pane() == ConfigPane::Options {
                        config_state.cycle_option_backward();
                        config_state.apply_draft(&mut self.config);
                        reload_theme = true;
                        next_status = Some(String::from("option updated"));
                    }
                }
                KeyCode::Right => {
                    if config_state.active_pane() == ConfigPane::Options {
                        config_state.cycle_option_forward();
                        config_state.apply_draft(&mut self.config);
                        reload_theme = true;
                        next_status = Some(String::from("option updated"));
                    }
                }
                _ if keybindings.config.apply.matches(key) => {
                    if config_state.active_pane() == ConfigPane::Options {
                        config_state.cycle_option_forward();
                        config_state.apply_draft(&mut self.config);
                        reload_theme = true;
                        next_status = Some(String::from("option updated"));
                    } else {
                        config_state.apply_draft(&mut self.config);
                        reload_theme = true;
                        next_status = Some(String::from("theme applied to session"));
                    }
                }
                _ if keybindings.config.save.matches(key) => {
                    config_state.apply_draft(&mut self.config);
                    reload_theme = true;
                    match self.config.save() {
                        Ok(()) => {
                            config_state.mark_saved();
                            next_status = Some(String::from("settings saved"));
                        }
                        Err(error) => next_status = Some(error.to_string()),
                    }
                }
                _ => {}
            },
        }

        if let Some(screen) = next_screen {
            self.screen = screen;
            clear_overlay = true;
        }

        if let Some(status_message) = next_status {
            self.status_message = status_message;
        }

        if persist_sidebar && let Err(error) = self.persist_sidebar_session() {
            self.status_message = error.to_string();
        }

        if clear_overlay {
            self.overlay = None;
        }

        if reload_theme {
            self.reload_theme();
        }

        if should_quit_now {
            self.should_quit = true;
        }

        if let Some(exit) = config_exit {
            self.finish_config(exit);
        }
    }

    pub fn sync_viewport(&mut self, height: usize, width: usize) {
        match &mut self.screen {
            Screen::Editor(editor) => {
                editor.viewport_height = height;
                let editor_width = editor_panel_width(width, &editor.sidebar);
                let content_width = editor_content_width(
                    editor_width,
                    editor.buffer.line_count(),
                    editor_line_numbers_enabled(editor.mode, self.config.line_numbers),
                    editor.git_change_markers.iter().any(Option::is_some),
                );
                editor.buffer.sync_viewport(height, content_width);
                if editor.sidebar.is_open() {
                    editor.sidebar.sync_viewport(height);
                }
            }
            Screen::Picker(picker) => picker.sync_viewport(height),
            Screen::Config(_) | Screen::Welcome(_) => {}
        }
    }

    pub fn current_view(&self, list_height: usize, list_width: usize) -> ViewModel {
        match &self.screen {
            Screen::Editor(editor) => {
                let line_numbers =
                    editor_line_numbers_enabled(editor.mode, self.config.line_numbers);
                let wrap = editor_wrap_enabled(editor.mode, self.config.wrap);
                let editor_width = editor_panel_width(list_width, &editor.sidebar);
                let show_git_change_gutter = editor.git_change_markers.iter().any(Option::is_some);
                let content_width = editor_content_width(
                    editor_width,
                    editor.buffer.line_count(),
                    line_numbers,
                    show_git_change_gutter,
                );

                ViewModel::Editor {
                    title: editor
                        .file_path
                        .as_deref()
                        .map(short_path)
                        .unwrap_or_else(|| String::from("[untitled]")),
                    line_numbers,
                    wrap,
                    git_change_markers: editor.git_change_markers.clone(),
                    lines: match (editor.file_type, editor.mode) {
                        (FileType::Code, EditorMode::SourceHints | EditorMode::Source)
                        | (FileType::Unknown, EditorMode::SourceHints) => code::render_document(
                            editor.buffer.lines(),
                            &self.theme,
                            editor.file_path.as_deref(),
                            editor.file_type,
                        ),
                        (_, EditorMode::SourceHints) => {
                            markdown::style_document(editor.buffer.lines(), &self.theme)
                        }
                        (FileType::Code, EditorMode::Preview) => code::render_preview_document(
                            editor.buffer.lines(),
                            &self.theme,
                            editor.file_path.as_deref(),
                            editor.file_type,
                            content_width,
                        ),
                        (_, EditorMode::Preview) => preview::render_document(
                            editor.buffer.lines(),
                            &self.theme,
                            content_width,
                        ),
                        (_, EditorMode::Source) => editor
                            .buffer
                            .lines()
                            .iter()
                            .cloned()
                            .map(ratatui::text::Line::raw)
                            .collect(),
                    },
                    search_matches: editor
                        .search
                        .as_ref()
                        .map(|state| state.matches.clone())
                        .unwrap_or_default(),
                    search_current: editor.search.as_ref().and_then(|state| state.current_index),
                    cursor: if editor.dialog.is_some()
                        || editor.mode == EditorMode::Preview
                        || editor.focus == EditorFocus::Sidebar
                    {
                        None
                    } else {
                        editor.buffer.cursor_screen_position()
                    },
                    scroll: editor.buffer.scroll_offset(),
                    sidebar_rows: if editor.sidebar.is_open() {
                        editor.sidebar.visible_rows(list_height)
                    } else {
                        Vec::new()
                    },
                    sidebar_selected_row: if editor.sidebar.is_open() {
                        editor.sidebar.selected_row()
                    } else {
                        None
                    },
                    sidebar_width: if editor.sidebar.is_open() {
                        editor.sidebar.width()
                    } else {
                        0
                    },
                    sidebar_focused: editor.sidebar.is_open()
                        && editor.focus == EditorFocus::Sidebar,
                    sidebar_root: editor.sidebar.root_display(),
                    dialog: editor.dialog.as_ref().map(|dialog| match dialog {
                        EditorDialog::Quit => DialogView {
                            title: String::from(" Unsaved Changes "),
                            lines: vec![
                                String::from("Save before quitting?"),
                                String::from("Enter/y/ctrl+q: discard   ctrl+s: save and stay"),
                                String::from("Esc or n: cancel"),
                            ],
                        },
                        EditorDialog::ReturnHome => DialogView {
                            title: String::from(" Return Home "),
                            lines: vec![
                                String::from("Save before returning home?"),
                                String::from("Enter/y: discard   ctrl+s: save and return"),
                                String::from("Esc or n: cancel"),
                            ],
                        },
                        EditorDialog::SaveAs(state) => DialogView {
                            title: String::from(if state.confirm_overwrite {
                                " Overwrite File "
                            } else {
                                " Save As "
                            }),
                            lines: if state.confirm_overwrite {
                                vec![
                                    format!("path: {}", state.path),
                                    String::from(
                                        "File exists. Press Enter or Ctrl+S to overwrite.",
                                    ),
                                    String::from("Edit path, Backspace, or Esc to cancel."),
                                ]
                            } else {
                                vec![
                                    String::from("Enter a path and press Enter or Ctrl+S"),
                                    format!("path: {}", state.path),
                                    String::from("Tab completes path   Esc cancels"),
                                ]
                            },
                        },
                        EditorDialog::Find(state) => DialogView {
                            title: String::from(" Find "),
                            lines: vec![
                                format!("query: {}", state.query),
                                match state.current_index {
                                    Some(index) => {
                                        format!("matches: {}/{}", index + 1, state.matches.len())
                                    }
                                    None => format!("matches: 0/{}", state.matches.len()),
                                },
                                String::from("Enter/Down next   Shift+Enter/Up prev"),
                                String::from("Alt+N next after close   Alt+P prev"),
                                String::from("Esc closes"),
                            ],
                        },
                        EditorDialog::GotoLine(state) => DialogView {
                            title: String::from(" Goto Line "),
                            lines: vec![
                                format!("line: {}", state.line),
                                String::from("Enter jumps to line"),
                                String::from("Esc cancels"),
                            ],
                        },
                        EditorDialog::SidebarCreate(state) => DialogView {
                            title: String::from(state.title()),
                            lines: vec![
                                format!("parent: {}", state.parent.display()),
                                format!("name: {}", state.name),
                                String::from("Enter creates   Esc cancels"),
                            ],
                        },
                        EditorDialog::SidebarRename(state) => DialogView {
                            title: String::from(" Rename "),
                            lines: vec![
                                format!("target: {}", state.target.path.display()),
                                format!("name: {}", state.name),
                                String::from("Enter renames   Esc cancels"),
                            ],
                        },
                        EditorDialog::SidebarDelete(state) => DialogView {
                            title: String::from(if state.target.is_dir {
                                " Delete Folder "
                            } else {
                                " Delete File "
                            }),
                            lines: vec![
                                format!("target: {}", state.target.path.display()),
                                String::from(if state.target.is_dir {
                                    "Enter deletes folder and contents"
                                } else {
                                    "Enter deletes file"
                                }),
                                String::from("Esc cancels"),
                            ],
                        },
                    }),
                }
            }
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
            Screen::Config(config_state) => {
                let preview_theme = config_state.preview_theme();

                ViewModel::Config {
                    themes: config_state.theme_names().to_vec(),
                    selected_theme: config_state.selected_theme(),
                    active_pane: config_state.active_pane(),
                    applied_theme: config_state.applied_theme_name().to_owned(),
                    options: config_state
                        .option_rows()
                        .into_iter()
                        .map(|row| (row.label, row.value))
                        .collect(),
                    selected_option: config_state.selected_option(),
                    preview_theme: Box::new(preview_theme),
                    preview_lines: config_state.preview_lines(&preview_theme, list_width),
                }
            }
            Screen::Welcome(welcome) => ViewModel::Welcome {
                logo: BRAILLE_LOGO.iter().map(|line| (*line).to_owned()).collect(),
                version: format!("v{}", env!("CARGO_PKG_VERSION")),
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
                tick: self.tick,
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
                let total_lines = editor.buffer.line_count();
                let total_chars = editor.buffer.total_char_count();
                let line_chars = editor.buffer.current_line_char_count();
                format!(
                    " {} [{}] {}  Ln {}/{}, Col {}  Line {} ch  Doc {} ch  {} ",
                    editor.file_name(),
                    editor.mode.label(),
                    modified_flag,
                    row + 1,
                    total_lines,
                    col + 1,
                    line_chars,
                    total_chars,
                    self.status_message
                )
            }
            Screen::Picker(picker) => format!(
                " {} [Picker]  filter: {}  {} ",
                picker.cwd_display(),
                picker.filter_label(),
                self.status_message
            ),
            Screen::Config(_) => format!(" settings [Config]  {} ", self.status_message),
            Screen::Welcome(_) => format!(" welcome [Home]  {} ", self.status_message),
        }
    }

    fn default_mode(&self) -> EditorMode {
        EditorMode::from(self.config.default_mode)
    }

    fn editor_mode_for_path(path: &std::path::Path, configured: EditorMode) -> EditorMode {
        match crate::filetype::detect(path) {
            FileType::Markdown => EditorMode::SourceHints,
            FileType::Text => EditorMode::Source,
            FileType::Code => EditorMode::Source,
            FileType::Unknown => configured,
        }
    }

    fn open_editor(
        path: PathBuf,
        configured_mode: EditorMode,
        session: &SessionState,
    ) -> Result<EditorState> {
        let mode = Self::editor_mode_for_path(&path, configured_mode);
        let mut editor = EditorState::open(path, mode)?;
        editor.apply_session(session)?;
        Ok(editor)
    }

    fn empty_editor(mode: EditorMode, session: &SessionState) -> EditorState {
        let mut editor = EditorState::empty(mode);
        let _ = editor.apply_session(session);
        editor
    }

    fn persist_sidebar_session(&mut self) -> Result<()> {
        if !self.persist_session {
            return Ok(());
        }

        let Screen::Editor(editor) = &self.screen else {
            return Ok(());
        };

        self.session.sidebar_open = editor.sidebar.is_open();
        self.session.sidebar_width = editor.sidebar.width();
        self.session.save()
    }

    fn request_quit(&mut self) {
        match &mut self.screen {
            Screen::Editor(editor) => {
                if editor.buffer.is_dirty() {
                    editor.dialog = Some(EditorDialog::Quit);
                    self.status_message = String::from("unsaved changes");
                    return;
                }
            }
            Screen::Picker(_) | Screen::Config(_) | Screen::Welcome(_) => {}
        }

        self.should_quit = true;
    }

    fn save_editor(editor: &mut EditorState) -> Result<SaveOutcome> {
        let Some(path) = editor.file_path.as_deref() else {
            return Ok(SaveOutcome::NeedsPath);
        };

        editor.buffer.save_to_path(path)?;
        editor.refresh_git_changes();
        editor.dialog = None;
        Ok(SaveOutcome::Saved)
    }

    fn step_editor_search(editor: &mut EditorState, forward: bool) -> String {
        let Some(search) = editor.search.as_mut() else {
            return String::from("no active search");
        };

        match search.step(&mut editor.buffer, forward) {
            FindDialogOutcome::Moved { current, total } => format!("find {current}/{total}"),
            FindDialogOutcome::NoMatches => String::from("no matches"),
            FindDialogOutcome::None | FindDialogOutcome::Closed => String::from("find"),
        }
    }

    fn handle_find_input(editor: &mut EditorState, key: KeyEvent) -> FindDialogOutcome {
        let Some(EditorDialog::Find(state)) = editor.dialog.as_mut() else {
            return FindDialogOutcome::None;
        };

        match key.code {
            KeyCode::Esc => {
                editor.search = Some(state.clone());
                editor.dialog = None;
                FindDialogOutcome::Closed
            }
            KeyCode::Enter if key.modifiers.contains(KeyModifiers::SHIFT) => {
                let outcome = state.step(&mut editor.buffer, false);
                editor.search = Some(state.clone());
                outcome
            }
            KeyCode::Enter | KeyCode::Down => {
                let outcome = state.step(&mut editor.buffer, true);
                editor.search = Some(state.clone());
                outcome
            }
            KeyCode::Up => {
                let outcome = state.step(&mut editor.buffer, false);
                editor.search = Some(state.clone());
                outcome
            }
            KeyCode::Backspace => {
                state.query.pop();
                state.refresh(&mut editor.buffer);
                editor.search = Some(state.clone());
                state.outcome()
            }
            KeyCode::Char(ch) if is_insertable(key.modifiers) => {
                state.query.push(ch);
                state.refresh(&mut editor.buffer);
                editor.search = Some(state.clone());
                state.outcome()
            }
            _ => FindDialogOutcome::None,
        }
    }

    fn handle_goto_line_input(editor: &mut EditorState, key: KeyEvent) -> GotoLineOutcome {
        let Some(EditorDialog::GotoLine(state)) = editor.dialog.as_mut() else {
            return GotoLineOutcome::None;
        };

        match key.code {
            KeyCode::Esc => {
                editor.dialog = None;
                GotoLineOutcome::Canceled
            }
            KeyCode::Enter => {
                let Some(line_number) = state.line.trim().parse::<usize>().ok() else {
                    return GotoLineOutcome::Error(anyhow!("line must be a positive number"));
                };

                if !editor.buffer.goto_line(line_number) {
                    return GotoLineOutcome::Error(anyhow!("line out of range"));
                }

                editor.dialog = None;
                GotoLineOutcome::Moved(line_number)
            }
            KeyCode::Backspace => {
                state.line.pop();
                GotoLineOutcome::None
            }
            KeyCode::Char(ch) if ch.is_ascii_digit() && is_insertable(key.modifiers) => {
                if state.line == "0" {
                    state.line.clear();
                }
                state.line.push(ch);
                GotoLineOutcome::None
            }
            _ => GotoLineOutcome::None,
        }
    }

    fn handle_sidebar_create_input(
        editor: &mut EditorState,
        key: KeyEvent,
    ) -> SidebarCreateOutcome {
        let Some(EditorDialog::SidebarCreate(state)) = editor.dialog.as_mut() else {
            return SidebarCreateOutcome::None;
        };

        match key.code {
            KeyCode::Esc => {
                editor.dialog = None;
                SidebarCreateOutcome::Canceled
            }
            KeyCode::Enter => {
                let path = match editor.sidebar.create_entry(state.kind, &state.name) {
                    Ok(path) => path,
                    Err(error) => return SidebarCreateOutcome::Error(error),
                };
                editor.dialog = None;
                SidebarCreateOutcome::Created(path)
            }
            KeyCode::Backspace => {
                state.name.pop();
                SidebarCreateOutcome::None
            }
            KeyCode::Char(ch) if is_insertable(key.modifiers) => {
                state.name.push(ch);
                SidebarCreateOutcome::None
            }
            _ => SidebarCreateOutcome::None,
        }
    }

    fn handle_sidebar_rename_input(
        editor: &mut EditorState,
        key: KeyEvent,
    ) -> SidebarRenameOutcome {
        let Some(EditorDialog::SidebarRename(state)) = editor.dialog.as_mut() else {
            return SidebarRenameOutcome::None;
        };

        match key.code {
            KeyCode::Esc => {
                editor.dialog = None;
                SidebarRenameOutcome::Canceled
            }
            KeyCode::Enter => {
                let (from, to) = match editor.sidebar.rename_selected(&state.name) {
                    Ok(paths) => paths,
                    Err(error) => return SidebarRenameOutcome::Error(error),
                };
                Self::remap_open_path_after_rename(editor, &from, &to);
                editor.dialog = None;
                SidebarRenameOutcome::Renamed { from, to }
            }
            KeyCode::Backspace => {
                state.name.pop();
                SidebarRenameOutcome::None
            }
            KeyCode::Char(ch) if is_insertable(key.modifiers) => {
                state.name.push(ch);
                SidebarRenameOutcome::None
            }
            _ => SidebarRenameOutcome::None,
        }
    }

    fn handle_sidebar_delete_input(
        editor: &mut EditorState,
        key: KeyEvent,
    ) -> SidebarDeleteOutcome {
        let Some(EditorDialog::SidebarDelete(_)) = editor.dialog.as_ref() else {
            return SidebarDeleteOutcome::None;
        };

        match key.code {
            KeyCode::Esc => {
                editor.dialog = None;
                SidebarDeleteOutcome::Canceled
            }
            KeyCode::Enter => {
                let deleted_path = match editor.sidebar.delete_selected() {
                    Ok(path) => path,
                    Err(error) => return SidebarDeleteOutcome::Error(error),
                };
                let detached_open_buffer = Self::detach_open_path_if_deleted(editor, &deleted_path);
                editor.dialog = None;
                SidebarDeleteOutcome::Deleted {
                    path: deleted_path,
                    detached_open_buffer,
                }
            }
            _ => SidebarDeleteOutcome::None,
        }
    }

    fn handle_save_as_input(editor: &mut EditorState, key: KeyEvent) -> SaveDialogOutcome {
        let Some(EditorDialog::SaveAs(state)) = editor.dialog.as_mut() else {
            return SaveDialogOutcome::None;
        };

        match key.code {
            KeyCode::Esc => {
                editor.dialog = None;
                SaveDialogOutcome::Canceled
            }
            KeyCode::Enter => Self::save_editor_as(editor),
            KeyCode::Tab => Self::complete_save_as_path(editor),
            KeyCode::Backspace => {
                state.confirm_overwrite = false;
                state.path.pop();
                SaveDialogOutcome::None
            }
            KeyCode::Char(ch) if is_insertable(key.modifiers) => {
                state.confirm_overwrite = false;
                state.path.push(ch);
                SaveDialogOutcome::None
            }
            _ if key.modifiers == KeyModifiers::CONTROL && key.code == KeyCode::Char('s') => {
                Self::save_editor_as(editor)
            }
            _ => SaveDialogOutcome::None,
        }
    }

    fn save_editor_as(editor: &mut EditorState) -> SaveDialogOutcome {
        let Some(EditorDialog::SaveAs(state)) = editor.dialog.take() else {
            return SaveDialogOutcome::None;
        };

        let trimmed = state.path.trim();
        if trimmed.is_empty() {
            editor.dialog = Some(EditorDialog::SaveAs(state));
            return SaveDialogOutcome::Error(anyhow!("path cannot be empty"));
        }

        let path = PathBuf::from(trimmed);
        let same_target = editor.file_path.as_deref() == Some(path.as_path());
        if path.exists() && !same_target && !state.confirm_overwrite {
            let mut state = state;
            state.confirm_overwrite = true;
            editor.dialog = Some(EditorDialog::SaveAs(state));
            return SaveDialogOutcome::Status(String::from("confirm overwrite"));
        }

        match editor.buffer.save_to_path(&path) {
            Ok(()) => {
                editor.file_type = crate::filetype::detect(&path);
                editor.file_path = Some(path);
                editor.refresh_git_changes();
                SaveDialogOutcome::Saved(state.after_save)
            }
            Err(error) => {
                editor.dialog = Some(EditorDialog::SaveAs(state));
                SaveDialogOutcome::Error(error)
            }
        }
    }

    fn complete_save_as_path(editor: &mut EditorState) -> SaveDialogOutcome {
        let Some(EditorDialog::SaveAs(state)) = editor.dialog.as_mut() else {
            return SaveDialogOutcome::None;
        };

        let current = state.path.trim();
        let (base_dir, prefix, replace_from) = completion_parts(current);
        let Ok(entries) = fs::read_dir(&base_dir) else {
            return SaveDialogOutcome::Error(anyhow!("failed to read {}", base_dir.display()));
        };

        let mut matches = entries
            .filter_map(|entry| entry.ok())
            .filter_map(|entry| {
                let file_name = entry.file_name();
                let name = file_name.to_str()?.to_owned();
                name.starts_with(prefix)
                    .then_some((name, entry.path().is_dir()))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| left.0.cmp(&right.0));

        if matches.is_empty() {
            return SaveDialogOutcome::Status(String::from("no path matches"));
        }

        let common = longest_common_prefix(
            &matches
                .iter()
                .map(|(name, _)| name.as_str())
                .collect::<Vec<_>>(),
        );
        let replacement = if matches.len() == 1 {
            let (name, is_dir) = &matches[0];
            let mut completed = name.clone();
            if *is_dir {
                completed.push(std::path::MAIN_SEPARATOR);
            }
            completed
        } else if common.len() > prefix.len() {
            common
        } else {
            return SaveDialogOutcome::Status(format!("{} path matches", matches.len()));
        };

        state.path.replace_range(replace_from.., &replacement);
        state.confirm_overwrite = false;
        SaveDialogOutcome::Status(String::from("completed path"))
    }

    fn apply_edit<F>(editor: &mut EditorState, edit: F)
    where
        F: FnOnce(&mut Buffer),
    {
        edit(&mut editor.buffer);
        editor.refresh_git_changes();
    }

    fn load_welcome_state() -> WelcomeState {
        match recents::load() {
            Ok(recents) => WelcomeState::new(recents),
            Err(_) => WelcomeState::default(),
        }
    }

    fn open_config(&mut self) {
        match ConfigState::new(&self.config) {
            Ok(state) => {
                let previous =
                    std::mem::replace(&mut self.screen, Screen::Welcome(WelcomeState::default()));
                self.config_return = Some(Box::new(previous));
                self.screen = Screen::Config(state);
                self.overlay = None;
                self.search_mode = false;
                self.status_message = String::from(CONFIG_HELP);
            }
            Err(error) => self.status_message = error.to_string(),
        }
    }

    fn finish_config(&mut self, exit: ConfigExit) {
        let Screen::Config(config_state) = &self.screen else {
            return;
        };

        match exit {
            ConfigExit::Cancel => {
                config_state.revert_to_original(&mut self.config);
                self.reload_theme();
                self.status_message = String::from("settings canceled");
            }
            ConfigExit::Close => {
                config_state.keep_applied(&mut self.config);
                self.reload_theme();
                self.status_message = String::from("closed settings");
            }
        }

        self.screen = self
            .config_return
            .take()
            .map(|screen| *screen)
            .unwrap_or(Screen::Welcome(Self::load_welcome_state()));
        self.overlay = None;
    }

    fn reload_theme(&mut self) {
        self.theme = Theme::load_named(&self.config.theme).unwrap_or_else(|_| {
            Theme::load_named("dark").unwrap_or_else(|_| Theme::source_hints_default())
        });
    }

    fn remap_open_path_after_rename(
        editor: &mut EditorState,
        from: &std::path::Path,
        to: &std::path::Path,
    ) {
        let Some(current_path) = editor.file_path.clone() else {
            return;
        };

        if current_path == from {
            editor.file_type = crate::filetype::detect(to);
            editor.file_path = Some(to.to_path_buf());
            editor.refresh_git_changes();
            return;
        }

        let Ok(suffix) = current_path.strip_prefix(from) else {
            return;
        };

        let next_path = to.join(suffix);
        editor.file_type = crate::filetype::detect(&next_path);
        editor.file_path = Some(next_path);
        editor.refresh_git_changes();
    }

    fn detach_open_path_if_deleted(
        editor: &mut EditorState,
        deleted_path: &std::path::Path,
    ) -> bool {
        let Some(current_path) = editor.file_path.as_ref() else {
            return false;
        };

        if current_path == deleted_path || current_path.starts_with(deleted_path) {
            editor.file_path = None;
            editor.refresh_git_changes();
            return true;
        }

        false
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

fn editor_content_width(
    width: usize,
    line_count: usize,
    line_numbers: bool,
    show_git_change_gutter: bool,
) -> usize {
    width
        .saturating_sub(git_change_gutter_width(show_git_change_gutter))
        .saturating_sub(if line_numbers {
            line_number_gutter_width(line_count)
        } else {
            0
        })
        .max(1)
}

fn editor_panel_width(width: usize, sidebar: &SidebarState) -> usize {
    if !sidebar.is_open() {
        return width.max(1);
    }

    width.saturating_sub(sidebar.width() as usize).max(1)
}

fn line_number_gutter_width(line_count: usize) -> usize {
    line_count.max(1).to_string().len() + 2
}

fn git_change_gutter_width(show: bool) -> usize {
    if show { 2 } else { 0 }
}

fn editor_line_numbers_enabled(mode: EditorMode, configured: bool) -> bool {
    configured && mode != EditorMode::Preview
}

fn editor_wrap_enabled(mode: EditorMode, configured: bool) -> bool {
    configured && mode == EditorMode::Preview
}

fn short_path(path: &std::path::Path) -> String {
    let parts: Vec<&str> = path
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

fn completion_parts(path: &str) -> (PathBuf, &str, usize) {
    let separator = std::path::MAIN_SEPARATOR;
    if path.is_empty() {
        return (
            env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            "",
            0,
        );
    }

    if path.ends_with(separator) {
        return (PathBuf::from(path), "", path.len());
    }

    match path.rfind(separator) {
        Some(index) => {
            let base = if index == 0 {
                PathBuf::from(separator.to_string())
            } else {
                PathBuf::from(&path[..index])
            };
            (base, &path[index + 1..], index + 1)
        }
        None => (
            env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            path,
            0,
        ),
    }
}

fn longest_common_prefix(values: &[&str]) -> String {
    let Some(first) = values.first() else {
        return String::new();
    };

    let mut prefix = String::new();
    for (index, ch) in first.chars().enumerate() {
        if values
            .iter()
            .skip(1)
            .all(|value| value.chars().nth(index) == Some(ch))
        {
            prefix.push(ch);
        } else {
            break;
        }
    }

    prefix
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorFocus {
    Editor,
    Sidebar,
}

#[derive(Debug)]
pub struct EditorState {
    buffer: Buffer,
    file_path: Option<PathBuf>,
    file_type: FileType,
    git_change_markers: Vec<Option<LineChange>>,
    dialog: Option<EditorDialog>,
    search: Option<FindState>,
    viewport_height: usize,
    mode: EditorMode,
    focus: EditorFocus,
    sidebar: SidebarState,
}

impl EditorState {
    fn open(path: PathBuf, mode: EditorMode) -> Result<Self> {
        let buffer = Buffer::from_path(&path)?;
        let sidebar =
            SidebarState::for_file(Some(&path)).unwrap_or_else(|_| SidebarState::fallback());
        let file_type = crate::filetype::detect(&path);

        Ok(Self {
            buffer,
            file_path: Some(path),
            file_type,
            git_change_markers: Vec::new(),
            dialog: None,
            search: None,
            viewport_height: 1,
            mode,
            focus: EditorFocus::Editor,
            sidebar,
        }
        .with_git_changes())
    }

    fn empty(mode: EditorMode) -> Self {
        Self {
            buffer: Buffer::empty(),
            file_path: None,
            file_type: FileType::Markdown,
            git_change_markers: Vec::new(),
            dialog: None,
            search: None,
            viewport_height: 1,
            mode,
            focus: EditorFocus::Editor,
            sidebar: SidebarState::for_file(None).unwrap_or_else(|_| SidebarState::fallback()),
        }
    }

    fn apply_session(&mut self, session: &SessionState) -> Result<()> {
        self.sidebar.set_width(session.sidebar_width);
        if session.sidebar_open {
            self.sidebar.open()?;
        } else {
            self.sidebar.close();
        }
        self.focus = EditorFocus::Editor;
        Ok(())
    }

    fn file_name(&self) -> &str {
        match &self.file_path {
            Some(path) => path.to_str().unwrap_or("[non-utf8 path]"),
            None => "[untitled]",
        }
    }

    fn with_git_changes(mut self) -> Self {
        self.refresh_git_changes();
        self
    }

    fn refresh_git_changes(&mut self) {
        self.git_change_markers =
            crate::gitdiff::markers_for_buffer(self.file_path.as_deref(), self.buffer.lines());
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug)]
enum Screen {
    Editor(EditorState),
    Picker(Picker),
    Config(ConfigState),
    Welcome(WelcomeState),
}

#[derive(Clone, Copy, Debug)]
enum ConfigExit {
    Cancel,
    Close,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum EditorDialog {
    Quit,
    ReturnHome,
    SaveAs(SaveAsState),
    Find(FindState),
    GotoLine(GotoLineState),
    SidebarCreate(SidebarCreateState),
    SidebarRename(SidebarRenameState),
    SidebarDelete(SidebarDeleteState),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SaveAfterAction {
    Stay,
    Quit,
    ReturnHome,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SaveAsState {
    path: String,
    after_save: SaveAfterAction,
    confirm_overwrite: bool,
}

impl SaveAsState {
    fn new(after_save: SaveAfterAction) -> Self {
        Self {
            path: String::from(DEFAULT_SAVE_AS_PATH),
            after_save,
            confirm_overwrite: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SaveOutcome {
    Saved,
    NeedsPath,
}

#[derive(Debug)]
enum SaveDialogOutcome {
    None,
    Saved(SaveAfterAction),
    Canceled,
    Status(String),
    Error(anyhow::Error),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FindState {
    query: String,
    matches: Vec<SearchMatch>,
    current_index: Option<usize>,
    anchor: (usize, usize),
}

impl FindState {
    fn new(editor: &EditorState) -> Self {
        Self {
            query: String::new(),
            matches: Vec::new(),
            current_index: None,
            anchor: editor.buffer.cursor(),
        }
    }

    fn for_reopen(editor: &EditorState) -> Self {
        match &editor.search {
            Some(state) => {
                let mut reopened = state.clone();
                reopened.anchor = editor.buffer.cursor();
                reopened
            }
            None => Self::new(editor),
        }
    }

    fn refresh(&mut self, buffer: &mut Buffer) {
        self.matches = buffer.search_matches(&self.query);
        self.current_index = self
            .matches
            .iter()
            .position(|search_match| (search_match.row, search_match.col) >= self.anchor)
            .or_else(|| (!self.matches.is_empty()).then_some(0));

        if let Some(index) = self.current_index {
            buffer.move_to_search_match(self.matches[index]);
        }
    }

    fn step(&mut self, buffer: &mut Buffer, forward: bool) -> FindDialogOutcome {
        if self.query.is_empty() || self.matches.is_empty() {
            return FindDialogOutcome::NoMatches;
        }

        let next_index = match self.current_index {
            Some(index) if forward => (index + 1) % self.matches.len(),
            Some(index) => (index + self.matches.len() - 1) % self.matches.len(),
            None => 0,
        };
        self.current_index = Some(next_index);
        buffer.move_to_search_match(self.matches[next_index]);
        self.outcome()
    }

    fn outcome(&self) -> FindDialogOutcome {
        match self.current_index {
            Some(index) => FindDialogOutcome::Moved {
                current: index + 1,
                total: self.matches.len(),
            },
            None => FindDialogOutcome::NoMatches,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct GotoLineState {
    line: String,
}

impl GotoLineState {
    fn new(current_line: usize) -> Self {
        Self {
            line: current_line.to_string(),
        }
    }
}

#[derive(Debug)]
enum FindDialogOutcome {
    None,
    Moved { current: usize, total: usize },
    NoMatches,
    Closed,
}

#[derive(Debug)]
enum GotoLineOutcome {
    None,
    Moved(usize),
    Canceled,
    Error(anyhow::Error),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SidebarCreateState {
    kind: SidebarCreateKind,
    parent: PathBuf,
    name: String,
}

impl SidebarCreateState {
    fn new(kind: SidebarCreateKind, parent: PathBuf) -> Self {
        Self {
            kind,
            parent,
            name: String::new(),
        }
    }

    fn title(&self) -> &'static str {
        match self.kind {
            SidebarCreateKind::File => " New File ",
            SidebarCreateKind::Directory => " New Folder ",
        }
    }
}

#[derive(Debug)]
enum SidebarCreateOutcome {
    None,
    Created(PathBuf),
    Canceled,
    Error(anyhow::Error),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SidebarRenameState {
    target: SidebarSelection,
    name: String,
}

impl SidebarRenameState {
    fn new(target: SidebarSelection) -> Self {
        let name = target.name.clone();
        Self { target, name }
    }
}

#[derive(Debug)]
enum SidebarRenameOutcome {
    None,
    Renamed { from: PathBuf, to: PathBuf },
    Canceled,
    Error(anyhow::Error),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct SidebarDeleteState {
    target: SidebarSelection,
}

impl SidebarDeleteState {
    fn new(target: SidebarSelection) -> Self {
        Self { target }
    }
}

#[derive(Debug)]
enum SidebarDeleteOutcome {
    None,
    Deleted {
        path: PathBuf,
        detached_open_buffer: bool,
    },
    Canceled,
    Error(anyhow::Error),
}

#[derive(Clone, Copy, Debug)]
enum Overlay {
    Editor(EditorMode),
    Picker,
    Config,
    Home,
}

impl Overlay {
    fn title(self) -> &'static str {
        match self {
            Self::Editor(_) => " Controls ",
            Self::Picker => " Picker Controls ",
            Self::Config => " Settings Controls ",
            Self::Home => " Home Controls ",
        }
    }

    fn lines(self) -> Vec<&'static str> {
        match self {
            Self::Editor(EditorMode::SourceHints) => vec![
                "Arrows move   Home/End line start/end   Ctrl+Home/End doc start/end",
                "Ctrl+S save or save-as   Ctrl+F find   Ctrl+G goto line   Ctrl+E sidebar",
                "Alt+N next match   Alt+P previous match   Ctrl+Z undo   Ctrl+R redo",
                "Ctrl+P preview mode   Ctrl+, settings   Ctrl+W return home",
                "When sidebar is open: Tab focus   Enter open file   N file   Shift+N folder",
                "E rename   D delete   Space/Right toggle dir",
                "Ctrl+[ narrower   Ctrl+] wider",
                "Ctrl+Q quit app   ? or Esc close this dialog",
            ],
            Self::Editor(EditorMode::Preview) => vec![
                "Arrows move   Home/End line start/end   Ctrl+Home/End doc start/end",
                "Ctrl+F find   Ctrl+G goto line   Ctrl+P source mode",
                "Alt+N next match   Alt+P previous match",
                "Ctrl+E sidebar   Ctrl+, settings   Ctrl+W return home",
                "When sidebar is open: Tab focus   Enter open file   N file   Shift+N folder",
                "E rename   D delete   Space/Right toggle dir",
                "Ctrl+[ narrower   Ctrl+] wider",
                "Ctrl+S save   Ctrl+Q quit app",
                "? or Esc close this dialog",
            ],
            Self::Editor(EditorMode::Source) => vec![
                "Arrows move   Home/End line start/end   Ctrl+Home/End doc start/end",
                "Ctrl+S save or save-as   Ctrl+F find   Ctrl+G goto line   Ctrl+E sidebar",
                "Alt+N next match   Alt+P previous match   Ctrl+Z undo   Ctrl+R redo",
                "Ctrl+P source+hints mode   Ctrl+, settings   Ctrl+W return home",
                "When sidebar is open: Tab focus   Enter open file   N file   Shift+N folder",
                "E rename   D delete   Space/Right toggle dir",
                "Ctrl+[ narrower   Ctrl+] wider",
                "Ctrl+Q quit app   ? or Esc close this dialog",
            ],
            Self::Picker => vec![
                "Enter or Right: open file or enter folder",
                "Left or Backspace: go to parent folder",
                "A toggle filter   / search   Esc home",
                "? or Esc close this dialog",
            ],
            Self::Config => vec![
                "Tab switches Theme and Options panes",
                "Arrows move selection in the active pane",
                "Enter applies highlighted theme   Left/Right/Enter edit options",
                "Ctrl+, close   S save to config.toml   Esc cancel and revert",
                "? or Esc close this dialog",
            ],
            Self::Home => vec![
                "O open file picker   N new untitled buffer   C settings",
                "Enter open selected recent   Up/Down move",
                "/ search files   Q quit",
                "? or Esc close this dialog",
            ],
        }
    }
}

#[derive(Debug)]
pub struct DialogView {
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Debug)]
pub struct OverlayView {
    pub title: String,
    pub lines: Vec<String>,
}

#[derive(Debug)]
pub enum ViewModel {
    Editor {
        title: String,
        line_numbers: bool,
        wrap: bool,
        git_change_markers: Vec<Option<LineChange>>,
        lines: Vec<ratatui::text::Line<'static>>,
        search_matches: Vec<SearchMatch>,
        search_current: Option<usize>,
        cursor: Option<(usize, usize)>,
        scroll: (usize, usize),
        sidebar_rows: Vec<SidebarRow>,
        sidebar_selected_row: Option<usize>,
        sidebar_width: u16,
        sidebar_focused: bool,
        sidebar_root: String,
        dialog: Option<DialogView>,
    },
    Picker {
        cwd: String,
        filter: String,
        query: String,
        entries: Vec<PickerEntry>,
        selected_row: Option<usize>,
        metadata: [String; 4],
    },
    Config {
        themes: Vec<String>,
        selected_theme: usize,
        applied_theme: String,
        active_pane: ConfigPane,
        options: Vec<(String, String)>,
        selected_option: usize,
        preview_theme: Box<Theme>,
        preview_lines: Vec<ratatui::text::Line<'static>>,
    },
    Welcome {
        logo: Vec<String>,
        version: String,
        shortcuts: Vec<(String, String)>,
        recents: Vec<(String, String)>,
        selected_row: Option<usize>,
        search_active: bool,
        tick: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{Event, KeyEventState};
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("implicit-app-{name}-{unique}"))
    }

    fn git(root: &PathBuf, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("git command");
        assert!(status.success(), "git {:?} failed", args);
    }

    fn editor_app() -> App {
        App {
            screen: Screen::Editor(EditorState::empty(EditorMode::SourceHints)),
            should_quit: false,
            status_message: String::from(EDITOR_HELP),
            search_mode: false,
            theme: Theme::source_hints_default(),
            config: AppConfig::default(),
            keybindings: KeyBindings::default(),
            session: SessionState::default(),
            persist_session: false,
            config_return: None,
            overlay: None,
            tick: 0,
        }
    }

    fn home_app() -> App {
        App {
            screen: Screen::Welcome(WelcomeState::default()),
            should_quit: false,
            status_message: String::from(HOME_HELP),
            search_mode: false,
            theme: Theme::source_hints_default(),
            config: AppConfig::default(),
            keybindings: KeyBindings::default(),
            session: SessionState::default(),
            persist_session: false,
            config_return: None,
            overlay: None,
            tick: 0,
        }
    }

    #[test]
    fn default_editor_quit_binding_matches_ctrl_q() {
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL);

        assert!(KeyBindings::default().editor.quit.matches(key));
    }

    #[test]
    fn default_editor_quit_binding_ignores_plain_q() {
        let key = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE);

        assert!(!KeyBindings::default().editor.quit.matches(key));
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

        let mut app = editor_app();
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('p'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert_eq!(editor.mode, EditorMode::Source);
    }

    #[test]
    fn ctrl_e_opens_sidebar_with_sidebar_focus() {
        let mut app = editor_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('e'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let ViewModel::Editor {
            sidebar_width,
            sidebar_focused,
            ..
        } = app.current_view(10, 40)
        else {
            panic!("editor view");
        };

        assert!(sidebar_width > 0);
        assert!(sidebar_focused);
        assert_eq!(app.status_message, SIDEBAR_HELP);
    }

    #[test]
    fn sidebar_tab_returns_focus_to_editor() {
        let mut app = editor_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('e'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let ViewModel::Editor {
            cursor,
            sidebar_focused,
            ..
        } = app.current_view(10, 40)
        else {
            panic!("editor view");
        };

        assert!(!sidebar_focused);
        assert_eq!(cursor, Some((0, 0)));
    }

    #[test]
    fn sidebar_enter_opens_selected_file() {
        let root = temp_dir("sidebar-open");
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("a.md"), "a").expect("file");
        fs::write(root.join("b.md"), "b").expect("file");

        let mut app = App::new(
            StartupTarget::Open(root.join("a.md")),
            AppConfig::default(),
            KeyBindings::default(),
        );

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('e'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert!(editor.file_name().ends_with("b.md"));
        assert!(editor.sidebar.is_open());
        assert_eq!(editor.focus, EditorFocus::Editor);

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sidebar_n_creates_new_file() {
        let root = temp_dir("sidebar-new-file");
        fs::create_dir_all(root.join("docs")).expect("mkdir");

        let mut app = App::new(
            StartupTarget::Browse(root.clone()),
            AppConfig::default(),
            KeyBindings::default(),
        );
        assert!(matches!(&app.screen, Screen::Picker(_)));
        app.screen = Screen::Editor(EditorState::empty(EditorMode::SourceHints));
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.sidebar = SidebarState::new(root.clone()).expect("sidebar");
        editor.sidebar.open().expect("open");
        editor.focus = EditorFocus::Sidebar;
        editor.sidebar.select_path(&root.join("docs"));

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('n'),
            KeyModifiers::NONE,
        )));
        for ch in ['n', 'o', 't', 'e', '.', 'm', 'd'] {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            )));
        }
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        assert!(root.join("docs/note.md").exists());
        assert!(app.status_message.contains("created"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sidebar_shift_n_creates_new_folder() {
        let root = temp_dir("sidebar-new-folder");
        fs::create_dir_all(root.join("docs")).expect("mkdir");

        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.sidebar = SidebarState::new(root.clone()).expect("sidebar");
        editor.sidebar.open().expect("open");
        editor.focus = EditorFocus::Sidebar;
        editor.sidebar.select_path(&root.join("docs"));

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('N'),
            KeyModifiers::SHIFT,
        )));
        for ch in ['a', 's', 's', 'e', 't', 's'] {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            )));
        }
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        assert!(root.join("docs/assets").is_dir());
        assert!(app.status_message.contains("created"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sidebar_e_renames_selected_file_and_updates_open_path() {
        let root = temp_dir("sidebar-rename-file");
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("a.md"), "a").expect("file");

        let mut app = App::new(
            StartupTarget::Open(root.join("a.md")),
            AppConfig::default(),
            KeyBindings::default(),
        );

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('e'),
            KeyModifiers::CONTROL,
        )));
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('e'),
            KeyModifiers::NONE,
        )));
        for _ in 0..6 {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Backspace,
                KeyModifiers::NONE,
            )));
        }
        for ch in "renamed.md".chars() {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            )));
        }
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert_eq!(editor.file_path, Some(root.join("renamed.md")));
        assert!(root.join("renamed.md").exists());
        assert!(!root.join("a.md").exists());
        assert!(app.status_message.contains("renamed"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sidebar_e_renames_selected_directory_and_updates_open_path() {
        let root = temp_dir("sidebar-rename-dir");
        fs::create_dir_all(root.join("docs/nested")).expect("mkdir");
        fs::write(root.join("docs/nested/note.md"), "note").expect("file");

        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.file_path = Some(root.join("docs/nested/note.md"));
        editor.file_type = FileType::Markdown;
        editor.sidebar = SidebarState::new(root.clone()).expect("sidebar");
        editor.sidebar.open().expect("open");
        editor.focus = EditorFocus::Sidebar;
        editor.sidebar.select_path(&root.join("docs"));
        editor.sidebar.toggle_selected_dir().expect("expand docs");
        editor.sidebar.select_path(&root.join("docs/nested"));

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('e'),
            KeyModifiers::NONE,
        )));
        for _ in 0..6 {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Backspace,
                KeyModifiers::NONE,
            )));
        }
        for ch in "notes".chars() {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            )));
        }
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert_eq!(editor.file_path, Some(root.join("docs/notes/note.md")));
        assert!(root.join("docs/notes/note.md").exists());
        assert!(!root.join("docs/nested").exists());

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn sidebar_d_deletes_selected_file_and_detaches_open_buffer() {
        let root = temp_dir("sidebar-delete-file");
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("a.md"), "a").expect("file");

        let mut app = App::new(
            StartupTarget::Open(root.join("a.md")),
            AppConfig::default(),
            KeyBindings::default(),
        );

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('e'),
            KeyModifiers::CONTROL,
        )));
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('d'),
            KeyModifiers::NONE,
        )));
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert_eq!(editor.file_path, None);
        assert!(!root.join("a.md").exists());
        assert!(app.status_message.contains("untitled"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn ctrl_right_bracket_widens_sidebar() {
        let mut app = editor_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('e'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        let before = match app.current_view(10, 40) {
            ViewModel::Editor { sidebar_width, .. } => sidebar_width,
            _ => panic!("editor view"),
        };

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char(']'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let after = match app.current_view(10, 40) {
            ViewModel::Editor { sidebar_width, .. } => sidebar_width,
            _ => panic!("editor view"),
        };

        assert_eq!(after, before + 2);
    }

    #[test]
    fn ctrl_left_bracket_narrows_sidebar() {
        let mut app = editor_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('e'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char(']'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        let before = match app.current_view(10, 40) {
            ViewModel::Editor { sidebar_width, .. } => sidebar_width,
            _ => panic!("editor view"),
        };

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('['),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let after = match app.current_view(10, 40) {
            ViewModel::Editor { sidebar_width, .. } => sidebar_width,
            _ => panic!("editor view"),
        };

        assert_eq!(after + 2, before);
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
    fn ctrl_w_returns_clean_editor_to_home() {
        let mut app = editor_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('w'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let Screen::Welcome(_) = app.screen else {
            panic!("welcome screen");
        };
    }

    #[test]
    fn ctrl_w_prompts_when_editor_is_dirty() {
        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.buffer.insert_char('x');

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('w'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert_eq!(editor.dialog, Some(EditorDialog::ReturnHome));
    }

    #[test]
    fn question_mark_opens_controls_overlay() {
        let mut app = editor_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('?'),
            modifiers: KeyModifiers::SHIFT,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        assert!(app.overlay.is_some());
    }

    #[test]
    fn ctrl_comma_opens_config_screen() {
        let mut app = editor_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char(','),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        assert!(matches!(app.screen, Screen::Config(_)));
        assert!(app.config_return.is_some());
    }

    #[test]
    fn custom_settings_binding_opens_config_screen() {
        let mut app = editor_app();
        app.keybindings.global.settings =
            crate::config::Shortcut::parse("ctrl+g").expect("shortcut");
        app.keybindings.config.close = crate::config::Shortcut::parse("ctrl+g").expect("shortcut");

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('g'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        assert!(matches!(app.screen, Screen::Config(_)));
    }

    #[test]
    fn home_settings_binding_opens_config_screen() {
        let mut app = home_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('c'),
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        assert!(matches!(app.screen, Screen::Config(_)));
    }

    #[test]
    fn code_files_open_in_source_mode() {
        let mode = App::editor_mode_for_path(&PathBuf::from("main.rs"), EditorMode::Preview);
        assert_eq!(mode, EditorMode::Source);
    }

    #[test]
    fn markdown_files_open_in_source_hints_mode() {
        let mode = App::editor_mode_for_path(&PathBuf::from("note.md"), EditorMode::Preview);
        assert_eq!(mode, EditorMode::SourceHints);
    }

    #[test]
    fn editor_applies_sidebar_session_state() {
        let mut editor = EditorState::empty(EditorMode::SourceHints);
        let session = SessionState {
            sidebar_open: true,
            sidebar_width: 30,
        };

        editor.apply_session(&session).expect("apply session");

        assert!(editor.sidebar.is_open());
        assert_eq!(editor.sidebar.width(), 30);
        assert_eq!(editor.focus, EditorFocus::Editor);
    }

    #[test]
    fn config_cancel_restores_previous_screen_and_config() {
        let mut app = editor_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char(','),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        assert!(app.config.line_numbers);

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Esc,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        assert!(matches!(app.screen, Screen::Editor(_)));
        assert!(!app.config.line_numbers);
    }

    #[test]
    fn config_enter_applies_theme_without_exiting() {
        let mut app = editor_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char(','),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        assert!(matches!(app.screen, Screen::Config(_)));
        assert_ne!(app.config.theme, "dark");
    }

    #[test]
    fn ctrl_comma_closes_config_and_keeps_applied_settings() {
        let mut app = editor_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char(','),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Tab,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Down,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Right,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));
        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char(','),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        assert!(matches!(app.screen, Screen::Editor(_)));
        assert!(app.config.line_numbers);
    }

    #[test]
    fn source_mode_renders_plain_text() {
        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.buffer = Buffer::from_text("# Title");
        editor.mode = EditorMode::Source;

        let ViewModel::Editor { lines, .. } = app.current_view(10, 40) else {
            panic!("editor view");
        };

        assert_eq!(lines[0].spans.len(), 1);
        assert_eq!(lines[0].spans[0].content.as_ref(), "# Title");
    }

    #[test]
    fn ctrl_end_jumps_to_document_end() {
        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.buffer = Buffer::from_text("alpha\nbeta");

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::End,
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert_eq!(editor.buffer.cursor(), (1, 4));
    }

    #[test]
    fn status_line_shows_document_stats() {
        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.buffer = Buffer::from_text("alpha\nbeta");
        editor.buffer.move_down();

        let status = app.status_line();
        assert!(status.contains("Ln 2/2"));
        assert!(status.contains("Line 4 ch"));
        assert!(status.contains("Doc 9 ch"));
    }

    #[test]
    fn ctrl_f_opens_find_dialog() {
        let mut app = editor_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('f'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert!(matches!(editor.dialog, Some(EditorDialog::Find(_))));
    }

    #[test]
    fn find_query_moves_to_first_match() {
        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.buffer = Buffer::from_text("alpha\nbeta\ngamma");

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('f'),
            KeyModifiers::CONTROL,
        )));
        for ch in ['b', 'e', 't', 'a'] {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            )));
        }

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert_eq!(editor.buffer.cursor(), (1, 0));
        assert_eq!(app.status_message, "find 1/1");
    }

    #[test]
    fn find_enter_cycles_to_next_match() {
        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.buffer = Buffer::from_text("beta one\nbeta two\nbeta three");

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('f'),
            KeyModifiers::CONTROL,
        )));
        for ch in ['b', 'e', 't', 'a'] {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            )));
        }
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert_eq!(editor.buffer.cursor(), (1, 0));
        assert_eq!(app.status_message, "find 2/3");
    }

    #[test]
    fn closing_find_preserves_highlights() {
        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.buffer = Buffer::from_text("alpha beta");

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('f'),
            KeyModifiers::CONTROL,
        )));
        for ch in ['b', 'e', 't', 'a'] {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            )));
        }
        app.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));

        let ViewModel::Editor { search_matches, .. } = app.current_view(10, 40) else {
            panic!("editor view");
        };
        assert_eq!(search_matches.len(), 1);
    }

    #[test]
    fn alt_n_moves_to_next_saved_match() {
        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.buffer = Buffer::from_text("beta one\nbeta two\nbeta three");

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('f'),
            KeyModifiers::CONTROL,
        )));
        for ch in ['b', 'e', 't', 'a'] {
            app.handle_event(Event::Key(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::NONE,
            )));
        }
        app.handle_event(Event::Key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)));
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('n'),
            KeyModifiers::ALT,
        )));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert_eq!(editor.buffer.cursor(), (1, 0));
        assert_eq!(app.status_message, "find 2/3");
    }

    #[test]
    fn ctrl_g_jumps_to_requested_line() {
        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.buffer = Buffer::from_text("alpha\nbeta\ngamma");

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('g'),
            KeyModifiers::CONTROL,
        )));
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Backspace,
            KeyModifiers::NONE,
        )));
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Char('3'),
            KeyModifiers::NONE,
        )));
        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert_eq!(editor.buffer.cursor(), (2, 0));
        assert!(editor.dialog.is_none());
        assert_eq!(app.status_message, "line 3");
    }

    #[test]
    fn ctrl_s_on_untitled_opens_save_as_dialog() {
        let mut app = editor_app();

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Char('s'),
            modifiers: KeyModifiers::CONTROL,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert!(matches!(editor.dialog, Some(EditorDialog::SaveAs(_))));
    }

    #[test]
    fn save_as_writes_untitled_buffer_to_path() {
        let root = temp_dir("save-as");
        fs::create_dir_all(&root).expect("mkdir");
        let path = root.join("note.md");

        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.buffer = Buffer::from_text("hello");
        editor.dialog = Some(EditorDialog::SaveAs(SaveAsState {
            path: path.display().to_string(),
            after_save: SaveAfterAction::Stay,
            confirm_overwrite: false,
        }));

        app.handle_event(Event::Key(KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert_eq!(editor.file_path.as_deref(), Some(path.as_path()));
        assert_eq!(fs::read_to_string(&path).expect("saved file"), "hello");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn save_as_existing_file_requires_overwrite_confirmation() {
        let root = temp_dir("save-overwrite");
        fs::create_dir_all(&root).expect("mkdir");
        let path = root.join("note.md");
        fs::write(&path, "old").expect("seed");

        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.buffer = Buffer::from_text("new");
        editor.dialog = Some(EditorDialog::SaveAs(SaveAsState {
            path: path.display().to_string(),
            after_save: SaveAfterAction::Stay,
            confirm_overwrite: false,
        }));

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        let Screen::Editor(editor) = &app.screen else {
            panic!("editor screen");
        };
        let Some(EditorDialog::SaveAs(state)) = &editor.dialog else {
            panic!("save as dialog");
        };
        assert!(state.confirm_overwrite);
        assert_eq!(fs::read_to_string(&path).expect("existing file"), "old");

        app.handle_event(Event::Key(KeyEvent::new(
            KeyCode::Enter,
            KeyModifiers::NONE,
        )));

        let Screen::Editor(editor) = app.screen else {
            panic!("editor screen");
        };
        assert!(editor.dialog.is_none());
        assert_eq!(fs::read_to_string(&path).expect("overwritten file"), "new");

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn save_as_tab_completes_unique_path() {
        let root = temp_dir("save-complete");
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(root.join("notes.md"), "body").expect("file");

        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.dialog = Some(EditorDialog::SaveAs(SaveAsState {
            path: root.join("no").display().to_string(),
            after_save: SaveAfterAction::Stay,
            confirm_overwrite: false,
        }));

        app.handle_event(Event::Key(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)));

        let Screen::Editor(editor) = &app.screen else {
            panic!("editor screen");
        };
        let Some(EditorDialog::SaveAs(state)) = &editor.dialog else {
            panic!("save as dialog");
        };
        assert!(state.path.ends_with("notes.md"));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn code_files_render_with_syntax_spans() {
        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.buffer = Buffer::from_text("fn main() {}");
        editor.file_path = Some(PathBuf::from("main.rs"));
        editor.file_type = FileType::Code;
        editor.mode = EditorMode::Source;

        let ViewModel::Editor { lines, .. } = app.current_view(10, 40) else {
            panic!("editor view");
        };

        assert!(lines[0].spans.len() > 1);
        assert_eq!(lines[0].spans[0].content.as_ref(), "fn");
    }

    #[test]
    fn code_preview_renders_gutter() {
        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.buffer = Buffer::from_text("fn main() {}");
        editor.file_path = Some(PathBuf::from("main.rs"));
        editor.file_type = FileType::Code;
        editor.mode = EditorMode::Preview;

        let ViewModel::Editor { lines, .. } = app.current_view(10, 40) else {
            panic!("editor view");
        };

        assert_eq!(lines[0].spans[0].content.as_ref(), "1 │ ");
    }

    #[test]
    fn current_view_includes_git_change_markers_for_modified_lines() {
        let root = temp_dir("git-line-markers");
        fs::create_dir_all(&root).expect("root");
        git(&root, &["init"]);
        git(&root, &["config", "user.name", "Implicit Draft"]);
        git(&root, &["config", "user.email", "implicit@example.com"]);

        let path = root.join("main.swift");
        fs::write(&path, "print(\"hello\")\nlet value = 1\n").expect("file");
        git(&root, &["add", "main.swift"]);
        git(&root, &["commit", "-m", "init"]);

        let mut app = App::new(
            StartupTarget::Open(path),
            AppConfig::default(),
            KeyBindings::default(),
        );
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("expected editor");
        };
        editor.buffer.move_end();
        editor.buffer.insert_char('!');
        let markers =
            crate::gitdiff::markers_for_buffer(editor.file_path.as_deref(), editor.buffer.lines());
        assert_eq!(markers[0], Some(LineChange::Modified));
        editor.git_change_markers = markers;

        let ViewModel::Editor {
            git_change_markers, ..
        } = app.current_view(10, 40)
        else {
            panic!("expected editor view");
        };

        assert_eq!(git_change_markers[0], Some(LineChange::Modified));

        fs::remove_dir_all(root).expect("cleanup");
    }

    #[test]
    fn current_view_preserves_deleted_git_change_markers() {
        let mut app = editor_app();
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("expected editor");
        };
        editor.git_change_markers = vec![Some(LineChange::Deleted)];

        let ViewModel::Editor {
            git_change_markers, ..
        } = app.current_view(10, 40)
        else {
            panic!("expected editor view");
        };

        assert_eq!(git_change_markers[0], Some(LineChange::Deleted));
    }

    #[test]
    fn preview_mode_enables_soft_wrap_from_config() {
        let mut app = editor_app();
        app.config.wrap = true;
        let Screen::Editor(editor) = &mut app.screen else {
            panic!("editor screen");
        };
        editor.mode = EditorMode::Preview;

        let ViewModel::Editor {
            wrap, line_numbers, ..
        } = app.current_view(10, 20)
        else {
            panic!("editor view");
        };

        assert!(wrap);
        assert!(!line_numbers);
    }

    #[test]
    fn startup_open_failure_falls_back_to_welcome() {
        let app = App::new(
            StartupTarget::Open(PathBuf::from("missing-file.md")),
            AppConfig::default(),
            KeyBindings::default(),
        );

        let Screen::Welcome(_) = app.screen else {
            panic!("welcome screen");
        };
        assert!(app.status_message.contains("failed to open"));
        assert!(app.status_message.contains("missing-file.md"));
    }

    #[test]
    fn startup_browse_failure_falls_back_to_welcome() {
        let app = App::new(
            StartupTarget::Browse(PathBuf::from("missing-folder")),
            AppConfig::default(),
            KeyBindings::default(),
        );

        let Screen::Welcome(_) = app.screen else {
            panic!("welcome screen");
        };
        assert!(app.status_message.contains("failed to browse"));
        assert!(app.status_message.contains("missing-folder"));
    }

    #[test]
    fn startup_welcome_uses_home_status() {
        let app = App::new(
            StartupTarget::Welcome,
            AppConfig::default(),
            KeyBindings::default(),
        );

        let Screen::Welcome(_) = app.screen else {
            panic!("welcome screen");
        };
        assert_eq!(app.status_message, HOME_HELP);
    }
}
