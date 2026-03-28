use std::{
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::Rc,
};

use anyhow::{Context, Result, anyhow, bail};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use ratatui::{
    style::Color,
    text::{Line, Span},
};
use softbuffer::{Context as SoftbufferContext, Surface};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{ElementState, Ime, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowAttributes, WindowId},
};

use crate::{
    buffer::{Buffer, SearchMatch},
    code, filetype, gitdiff, preview,
    sidebar::{SidebarAction, SidebarState},
    theme::Theme,
};

const WINDOW_WIDTH: u32 = 1080;
const WINDOW_HEIGHT: u32 = 760;
const CHAR_WIDTH: usize = 8;
const CHAR_HEIGHT: usize = 16;
const H_PADDING: usize = 16;
const TOP_BAR_HEIGHT: usize = 40;
const BOTTOM_BAR_HEIGHT: usize = 26;
const LINE_SPACING: usize = 4;

const BG: u32 = 0x101217;
const PANEL: u32 = 0x171b22;
const TEXT: u32 = 0xd7dce2;
const MUTED: u32 = 0x8d95a3;
const ACCENT: u32 = 0x4fd1c5;
const CURSOR: u32 = 0xffd166;
const CURRENT_LINE: u32 = 0x151922;
const STATUS_OK: u32 = 0x8bd3dd;
const STATUS_WARN: u32 = 0xf2c97d;
const DIALOG_BG: u32 = 0x11161d;
const DIALOG_BORDER: u32 = 0x2a3340;
const ERROR: u32 = 0xff7b72;
const GUTTER_BG: u32 = 0x0d1016;
const TAB_BG: u32 = 0x161b22;
const TAB_ACTIVE_BG: u32 = 0x1d2530;
const RAIL_BG: u32 = 0x0d1016;
const RAIL_ACCENT: u32 = 0x273443;
const LINE_NUMBER_WIDTH: usize = 5;
const GIT_GUTTER_WIDTH: usize = 2;
const LEFT_RAIL_WIDTH: usize = 14;
const TAB_BAR_HEIGHT: usize = 28;
const SIDEBAR_PADDING: usize = 10;
const SEARCH_CURRENT_BG: u32 = 0x365061;
const SEARCH_MATCH_BG: u32 = 0x24313d;

pub fn run(path: Option<&Path>) -> Result<()> {
    let event_loop = EventLoop::new()?;
    let mut app = WindowApp::new(load_document(path));
    event_loop.run_app(&mut app)?;
    Ok(())
}

pub fn launch_new_window(path: Option<&Path>) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        if let Some(bundle) = find_app_bundle()? {
            let mut command = Command::new("open");
            for arg in build_open_bundle_args(&bundle, path) {
                command.arg(arg);
            }
            command
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .with_context(|| format!("failed to open bundle {}", bundle.display()))?;
            return Ok(());
        }
    }

    let exe = std::env::current_exe().context("failed to resolve current executable")?;
    let mut command = Command::new(&exe);
    for arg in build_new_window_args(path) {
        command.arg(arg);
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("failed to launch {}", exe.display()))?;
    Ok(())
}

fn build_new_window_args(path: Option<&Path>) -> Vec<String> {
    let mut args = vec![String::from("--app")];
    if let Some(path) = path {
        args.push(path.display().to_string());
    }
    args
}

#[cfg(target_os = "macos")]
fn find_app_bundle() -> Result<Option<PathBuf>> {
    let current_exe = std::env::current_exe().context("failed to resolve current executable")?;
    for candidate in bundle_candidates(&current_exe) {
        if candidate.exists() {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

#[cfg(target_os = "macos")]
fn bundle_candidates(current_exe: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(bundle) = bundle_root_for_executable(current_exe) {
        candidates.push(bundle);
    }

    let dev_bundle = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|root| root.join("ImplicitDraft-output/Implicit.app"));
    if let Some(bundle) = dev_bundle {
        candidates.push(bundle);
    }

    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join("Applications/Implicit.app"));
    }
    candidates.push(PathBuf::from("/Applications/Implicit.app"));

    let mut unique = Vec::new();
    for candidate in candidates {
        if !unique.iter().any(|existing| existing == &candidate) {
            unique.push(candidate);
        }
    }
    unique
}

#[cfg(target_os = "macos")]
fn bundle_root_for_executable(exe: &Path) -> Option<PathBuf> {
    let contents = exe.parent()?.parent()?;
    let bundle = contents.parent()?;
    (contents.file_name()? == "Contents" && bundle.extension()? == "app")
        .then(|| bundle.to_path_buf())
}

#[cfg(target_os = "macos")]
fn build_open_bundle_args(bundle: &Path, path: Option<&Path>) -> Vec<String> {
    let mut args = vec![
        String::from("-n"),
        String::from("-a"),
        bundle.display().to_string(),
        String::from("--args"),
    ];
    if let Some(path) = path {
        args.push(path.display().to_string());
    }
    args
}

#[derive(Debug)]
struct NativeDocument {
    source_path: Option<PathBuf>,
    buffer: Buffer,
    status: String,
    file_type: filetype::FileType,
}

impl NativeDocument {
    fn message(source_path: Option<PathBuf>, lines: Vec<String>, status: &str) -> Self {
        Self {
            source_path,
            buffer: Buffer::from_text(&lines.join("\n")),
            status: status.to_owned(),
            file_type: filetype::FileType::Text,
        }
    }

    fn display_title(&self) -> String {
        let base = self
            .source_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled");
        if self.buffer.is_dirty() {
            format!("{base} •")
        } else {
            base.to_owned()
        }
    }

    fn subtitle(&self) -> String {
        self.source_path
            .as_ref()
            .map(|path| {
                let kind = match filetype::detect(path) {
                    filetype::FileType::Markdown => "markdown",
                    filetype::FileType::Text => "text",
                    filetype::FileType::Code => "code",
                    filetype::FileType::Unknown => "file",
                };
                format!("{} · {}", path.display(), kind)
            })
            .unwrap_or_else(|| String::from("Native app mode · untitled"))
    }
}

