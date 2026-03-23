use std::{env, path::PathBuf, time::Duration};

use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;

use crate::{
    buffer::Buffer,
    config::{AppConfig, DefaultMode, KeyBindings},
    markdown,
    picker::{Picker, PickerAction, PickerEntry},
    preview, recents, render,
    settings::{ConfigPane, ConfigState},
    theme::Theme,
    welcome::{BRAILLE_LOGO, SHORTCUTS, WelcomeState},
};

const FRAME_POLL_INTERVAL: Duration = Duration::from_millis(80);
const EDITOR_HELP: &str = "ctrl+z undo | ctrl+r redo | ctrl+s save | ctrl+w home | ? controls";
const PREVIEW_HELP: &str = "ctrl+p source+hints | arrows/page move | preview is read-only";
const SOURCE_HELP: &str = "ctrl+p source+hints | plain text editing | ctrl+w home | ? controls";
const PICKER_HELP: &str = "enter/right open | left/backspace parent | a filter | esc home";
const HOME_HELP: &str = "o open | n new | enter recent | / search | ? controls | q quit";
const SEARCH_HELP: &str = "type to filter | backspace delete | enter keep | esc clear";
const CONFIG_HELP: &str = "tab switch pane | enter apply | ctrl+, close | s save | esc cancel";

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
            StartupTarget::Open(path) => match EditorState::open(path.clone(), default_mode) {
                Ok(editor) => {
                    let _ = recents::remember(&path);
                    (Screen::Editor(editor), String::from(default_mode.help()))
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

        match &mut self.screen {
            Screen::Editor(editor) => {
                if let Some(dialog) = editor.dialog {
                    match key.code {
                        KeyCode::Enter | KeyCode::Char('y') => match dialog {
                            EditorDialog::Quit => should_quit_now = true,
                            EditorDialog::ReturnHome => {
                                next_screen = Some(Screen::Welcome(Self::load_welcome_state()));
                                next_status = Some(String::from(HOME_HELP));
                            }
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
                                Ok(()) => {
                                    editor.dialog = None;
                                    match dialog {
                                        EditorDialog::Quit => {
                                            next_status = Some(String::from("saved"));
                                        }
                                        EditorDialog::ReturnHome => {
                                            next_screen =
                                                Some(Screen::Welcome(Self::load_welcome_state()));
                                            next_status = Some(String::from(HOME_HELP));
                                        }
                                    }
                                }
                                Err(error) => next_status = Some(error.to_string()),
                            }
                        }
                        _ => {}
                    }
                } else {
                    if keybindings.editor.quit.matches(key) {
                        self.request_quit();
                        return;
                    } else if keybindings.editor.controls.matches(key) {
                        self.overlay = Some(Overlay::Editor(editor.mode));
                        next_status = Some(String::from("controls"));
                    } else if keybindings.editor.home.matches(key) {
                        if editor.buffer.is_dirty() {
                            editor.dialog = Some(EditorDialog::ReturnHome);
                            next_status = Some(String::from("unsaved changes"));
                        } else {
                            next_screen = Some(Screen::Welcome(Self::load_welcome_state()));
                            next_status = Some(String::from(HOME_HELP));
                        }
                    } else if keybindings.editor.cycle_mode.matches(key) {
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
                            _ if keybindings.editor.save.matches(key) => {
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
                                let tab_width = self.config.tab_width();
                                Self::apply_edit(editor, |buffer| buffer.insert_spaces(tab_width));
                                next_status = Some(String::from("editing"));
                            }
                            _ if keybindings.editor.undo.matches(key) => {
                                next_status = Some(if editor.buffer.undo() {
                                    String::from("undo")
                                } else {
                                    String::from("nothing to undo")
                                });
                            }
                            _ if keybindings.editor.redo.matches(key) => {
                                next_status = Some(if editor.buffer.redo() {
                                    String::from("redo")
                                } else {
                                    String::from("nothing to redo")
                                });
                            }
                            _ if keybindings.editor.save.matches(key) => {
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
                        match EditorState::open(path.clone(), self.default_mode()) {
                            Ok(editor) => {
                                let _ = recents::remember(&path);
                                next_screen = Some(Screen::Editor(editor));
                                next_status = Some(String::from(self.default_mode().help()));
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
                        match EditorState::open(path.clone(), self.default_mode()) {
                            Ok(editor) => {
                                let _ = recents::remember(&path);
                                next_screen = Some(Screen::Editor(editor));
                                next_status = Some(String::from(self.default_mode().help()));
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
                    next_screen = Some(Screen::Editor(EditorState::empty(self.default_mode())));
                    next_status = Some(String::from(self.default_mode().help()));
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
                let content_width = editor_content_width(
                    width,
                    editor.buffer.line_count(),
                    editor_line_numbers_enabled(editor.mode, self.config.line_numbers),
                );
                editor.buffer.sync_viewport(height, content_width);
            }
            Screen::Picker(picker) => picker.sync_viewport(height),
            Screen::Config(_) | Screen::Welcome(_) => {}
        }
    }

    pub fn current_view(&self, list_height: usize, list_width: usize) -> ViewModel {
        match &self.screen {
            Screen::Editor(editor) => ViewModel::Editor {
                line_numbers: editor_line_numbers_enabled(editor.mode, self.config.line_numbers),
                wrap: editor_wrap_enabled(editor.mode, self.config.wrap),
                lines: match editor.mode {
                    EditorMode::SourceHints => {
                        markdown::style_document(editor.buffer.lines(), &self.theme)
                    }
                    EditorMode::Preview => {
                        preview::render_document(editor.buffer.lines(), &self.theme, list_width)
                    }
                    EditorMode::Source => editor
                        .buffer
                        .lines()
                        .iter()
                        .cloned()
                        .map(ratatui::text::Line::raw)
                        .collect(),
                },
                cursor: if editor.dialog.is_some() || editor.mode == EditorMode::Preview {
                    None
                } else {
                    editor.buffer.cursor_screen_position()
                },
                scroll: editor.buffer.scroll_offset(),
                dialog: editor.dialog.map(|dialog| match dialog {
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
                }),
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
                    preview_theme,
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
            Screen::Config(_) => format!(" settings [Config]  {} ", self.status_message),
            Screen::Welcome(_) => format!(" welcome [Home]  {} ", self.status_message),
        }
    }

    fn default_mode(&self) -> EditorMode {
        EditorMode::from(self.config.default_mode)
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

    fn save_editor(editor: &mut EditorState) -> Result<()> {
        let Some(path) = editor.file_path.as_deref() else {
            return Err(anyhow!("save-as flow not implemented yet"));
        };

        editor.buffer.save_to_path(path)?;
        editor.dialog = None;
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

fn editor_content_width(width: usize, line_count: usize, line_numbers: bool) -> usize {
    if !line_numbers {
        return width.max(1);
    }

    width
        .saturating_sub(line_number_gutter_width(line_count))
        .max(1)
}

fn line_number_gutter_width(line_count: usize) -> usize {
    line_count.max(1).to_string().len() + 2
}

fn editor_line_numbers_enabled(mode: EditorMode, configured: bool) -> bool {
    configured && mode != EditorMode::Preview
}

fn editor_wrap_enabled(mode: EditorMode, configured: bool) -> bool {
    configured && mode == EditorMode::Preview
}

#[derive(Debug)]
pub struct EditorState {
    buffer: Buffer,
    file_path: Option<PathBuf>,
    dialog: Option<EditorDialog>,
    viewport_height: usize,
    mode: EditorMode,
}

impl EditorState {
    fn open(path: PathBuf, mode: EditorMode) -> Result<Self> {
        let buffer = Buffer::from_path(&path)?;

        Ok(Self {
            buffer,
            file_path: Some(path),
            dialog: None,
            viewport_height: 1,
            mode,
        })
    }

    fn empty(mode: EditorMode) -> Self {
        Self {
            buffer: Buffer::empty(),
            file_path: None,
            dialog: None,
            viewport_height: 1,
            mode,
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
    Config(ConfigState),
    Welcome(WelcomeState),
}

#[derive(Clone, Copy, Debug)]
enum ConfigExit {
    Cancel,
    Close,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EditorDialog {
    Quit,
    ReturnHome,
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
                "Arrows/Home/End/Page: move cursor",
                "Ctrl+S save   Ctrl+Z undo   Ctrl+R redo",
                "Ctrl+P preview mode   Ctrl+W return home",
                "Ctrl+Q quit app   ? or Esc close this dialog",
            ],
            Self::Editor(EditorMode::Preview) => vec![
                "Arrows/Home/End/Page: move cursor",
                "Ctrl+P source mode   Ctrl+W return home",
                "Ctrl+S save   Ctrl+Q quit app",
                "? or Esc close this dialog",
            ],
            Self::Editor(EditorMode::Source) => vec![
                "Arrows/Home/End/Page: move cursor",
                "Ctrl+S save   Ctrl+Z undo   Ctrl+R redo",
                "Ctrl+P source+hints mode   Ctrl+W return home",
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
                "O open file picker   N new untitled buffer",
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
        line_numbers: bool,
        wrap: bool,
        lines: Vec<ratatui::text::Line<'static>>,
        cursor: Option<(usize, usize)>,
        scroll: (usize, usize),
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
        preview_theme: Theme,
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

    fn editor_app() -> App {
        App {
            screen: Screen::Editor(EditorState::empty(EditorMode::SourceHints)),
            should_quit: false,
            status_message: String::from(EDITOR_HELP),
            search_mode: false,
            theme: Theme::source_hints_default(),
            config: AppConfig::default(),
            keybindings: KeyBindings::default(),
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