fn load_document(path: Option<&Path>) -> NativeDocument {
    match path {
        Some(path) => match fs::read_to_string(path) {
            Ok(contents) => NativeDocument {
                source_path: Some(path.to_path_buf()),
                buffer: Buffer::from_text(&contents),
                status: String::from("Opened file"),
                file_type: filetype::detect(path),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => NativeDocument {
                source_path: Some(path.to_path_buf()),
                buffer: Buffer::empty(),
                status: String::from("New file"),
                file_type: filetype::detect(path),
            },
            Err(error) => NativeDocument::message(
                Some(path.to_path_buf()),
                vec![
                    String::from("Failed to open file."),
                    String::new(),
                    error.to_string(),
                ],
                "Read error",
            ),
        },
        None => NativeDocument {
            source_path: None,
            buffer: Buffer::empty(),
            status: String::from("Untitled"),
            file_type: filetype::FileType::Markdown,
        },
    }
}

struct WindowApp {
    document: NativeDocument,
    window: Option<Rc<Window>>,
    context: Option<SoftbufferContext<Rc<Window>>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    modifiers: ModifiersState,
    overlay: Option<Overlay>,
    theme: Theme,
    mode: NativeMode,
    selection_anchor: Option<(usize, usize)>,
    search_query: String,
    search_matches: Vec<SearchMatch>,
    search_current: Option<usize>,
    sidebar: SidebarState,
    focus: NativeFocus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DeferredAction {
    Close,
}

#[derive(Debug)]
struct SaveAsState {
    input: String,
    after_save: Option<DeferredAction>,
    error: Option<String>,
}

#[derive(Debug)]
enum Overlay {
    SaveAs(SaveAsState),
    ConfirmClose,
    Search(SearchState),
}

#[derive(Debug)]
struct SearchState {
    input: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeMode {
    Source,
    SourceHints,
    Preview,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum NativeFocus {
    Editor,
    Sidebar,
}

#[derive(Clone, Copy, Debug)]
struct SelectionRange {
    start: (usize, usize),
    end: (usize, usize),
}

#[derive(Clone, Copy, Debug)]
struct SearchHighlight {
    start: usize,
    len: usize,
    current: bool,
}

impl WindowApp {
    fn new(document: NativeDocument) -> Self {
        let mode = NativeMode::for_file_type(document.file_type);
        let sidebar = SidebarState::for_file(document.source_path.as_deref())
            .unwrap_or_else(|_| SidebarState::fallback());
        Self {
            document,
            window: None,
            context: None,
            surface: None,
            modifiers: ModifiersState::default(),
            overlay: None,
            theme: Theme::source_hints_default(),
            mode,
            selection_anchor: None,
            search_query: String::new(),
            search_matches: Vec::new(),
            search_current: None,
            sidebar,
            focus: NativeFocus::Editor,
        }
    }

    fn set_document(&mut self, document: NativeDocument) {
        self.document = document;
        self.mode = NativeMode::for_file_type(self.document.file_type);
        self.selection_anchor = None;
        self.refresh_sidebar_for_document();
        self.refresh_search_matches();
        self.sync_window_title();
    }

    fn open_document(&mut self, path: &Path) {
        if self.document.buffer.is_dirty() {
            match launch_new_window(Some(path)) {
                Ok(()) => {
                    self.document.status = format!("Opened {} in a new window", path.display());
                }
                Err(error) => {
                    self.document.status = error.to_string();
                }
            }
            self.request_redraw();
            return;
        }
        self.set_document(load_document(Some(path)));
        self.request_redraw();
    }

    fn reload_document(&mut self) {
        if let Some(path) = self.document.source_path.clone() {
            self.set_document(load_document(Some(&path)));
            self.request_redraw();
        }
    }

    fn save_document(&mut self) {
        self.save_document_with_action(None);
    }

    fn save_document_with_action(
        &mut self,
        after_save: Option<DeferredAction>,
    ) -> Option<DeferredAction> {
        match self.document.source_path.clone() {
            Some(path) => self.save_document_to(path, after_save),
            None => {
                self.open_save_as(after_save);
                self.request_redraw();
                None
            }
        }
    }

    fn save_document_to(
        &mut self,
        path: PathBuf,
        after_save: Option<DeferredAction>,
    ) -> Option<DeferredAction> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
            && !parent.exists()
        {
            let message = format!("Folder not found: {}", parent.display());
            self.document.status = message.clone();
            if let Some(Overlay::SaveAs(state)) = self.overlay.as_mut() {
                state.error = Some(message);
            }
            self.request_redraw();
            return None;
        }

        match self.document.buffer.save_to_path(&path) {
            Ok(()) => {
                self.document.source_path = Some(path.clone());
                self.document.status = format!("Saved {}", path.display());
                self.overlay = None;
                self.refresh_sidebar_for_document();
                self.sync_window_title();
                self.request_redraw();
                after_save
            }
            Err(error) => {
                let message = error.to_string();
                self.document.status = message.clone();
                if let Some(Overlay::SaveAs(state)) = self.overlay.as_mut() {
                    state.error = Some(message);
                }
                self.request_redraw();
                None
            }
        }
    }

    fn open_save_as(&mut self, after_save: Option<DeferredAction>) {
        let default_path = suggested_save_path(self.document.source_path.as_deref());
        self.overlay = Some(Overlay::SaveAs(SaveAsState {
            input: default_path.display().to_string(),
            after_save,
            error: None,
        }));
        self.document.status = String::from("Save As");
    }

    fn request_close(&mut self) -> bool {
        if self.document.buffer.is_dirty() {
            self.overlay = Some(Overlay::ConfirmClose);
            self.document.status = String::from("Unsaved changes");
            self.request_redraw();
            false
        } else {
            true
        }
    }

    fn handle_overlay_key(&mut self, event: &winit::event::KeyEvent) -> Option<DeferredAction> {
        match self.overlay.as_mut()? {
            Overlay::ConfirmClose => match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.overlay = None;
                    self.document.status = String::from("Close canceled");
                    self.request_redraw();
                    None
                }
                Key::Named(NamedKey::Enter) => Some(DeferredAction::Close),
                Key::Character(text) if text.eq_ignore_ascii_case("d") => {
                    Some(DeferredAction::Close)
                }
                Key::Character(text)
                    if self.modifiers.super_key() && text.eq_ignore_ascii_case("w") =>
                {
                    Some(DeferredAction::Close)
                }
                Key::Character(text)
                    if self.modifiers.super_key() && text.eq_ignore_ascii_case("s") =>
                {
                    self.save_document_with_action(Some(DeferredAction::Close))
                }
                Key::Character(text) if text.eq_ignore_ascii_case("s") => {
                    self.save_document_with_action(Some(DeferredAction::Close))
                }
                Key::Character(text) if text.eq_ignore_ascii_case("n") => {
                    self.overlay = None;
                    self.document.status = String::from("Close canceled");
                    self.request_redraw();
                    None
                }
                _ => None,
            },
            Overlay::SaveAs(state) => match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.overlay = None;
                    self.document.status = String::from("Save canceled");
                    self.request_redraw();
                    None
                }
                Key::Named(NamedKey::Backspace) => {
                    state.input.pop();
                    state.error = None;
                    self.request_redraw();
                    None
                }
                Key::Named(NamedKey::Enter) => {
                    let input = state.input.trim();
                    if input.is_empty() {
                        state.error = Some(String::from("Enter a file path"));
                        self.request_redraw();
                        return None;
                    }

                    let path = PathBuf::from(input);
                    let after_save = state.after_save;
                    self.save_document_to(path, after_save)
                }
                Key::Character(text)
                    if !self.modifiers.super_key()
                        && !self.modifiers.control_key()
                        && !self.modifiers.alt_key() =>
                {
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        state.input.push(ch);
                    }
                    state.error = None;
                    self.request_redraw();
                    None
                }
                _ => None,
            },
            Overlay::Search(state) => match &event.logical_key {
                Key::Named(NamedKey::Escape) => {
                    self.overlay = None;
                    self.document.status = if self.search_query.is_empty() {
                        String::from("Search canceled")
                    } else {
                        format!("{} matches", self.search_matches.len())
                    };
                    self.request_redraw();
                    None
                }
                Key::Named(NamedKey::Backspace) => {
                    state.input.pop();
                    let input = state.input.clone();
                    self.apply_search_input(&input);
                    self.request_redraw();
                    None
                }
                Key::Named(NamedKey::Enter) => {
                    self.move_to_next_search_match(true);
                    self.request_redraw();
                    None
                }
                Key::Character(text)
                    if !self.modifiers.super_key()
                        && !self.modifiers.control_key()
                        && !self.modifiers.alt_key() =>
                {
                    for ch in text.chars().filter(|ch| !ch.is_control()) {
                        state.input.push(ch);
                    }
                    let input = state.input.clone();
                    self.apply_search_input(&input);
                    self.request_redraw();
                    None
                }
                _ => None,
            },
        }
    }

    fn sync_window_title(&self) {
        if let Some(window) = &self.window {
            window.set_title(&format!("Implicit — {}", self.document.display_title()));
        }
    }

    fn window_attributes(&self) -> WindowAttributes {
        Window::default_attributes()
            .with_title(format!("Implicit — {}", self.document.display_title()))
            .with_inner_size(LogicalSize::new(WINDOW_WIDTH as f64, WINDOW_HEIGHT as f64))
            .with_min_inner_size(LogicalSize::new(640.0, 420.0))
    }

    fn ensure_surface_size(&mut self, size: PhysicalSize<u32>) -> Result<()> {
        let Some(surface) = self.surface.as_mut() else {
            return Ok(());
        };
        let Some(width) = NonZeroU32::new(size.width.max(1)) else {
            bail!("invalid surface width");
        };
        let Some(height) = NonZeroU32::new(size.height.max(1)) else {
            bail!("invalid surface height");
        };
        surface
            .resize(width, height)
            .map_err(|error| anyhow!(error.to_string()))?;
        Ok(())
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn visible_rows(&self, size: PhysicalSize<u32>) -> usize {
        let content_height = size
            .height
            .saturating_sub((TOP_BAR_HEIGHT + TAB_BAR_HEIGHT + BOTTOM_BAR_HEIGHT) as u32)
            as usize;
        let row_height = CHAR_HEIGHT + LINE_SPACING;
        (content_height / row_height).max(1)
    }

    fn visible_cols(&self, size: PhysicalSize<u32>) -> usize {
        let sidebar_width = self.sidebar_pixel_width();
        let content_width = size.width as usize;
        content_width
            .saturating_sub(LEFT_RAIL_WIDTH)
            .saturating_sub(sidebar_width)
            .saturating_sub(H_PADDING * 2)
            .saturating_sub((LINE_NUMBER_WIDTH + GIT_GUTTER_WIDTH + 2) * CHAR_WIDTH)
            / CHAR_WIDTH.max(1)
    }

    fn handle_scroll_lines(&mut self, delta: isize) {
        if self.sidebar.is_open() && self.focus == NativeFocus::Sidebar {
            let height = self
                .window
                .as_ref()
                .map(|window| self.visible_rows(window.inner_size()))
                .unwrap_or(1);
            if delta.is_negative() {
                self.sidebar.page_up(delta.unsigned_abs());
            } else {
                self.sidebar.page_down(delta as usize);
            }
            self.sidebar.sync_viewport(height);
            return;
        }
        if delta.is_negative() {
            for _ in 0..delta.unsigned_abs() {
                self.move_cursor_up(false);
            }
        } else {
            for _ in 0..delta as usize {
                self.move_cursor_down(false);
            }
        }
    }

    fn draw(&mut self) -> Result<()> {
        let Some(window) = &self.window else {
            return Ok(());
        };
        let size = window.inner_size();
        if size.width == 0 || size.height == 0 {
            return Ok(());
        }
        let visible_rows = self.visible_rows(size);
        let visible_cols = self.visible_cols(size);
        self.document
            .buffer
            .sync_viewport(visible_rows, visible_cols);
        let (scroll_row, scroll_col) = self.document.buffer.scroll_offset();
        let rendered_lines = self.render_lines(visible_cols);
        let git_markers = gitdiff::markers_for_buffer(
            self.document.source_path.as_deref(),
            self.document.buffer.lines(),
        );
        let overlay = self.overlay.as_ref();
        let sidebar_width = self.sidebar_pixel_width();
        self.sidebar.sync_viewport(visible_rows);
        let sidebar_rows = self.sidebar.visible_rows(visible_rows);
        let sidebar_selected = self.sidebar.selected_row();
        let sidebar_root = self.sidebar.root_display();
        let sidebar_focused = self.focus == NativeFocus::Sidebar;
        let start = scroll_row.min(rendered_lines.len());
        let end = (start + visible_rows).min(rendered_lines.len());
        let row_overlays = (start..end)
            .map(|row| {
                (
                    self.selection_range_for_row(row),
                    self.search_highlights_for_row(row),
                )
            })
            .collect::<Vec<_>>();

        let Some(surface) = self.surface.as_mut() else {
            return Ok(());
        };
        let mut buffer = surface
            .buffer_mut()
            .map_err(|error| anyhow!(error.to_string()))?;
        let width = size.width as usize;
        let height = size.height as usize;
        for pixel in buffer.iter_mut() {
            *pixel = color_or(BG, self.theme.background.bg);
        }

        fill_rect(&mut buffer, width, 0, 0, LEFT_RAIL_WIDTH, height, RAIL_BG);
        fill_rect(
            &mut buffer,
            width,
            LEFT_RAIL_WIDTH.saturating_sub(2),
            0,
            2,
            height,
            RAIL_ACCENT,
        );
        fill_rect(&mut buffer, width, 0, 0, width, TOP_BAR_HEIGHT, PANEL);
        fill_rect(
            &mut buffer,
            width,
            LEFT_RAIL_WIDTH,
            TOP_BAR_HEIGHT,
            width.saturating_sub(LEFT_RAIL_WIDTH),
            TAB_BAR_HEIGHT,
            TAB_BG,
        );
        let tab_title = self.document.display_title();
        let tab_width = ((tab_title.chars().count() + 4) * CHAR_WIDTH)
            .clamp(96, width.saturating_sub(LEFT_RAIL_WIDTH + 24));
        fill_rect(
            &mut buffer,
            width,
            LEFT_RAIL_WIDTH + 12,
            TOP_BAR_HEIGHT + 4,
            tab_width,
            TAB_BAR_HEIGHT.saturating_sub(6),
            TAB_ACTIVE_BG,
        );
        draw_text(
            &mut buffer,
            width,
            LEFT_RAIL_WIDTH + H_PADDING,
            10,
            &self.document.display_title(),
            color_or(ACCENT, self.theme.ui_chrome.fg),
        );
        draw_text(
            &mut buffer,
            width,
            LEFT_RAIL_WIDTH + H_PADDING,
            24,
            &abbreviate_middle(
                &self.document.subtitle(),
                max_cols(size.width).saturating_sub(10),
            ),
            MUTED,
        );
        draw_text(
            &mut buffer,
            width,
            LEFT_RAIL_WIDTH + 24,
            TOP_BAR_HEIGHT + 9,
            &abbreviate_middle(&tab_title, tab_width / CHAR_WIDTH - 2),
            TEXT,
        );

        fill_rect(
            &mut buffer,
            width,
            0,
            height.saturating_sub(BOTTOM_BAR_HEIGHT),
            width,
            BOTTOM_BAR_HEIGHT,
            PANEL,
        );
        let footer = format!(
            "Cmd+N new  Cmd+S save  Cmd+Shift+S save as  Cmd+R reload  Cmd+W close  Ln {} Col {}  {}",
            self.document.buffer.cursor().0 + 1,
            self.document.buffer.cursor().1 + 1,
            self.document.status
        );
        draw_text(
            &mut buffer,
            width,
            LEFT_RAIL_WIDTH + H_PADDING,
            height.saturating_sub(BOTTOM_BAR_HEIGHT).saturating_add(8),
            &abbreviate_middle(&footer, max_cols(size.width).saturating_sub(10)),
            if self.document.buffer.is_dirty() {
                STATUS_WARN
            } else {
                STATUS_OK
            },
        );

        let cursor_screen = self.document.buffer.cursor_screen_position();
        let content_x = LEFT_RAIL_WIDTH + sidebar_width + H_PADDING;
        let line_number_x = content_x;
        let git_gutter_x = line_number_x + LINE_NUMBER_WIDTH * CHAR_WIDTH + CHAR_WIDTH;
        let text_x = git_gutter_x + GIT_GUTTER_WIDTH * CHAR_WIDTH + CHAR_WIDTH;

        if self.sidebar.is_open() {
            draw_sidebar_panel(
                &mut buffer,
                width,
                height,
                SidebarRender {
                    width: sidebar_width,
                    root: &sidebar_root,
                    rows: &sidebar_rows,
                    selected_row: sidebar_selected,
                    focused: sidebar_focused,
                },
            );
        }

        for (index, line) in rendered_lines[start..end].iter().enumerate() {
            let row_index = start + index;
            let y = TOP_BAR_HEIGHT + TAB_BAR_HEIGHT + 10 + index * (CHAR_HEIGHT + LINE_SPACING);
            if y + CHAR_HEIGHT >= height.saturating_sub(BOTTOM_BAR_HEIGHT) {
                break;
            }

            if cursor_screen.map(|(_, row)| row) == Some(index) {
                fill_rect(
                    &mut buffer,
                    width,
                    LEFT_RAIL_WIDTH,
                    y.saturating_sub(2),
                    width.saturating_sub(LEFT_RAIL_WIDTH),
                    CHAR_HEIGHT + 4,
                    color_or(CURRENT_LINE, self.theme.cursor.bg),
                );
            }

            fill_rect(
                &mut buffer,
                width,
                line_number_x.saturating_sub(6),
                y.saturating_sub(1),
                (LINE_NUMBER_WIDTH + GIT_GUTTER_WIDTH + 2) * CHAR_WIDTH,
                CHAR_HEIGHT + 2,
                GUTTER_BG,
            );
            draw_text(
                &mut buffer,
                width,
                line_number_x,
                y,
                &format!("{:>4}", row_index + 1),
                MUTED,
            );
            if let Some(marker) = git_markers.get(row_index).and_then(|slot| *slot) {
                let marker_color = match marker {
                    gitdiff::LineChange::Added => color_or(ACCENT, self.theme.git_added.fg),
                    gitdiff::LineChange::Modified => {
                        color_or(STATUS_WARN, self.theme.git_modified.fg)
                    }
                    gitdiff::LineChange::Deleted => color_or(ERROR, self.theme.git_deleted.fg),
                };
                fill_rect(
                    &mut buffer,
                    width,
                    git_gutter_x + CHAR_WIDTH / 2,
                    y.saturating_sub(1),
                    3,
                    CHAR_HEIGHT + 2,
                    marker_color,
                );
            }

            let (selection, search) = &row_overlays[index];
            draw_styled_line(
                &mut buffer,
                width,
                line,
                StyledLineLayout {
                    x: text_x,
                    y,
                    scroll_col,
                    visible_cols,
                    default_fg: TEXT,
                    selection: *selection,
                    search: search.clone(),
                },
            );
        }

        if let Some((cursor_x, cursor_y)) = cursor_screen
            && cursor_y < visible_rows
        {
            let y = TOP_BAR_HEIGHT + TAB_BAR_HEIGHT + 10 + cursor_y * (CHAR_HEIGHT + LINE_SPACING);
            let x = text_x + cursor_x * CHAR_WIDTH;
            fill_rect(&mut buffer, width, x, y, 2, CHAR_HEIGHT, CURSOR);
        }

        draw_overlay(overlay, &mut buffer, width, height);

        buffer
            .present()
            .map_err(|error| anyhow!(error.to_string()))?;
        Ok(())
    }

    fn render_lines(&self, width: usize) -> Vec<Line<'static>> {
        match self.mode {
            NativeMode::Source => self
                .document
                .buffer
                .lines()
                .iter()
                .cloned()
                .map(Line::raw)
                .collect(),
            NativeMode::SourceHints => match self.document.file_type {
                filetype::FileType::Markdown => markdown_lines(self.document.buffer.lines(), &self.theme),
                filetype::FileType::Code | filetype::FileType::Unknown => code::render_document(
                    self.document.buffer.lines(),
                    &self.theme,
                    self.document.source_path.as_deref(),
                    self.document.file_type,
                ),
                _ => self.document.buffer.lines().iter().cloned().map(Line::raw).collect(),
            },
            NativeMode::Preview => match self.document.file_type {
                filetype::FileType::Code => code::render_preview_document(
                    self.document.buffer.lines(),
                    &self.theme,
                    self.document.source_path.as_deref(),
                    self.document.file_type,
                    width,
                ),
                _ => preview::render_document(self.document.buffer.lines(), &self.theme, width),
            },
        }
        .into_iter()
        .map(|line| clip_line(line, width))
        .collect()
    }

    fn sidebar_pixel_width(&self) -> usize {
        if self.sidebar.is_open() {
            self.sidebar.width() as usize * CHAR_WIDTH + SIDEBAR_PADDING * 2
        } else {
            0
        }
    }

    fn refresh_sidebar_for_document(&mut self) {
        let was_open = self.sidebar.is_open();
        let width = self.sidebar.width();
        self.sidebar = SidebarState::for_file(self.document.source_path.as_deref())
            .unwrap_or_else(|_| SidebarState::fallback());
        self.sidebar.set_width(width);
        if was_open {
            let _ = self.sidebar.open();
        }
    }

    fn open_search(&mut self) {
        self.overlay = Some(Overlay::Search(SearchState {
            input: self.search_query.clone(),
        }));
        self.document.status = String::from("Find");
    }

    fn apply_search_input(&mut self, input: &str) {
        self.search_query = input.to_owned();
        self.refresh_search_matches();
        if let Some(index) = self.search_current
            && let Some(search_match) = self.search_matches.get(index).copied()
        {
            self.document.buffer.move_to_search_match(search_match);
        }
    }

    fn refresh_search_matches(&mut self) {
        self.search_matches = self
            .document
            .buffer
            .search_matches_with_case(&self.search_query, false);
        self.search_current = if self.search_matches.is_empty() {
            None
        } else {
            Some(self.search_current.unwrap_or(0).min(self.search_matches.len() - 1))
        };
    }

    fn move_to_next_search_match(&mut self, forward: bool) {
        if self.search_matches.is_empty() {
            return;
        }
        let len = self.search_matches.len();
        let next = match (self.search_current, forward) {
            (Some(index), true) => (index + 1) % len,
            (Some(index), false) => (index + len - 1) % len,
            (None, _) => 0,
        };
        self.search_current = Some(next);
        if let Some(search_match) = self.search_matches.get(next).copied() {
            self.document.buffer.move_to_search_match(search_match);
        }
    }

    fn current_selection_range(&self) -> Option<SelectionRange> {
        let anchor = self.selection_anchor?;
        let cursor = self.document.buffer.cursor();
        if anchor == cursor {
            return None;
        }
        let (start, end) = if anchor <= cursor {
            (anchor, cursor)
        } else {
            (cursor, anchor)
        };
        Some(SelectionRange { start, end })
    }

    fn selection_range_for_row(&self, row: usize) -> Option<(usize, usize)> {
        let range = self.current_selection_range()?;
        if row < range.start.0 || row > range.end.0 {
            return None;
        }
        if range.start.0 == range.end.0 {
            return Some((range.start.1, range.end.1.max(range.start.1 + 1)));
        }
        if row == range.start.0 {
            return Some((range.start.1, self.document.buffer.lines()[row].chars().count()));
        }
        if row == range.end.0 {
            return Some((0, range.end.1.max(1)));
        }
        Some((0, self.document.buffer.lines()[row].chars().count().max(1)))
    }

    fn search_highlights_for_row(&self, row: usize) -> Vec<SearchHighlight> {
        self.search_matches
            .iter()
            .enumerate()
            .filter(|(_, matched)| matched.row == row)
            .map(|(index, matched)| SearchHighlight {
                start: matched.col,
                len: matched.len.max(1),
                current: self.search_current == Some(index),
            })
            .collect()
    }

    fn move_cursor_left(&mut self, selecting: bool) {
        self.before_motion(selecting);
        self.document.buffer.move_left();
        self.after_motion(selecting);
    }

    fn move_cursor_right(&mut self, selecting: bool) {
        self.before_motion(selecting);
        self.document.buffer.move_right();
        self.after_motion(selecting);
    }

    fn move_cursor_up(&mut self, selecting: bool) {
        self.before_motion(selecting);
        self.document.buffer.move_up();
        self.after_motion(selecting);
    }

    fn move_cursor_down(&mut self, selecting: bool) {
        self.before_motion(selecting);
        self.document.buffer.move_down();
        self.after_motion(selecting);
    }

    fn before_motion(&mut self, selecting: bool) {
        if selecting && self.selection_anchor.is_none() {
            self.selection_anchor = Some(self.document.buffer.cursor());
        }
        if !selecting {
            self.selection_anchor = None;
        }
    }

    fn after_motion(&mut self, selecting: bool) {
        if selecting && self.selection_anchor == Some(self.document.buffer.cursor()) {
            self.selection_anchor = None;
        }
    }
}

impl NativeMode {
    fn for_file_type(file_type: filetype::FileType) -> Self {
        match file_type {
            filetype::FileType::Markdown => Self::SourceHints,
            filetype::FileType::Code => Self::SourceHints,
            _ => Self::Source,
        }
    }

    fn next(self) -> Self {
        match self {
            Self::Source => Self::SourceHints,
            Self::SourceHints => Self::Preview,
            Self::Preview => Self::Source,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::SourceHints => "source+hints",
            Self::Preview => "preview",
        }
    }
}

impl ApplicationHandler for WindowApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let Ok(window) = event_loop.create_window(self.window_attributes()) else {
            event_loop.exit();
            return;
        };
        let window = Rc::new(window);
        let Ok(context) = SoftbufferContext::new(window.clone()) else {
            event_loop.exit();
            return;
        };
        let Ok(surface) = Surface::new(&context, window.clone()) else {
            event_loop.exit();
            return;
        };
        window.set_ime_allowed(true);

        self.window = Some(window);
        self.context = Some(context);
        self.surface = Some(surface);

        if self
            .ensure_surface_size(self.window.as_ref().expect("window").inner_size())
            .is_err()
        {
            event_loop.exit();
            return;
        }
        self.request_redraw();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                if self.request_close() {
                    event_loop.exit();
                }
            }
            WindowEvent::DroppedFile(path) => {
                self.open_document(&path);
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers.state();
            }
            WindowEvent::Resized(size) => {
                if self.ensure_surface_size(size).is_ok() {
                    self.request_redraw();
                } else {
                    event_loop.exit();
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let amount = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -(y.round() as isize),
                    MouseScrollDelta::PixelDelta(position) => -(position.y / 40.0).round() as isize,
                };
                if amount != 0 {
                    self.handle_scroll_lines(amount);
                    let size = self
                        .window
                        .as_ref()
                        .map(|window| window.inner_size())
                        .unwrap_or(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));
                    self.document
                        .buffer
                        .sync_viewport(self.visible_rows(size), self.visible_cols(size));
                    self.request_redraw();
                }
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let size = self
                    .window
                    .as_ref()
                    .map(|window| window.inner_size())
                    .unwrap_or(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));
                let page = self.visible_rows(size).max(1) as isize;
                if let Some(action) = self.handle_overlay_key(&event) {
                    match action {
                        DeferredAction::Close => event_loop.exit(),
                    }
                    return;
                }
                let selecting = self.modifiers.shift_key();
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        if self.focus == NativeFocus::Sidebar {
                            self.focus = NativeFocus::Editor;
                            self.request_redraw();
                            return;
                        }
                        if self.request_close() {
                            event_loop.exit();
                        }
                        return;
                    }
                    Key::Named(NamedKey::ArrowUp) if self.focus == NativeFocus::Sidebar => {
                        self.sidebar.move_up();
                    }
                    Key::Named(NamedKey::ArrowDown) if self.focus == NativeFocus::Sidebar => {
                        self.sidebar.move_down();
                    }
                    Key::Named(NamedKey::ArrowLeft) if self.focus == NativeFocus::Sidebar => {
                        let _ = self.sidebar.move_left();
                    }
                    Key::Named(NamedKey::ArrowRight) if self.focus == NativeFocus::Sidebar => {
                        let _ = self.sidebar.toggle_selected_dir();
                    }
                    Key::Named(NamedKey::PageUp) if self.focus == NativeFocus::Sidebar => {
                        self.sidebar.page_up(page as usize);
                    }
                    Key::Named(NamedKey::PageDown) if self.focus == NativeFocus::Sidebar => {
                        self.sidebar.page_down(page as usize);
                    }
                    Key::Named(NamedKey::Enter) if self.focus == NativeFocus::Sidebar => {
                        if let Ok(action) = self.sidebar.open_selected()
                            && let SidebarAction::OpenFile(path) = action
                        {
                            self.focus = NativeFocus::Editor;
                            self.open_document(&path);
                            return;
                        }
                    }
                    Key::Named(NamedKey::Tab) if self.sidebar.is_open() => {
                        self.focus = match self.focus {
                            NativeFocus::Editor => NativeFocus::Sidebar,
                            NativeFocus::Sidebar => NativeFocus::Editor,
                        };
                    }
                    Key::Named(NamedKey::ArrowUp) => self.move_cursor_up(selecting),
                    Key::Named(NamedKey::ArrowDown) => self.move_cursor_down(selecting),
                    Key::Named(NamedKey::ArrowLeft) => self.move_cursor_left(selecting),
                    Key::Named(NamedKey::ArrowRight) => self.move_cursor_right(selecting),
                    Key::Named(NamedKey::PageUp) => {
                        self.before_motion(selecting);
                        self.document.buffer.page_up(page as usize);
                        self.after_motion(selecting);
                    }
                    Key::Named(NamedKey::PageDown) => {
                        self.before_motion(selecting);
                        self.document.buffer.page_down(page as usize);
                        self.after_motion(selecting);
                    }
                    Key::Named(NamedKey::Home) => {
                        self.before_motion(selecting);
                        self.document.buffer.move_home();
                        self.after_motion(selecting);
                    }
                    Key::Named(NamedKey::End) => {
                        self.before_motion(selecting);
                        self.document.buffer.move_end();
                        self.after_motion(selecting);
                    }
                    Key::Named(NamedKey::Backspace) if self.mode != NativeMode::Preview => {
                        self.selection_anchor = None;
                        self.document.buffer.backspace();
                    }
                    Key::Named(NamedKey::Delete) if self.mode != NativeMode::Preview => {
                        self.selection_anchor = None;
                        self.document.buffer.delete_forward();
                    }
                    Key::Named(NamedKey::Enter) if self.mode != NativeMode::Preview => {
                        self.selection_anchor = None;
                        self.document.buffer.insert_newline();
                    }
                    Key::Named(NamedKey::Tab) if self.mode != NativeMode::Preview => {
                        self.selection_anchor = None;
                        self.document.buffer.insert_spaces(4);
                    }
                    Key::Character(text)
                        if self.modifiers.super_key() && text.eq_ignore_ascii_case("w") =>
                    {
                        if self.request_close() {
                            event_loop.exit();
                        }
                        return;
                    }
                    Key::Character(text)
                        if self.modifiers.super_key() && text.eq_ignore_ascii_case("n") =>
                    {
                        self.document.status = match launch_new_window(None) {
                            Ok(()) => String::from("Opened a new window"),
                            Err(error) => error.to_string(),
                        };
                    }
                    Key::Character(text)
                        if self.modifiers.super_key() && text.eq_ignore_ascii_case("f") =>
                    {
                        self.open_search();
                    }
                    Key::Character(text)
                        if self.modifiers.super_key() && text.eq_ignore_ascii_case("e") =>
                    {
                        if self.sidebar.is_open() {
                            self.sidebar.close();
                            self.focus = NativeFocus::Editor;
                            self.document.status = String::from("Sidebar hidden");
                        } else if self.sidebar.open().is_ok() {
                            self.focus = NativeFocus::Sidebar;
                            self.document.status = String::from("Sidebar shown");
                        }
                    }
                    Key::Character(text)
                        if self.modifiers.super_key() && text.eq_ignore_ascii_case("p") =>
                    {
                        self.mode = self.mode.next();
                        self.document.status = format!("Mode: {}", self.mode.label());
                    }
                    Key::Character(text)
                        if self.modifiers.alt_key() && text.eq_ignore_ascii_case("n") =>
                    {
                        self.move_to_next_search_match(true);
                    }
                    Key::Character(text)
                        if self.modifiers.alt_key() && text.eq_ignore_ascii_case("p") =>
                    {
                        self.move_to_next_search_match(false);
                    }
                    Key::Character(text)
                        if self.modifiers.super_key()
                            && self.modifiers.shift_key()
                            && text.eq_ignore_ascii_case("s") =>
                    {
                        self.open_save_as(None);
                    }
                    Key::Character(text)
                        if self.modifiers.super_key() && text.eq_ignore_ascii_case("s") =>
                    {
                        self.save_document();
                    }
                    Key::Character(text)
                        if self.modifiers.super_key() && text.eq_ignore_ascii_case("r") =>
                    {
                        self.reload_document();
                    }
                    Key::Character(text)
                        if self.mode != NativeMode::Preview
                            && !self.modifiers.super_key()
                            && !self.modifiers.control_key()
                            && !self.modifiers.alt_key() =>
                    {
                        self.selection_anchor = None;
                        for ch in text.chars().filter(|ch| !ch.is_control()) {
                            self.document.buffer.insert_char(ch);
                        }
                    }
                    _ => return,
                }
                self.document
                    .buffer
                    .sync_viewport(self.visible_rows(size), self.visible_cols(size));
                self.sidebar.sync_viewport(self.visible_rows(size));
                self.refresh_search_matches();
                self.sync_window_title();
                self.request_redraw();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if self.modifiers.super_key()
                    || self.modifiers.control_key()
                    || self.modifiers.alt_key()
                    || self.mode == NativeMode::Preview
                {
                    return;
                }
                self.selection_anchor = None;
                for ch in text.chars().filter(|ch| !ch.is_control()) {
                    self.document.buffer.insert_char(ch);
                }
                let size = self
                    .window
                    .as_ref()
                    .map(|window| window.inner_size())
                    .unwrap_or(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT));
                self.document
                    .buffer
                    .sync_viewport(self.visible_rows(size), self.visible_cols(size));
                self.refresh_search_matches();
                self.sync_window_title();
                self.request_redraw();
            }
            WindowEvent::RedrawRequested => {
                if self.draw().is_err() {
                    event_loop.exit();
                }
            }
            _ => {}
        }
    }
}

fn max_cols(width: u32) -> usize {
    (width as usize).saturating_sub(H_PADDING * 2) / CHAR_WIDTH.max(1)
}

#[cfg(test)]
fn visible_fragment(line: &str, scroll_col: usize, visible_cols: usize) -> String {
    line.chars()
        .skip(scroll_col)
        .take(visible_cols)
        .collect::<String>()
}

fn clip_line(line: Line<'static>, width: usize) -> Line<'static> {
    if width == 0 {
        return Line::default();
    }

    let mut spans = Vec::new();
    let mut remaining = width;
    for span in line.spans {
        if remaining == 0 {
            break;
        }
        let content = span.content.chars().take(remaining).collect::<String>();
        let used = content.chars().count();
        if used > 0 {
            spans.push(Span::styled(content, span.style));
            remaining = remaining.saturating_sub(used);
        }
    }
    Line::from(spans)
}

fn suggested_save_path(source_path: Option<&Path>) -> PathBuf {
    source_path.map(Path::to_path_buf).unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("Untitled.md")
    })
}

fn markdown_lines(lines: &[String], theme: &Theme) -> Vec<Line<'static>> {
    crate::markdown::style_document(lines, theme)
}

struct SidebarRender<'a> {
    width: usize,
    root: &'a str,
    rows: &'a [crate::sidebar::SidebarRow],
    selected_row: Option<usize>,
    focused: bool,
}

fn draw_sidebar_panel(
    buffer: &mut [u32],
    width: usize,
    height: usize,
    sidebar: SidebarRender<'_>,
) {
    if sidebar.width == 0 {
        return;
    }

    let x = LEFT_RAIL_WIDTH;
    fill_rect(buffer, width, x, TOP_BAR_HEIGHT, sidebar.width, height, TAB_BG);
    fill_rect(
        buffer,
        width,
        x + sidebar.width.saturating_sub(2),
        TOP_BAR_HEIGHT,
        2,
        height.saturating_sub(TOP_BAR_HEIGHT),
        DIALOG_BORDER,
    );
    draw_text(
        buffer,
        width,
        x + SIDEBAR_PADDING,
        TOP_BAR_HEIGHT + 9,
        &abbreviate_middle(sidebar.root, sidebar.width / CHAR_WIDTH - 4),
        if sidebar.focused { ACCENT } else { MUTED },
    );
    for (index, row) in sidebar.rows.iter().enumerate() {
        let y = TOP_BAR_HEIGHT + TAB_BAR_HEIGHT + 10 + index * (CHAR_HEIGHT + LINE_SPACING);
        if Some(index) == sidebar.selected_row {
            fill_rect(
                buffer,
                width,
                x,
                y.saturating_sub(2),
                sidebar.width,
                CHAR_HEIGHT + 4,
                if sidebar.focused {
                    TAB_ACTIVE_BG
                } else {
                    CURRENT_LINE
                },
            );
        }
        if let Some(marker) = row.marker {
            draw_text(buffer, width, x + SIDEBAR_PADDING, y, &marker.to_string(), MUTED);
        }
        draw_text(
            buffer,
            width,
            x + SIDEBAR_PADDING + CHAR_WIDTH * 2,
            y,
            &abbreviate_middle(&row.label, sidebar.width / CHAR_WIDTH - 6),
            TEXT,
        );
    }
}

struct StyledLineLayout {
    x: usize,
    y: usize,
    scroll_col: usize,
    visible_cols: usize,
    default_fg: u32,
    selection: Option<(usize, usize)>,
    search: Vec<SearchHighlight>,
}

fn draw_styled_line(
    buffer: &mut [u32],
    width: usize,
    line: &Line<'static>,
    layout: StyledLineLayout,
) {
    let mut col = 0usize;
    for span in &line.spans {
        let fg = color_or(layout.default_fg, span.style.fg);
        let bg = span.style.bg.map(color_to_u32);
        for ch in span.content.chars() {
            if col >= layout.scroll_col + layout.visible_cols {
                return;
            }
            if col >= layout.scroll_col {
                let draw_x = layout.x + (col - layout.scroll_col) * CHAR_WIDTH;
                let search_bg = layout
                    .search
                    .iter()
                    .find(|highlight| col >= highlight.start && col < highlight.start + highlight.len)
                    .map(|highlight| if highlight.current { SEARCH_CURRENT_BG } else { SEARCH_MATCH_BG });
                let selection_bg = layout
                    .selection
                    .filter(|(start, end)| col >= *start && col < *end)
                    .map(|_| color_or(TAB_ACTIVE_BG, Some(Color::Rgb(52, 73, 94))))
                    .or(bg);
                let final_bg = search_bg.or(selection_bg);
                if let Some(bg) = final_bg {
                    fill_rect(
                        buffer,
                        width,
                        draw_x,
                        layout.y.saturating_sub(1),
                        CHAR_WIDTH,
                        CHAR_HEIGHT + 2,
                        bg,
                    );
                }
                draw_char(buffer, width, draw_x, layout.y, ch, fg);
            }
            col += 1;
        }
    }
}

fn color_or(default: u32, color: Option<Color>) -> u32 {
    color.map(color_to_u32).unwrap_or(default)
}

fn color_to_u32(color: Color) -> u32 {
    match color {
        Color::Reset => BG,
        Color::Black => 0x000000,
        Color::DarkGray => 0x555555,
        Color::Gray => 0x888888,
        Color::White => 0xffffff,
        Color::Red => 0xff5555,
        Color::LightRed => 0xff7b72,
        Color::Green => 0x50fa7b,
        Color::LightGreen => 0x8be9a8,
        Color::Yellow => 0xf1fa8c,
        Color::LightYellow => 0xffd866,
        Color::Blue => 0x5e81ac,
        Color::LightBlue => 0x82aaff,
        Color::Magenta => 0xc792ea,
        Color::LightMagenta => 0xff79c6,
        Color::Cyan => 0x4fd1c5,
        Color::LightCyan => 0x8be9fd,
        Color::Rgb(r, g, b) => ((r as u32) << 16) | ((g as u32) << 8) | (b as u32),
        Color::Indexed(index) => {
            let shade = index as u32;
            (shade << 16) | (shade << 8) | shade
        }
    }
}

fn abbreviate_middle(text: &str, max_chars: usize) -> String {
    let chars = text.chars().collect::<Vec<_>>();
    if chars.len() <= max_chars {
        return text.to_owned();
    }
    if max_chars <= 1 {
        return String::from("…");
    }

    let head = (max_chars - 1) / 2;
    let tail = max_chars - 1 - head;
    format!(
        "{}…{}",
        chars[..head].iter().collect::<String>(),
        chars[chars.len() - tail..].iter().collect::<String>()
    )
}

fn draw_overlay(overlay: Option<&Overlay>, buffer: &mut [u32], width: usize, height: usize) {
    let Some(overlay) = overlay else {
        return;
    };

    let dialog_width = width.saturating_sub(120).clamp(420, 760);
    let dialog_height = match overlay {
        Overlay::SaveAs(_) => 146,
        Overlay::ConfirmClose => 112,
        Overlay::Search(_) => 104,
    };
    let x = width.saturating_sub(dialog_width) / 2;
    let y = height.saturating_sub(dialog_height) / 2;

    fill_rect(buffer, width, x, y, dialog_width, dialog_height, DIALOG_BG);
    fill_rect(buffer, width, x, y, dialog_width, 2, DIALOG_BORDER);
    fill_rect(
        buffer,
        width,
        x,
        y + dialog_height.saturating_sub(2),
        dialog_width,
        2,
        DIALOG_BORDER,
    );
    fill_rect(buffer, width, x, y, 2, dialog_height, DIALOG_BORDER);
    fill_rect(
        buffer,
        width,
        x + dialog_width.saturating_sub(2),
        y,
        2,
        dialog_height,
        DIALOG_BORDER,
    );

    match overlay {
        Overlay::SaveAs(state) => {
            draw_text(buffer, width, x + 16, y + 14, "Save As", ACCENT);
            draw_text(buffer, width, x + 16, y + 38, "Path", MUTED);
            fill_rect(
                buffer,
                width,
                x + 16,
                y + 54,
                dialog_width.saturating_sub(32),
                28,
                PANEL,
            );
            draw_text(
                buffer,
                width,
                x + 22,
                y + 61,
                &abbreviate_middle(&state.input, dialog_width / CHAR_WIDTH - 6),
                TEXT,
            );
            draw_text(buffer, width, x + 16, y + 92, "Enter save  Esc cancel", MUTED);
            let status = state
                .error
                .as_deref()
                .unwrap_or("Save untitled docs to a real path");
            draw_text(
                buffer,
                width,
                x + 16,
                y + 112,
                &abbreviate_middle(status, dialog_width / CHAR_WIDTH - 6),
                if state.error.is_some() { ERROR } else { STATUS_OK },
            );
        }
        Overlay::ConfirmClose => {
            draw_text(buffer, width, x + 16, y + 14, "Unsaved Changes", ACCENT);
            draw_text(
                buffer,
                width,
                x + 16,
                y + 42,
                "Save before closing this window?",
                TEXT,
            );
            draw_text(
                buffer,
                width,
                x + 16,
                y + 72,
                "S save  D discard  Esc cancel",
                MUTED,
            );
        }
        Overlay::Search(state) => {
            draw_text(buffer, width, x + 16, y + 14, "Find", ACCENT);
            fill_rect(
                buffer,
                width,
                x + 16,
                y + 42,
                dialog_width.saturating_sub(32),
                28,
                PANEL,
            );
            draw_text(
                buffer,
                width,
                x + 22,
                y + 49,
                &abbreviate_middle(&state.input, dialog_width / CHAR_WIDTH - 6),
                TEXT,
            );
            draw_text(
                buffer,
                width,
                x + 16,
                y + 80,
                "Enter next  Alt+N/P cycle  Esc close",
                MUTED,
            );
        }
    }
}

fn fill_rect(
    buffer: &mut [u32],
    width: usize,
    x: usize,
    y: usize,
    rect_width: usize,
    rect_height: usize,
    color: u32,
) {
    if width == 0 {
        return;
    }
    let height = buffer.len() / width;
    let max_y = (y + rect_height).min(height);
    let max_x = (x + rect_width).min(width);
    for row in y..max_y {
        let start = row * width;
        for col in x..max_x {
            buffer[start + col] = color;
        }
    }
}

fn draw_text(buffer: &mut [u32], width: usize, x: usize, y: usize, text: &str, color: u32) {
    for (index, ch) in text.chars().enumerate() {
        draw_char(buffer, width, x + index * CHAR_WIDTH, y, ch, color);
    }
}

fn draw_char(buffer: &mut [u32], width: usize, x: usize, y: usize, ch: char, color: u32) {
    let Some(glyph) = BASIC_FONTS.get(ch).or_else(|| BASIC_FONTS.get('?')) else {
        return;
    };
    let height = buffer.len() / width;
    for (row, bits) in glyph.iter().enumerate() {
        for col in 0..8 {
            if (bits >> col) & 1 == 0 {
                continue;
            }

            let px = x + col;
            let py = y + row * 2;
            if px >= width || py + 1 >= height {
                continue;
            }

            buffer[py * width + px] = color;
            buffer[(py + 1) * width + px] = color;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_new_window_args_adds_app_flag() {
        let args = build_new_window_args(Some(Path::new("notes.md")));
        assert_eq!(args, vec![String::from("--app"), String::from("notes.md")]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bundle_root_is_detected_from_executable_path() {
        let exe = Path::new("/Applications/Implicit.app/Contents/MacOS/Implicit");
        assert_eq!(
            bundle_root_for_executable(exe),
            Some(PathBuf::from("/Applications/Implicit.app"))
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn open_bundle_args_include_new_instance_and_file() {
        let args = build_open_bundle_args(
            Path::new("/Applications/Implicit.app"),
            Some(Path::new("note.md")),
        );
        assert_eq!(
            args,
            vec![
                String::from("-n"),
                String::from("-a"),
                String::from("/Applications/Implicit.app"),
                String::from("--args"),
                String::from("note.md")
            ]
        );
    }

    #[test]
    fn visible_fragment_applies_horizontal_scroll() {
        let clipped = visible_fragment("abcdefgh", 2, 3);
        assert_eq!(clipped, "cde");
    }

    #[test]
    fn abbreviate_middle_preserves_ends() {
        let result = abbreviate_middle("abcdefghijklmnopqrstuvwxyz", 9);
        assert_eq!(result, "abcd…wxyz");
    }

    #[test]
    fn load_document_without_path_returns_welcome() {
        let document = load_document(None);
        assert_eq!(document.display_title(), "Untitled");
        assert_eq!(document.buffer.lines(), [""]);
        assert_eq!(document.status, "Untitled");
    }

    #[test]
    fn load_document_from_path_reads_file_contents() {
        let path = std::env::temp_dir().join("implicit-window-app-load.txt");
        fs::write(&path, "hello\nworld").expect("write temp file");

        let document = load_document(Some(&path));

        assert_eq!(document.source_path.as_deref(), Some(path.as_path()));
        assert_eq!(document.display_title(), "implicit-window-app-load.txt");
        assert_eq!(document.buffer.lines(), ["hello", "world"]);

        fs::remove_file(path).expect("cleanup");
    }

    #[test]
    fn missing_path_opens_editable_new_file() {
        let path = std::env::temp_dir().join("implicit-window-app-missing.txt");
        let _ = fs::remove_file(&path);

        let document = load_document(Some(&path));

        assert_eq!(document.source_path.as_deref(), Some(path.as_path()));
        assert_eq!(document.status, "New file");
        assert_eq!(document.buffer.lines(), [""]);
    }

    #[test]
    fn dirty_document_title_includes_marker() {
        let mut document = NativeDocument::message(None, vec![String::from("alpha")], "Ready");
        document.buffer.move_end();
        document.buffer.insert_char('!');

        assert_eq!(document.display_title(), "Untitled •");
    }

    #[test]
    fn suggested_save_path_defaults_to_untitled_markdown() {
        let path = suggested_save_path(None);
        assert_eq!(path.file_name().and_then(|name| name.to_str()), Some("Untitled.md"));
    }

    #[test]
    fn request_close_prompts_for_dirty_document() {
        let mut app = WindowApp::new(load_document(None));
        app.document.buffer.insert_char('x');

        assert!(!app.request_close());
        assert!(matches!(app.overlay, Some(Overlay::ConfirmClose)));
    }

    #[test]
    fn save_document_to_sets_source_path_and_writes_contents() {
        let path = std::env::temp_dir().join("implicit-window-app-save.txt");
        let _ = fs::remove_file(&path);

        let mut app = WindowApp::new(NativeDocument {
            source_path: None,
            buffer: Buffer::empty(),
            status: String::from("Ready"),
            file_type: filetype::FileType::Markdown,
        });
        app.document.buffer.insert_char('x');
        let result = app.save_document_to(path.clone(), None);

        assert_eq!(result, None);
        assert_eq!(app.document.source_path.as_deref(), Some(path.as_path()));
        assert_eq!(fs::read_to_string(&path).expect("saved file"), "x");

        fs::remove_file(path).expect("cleanup");
    }
}
