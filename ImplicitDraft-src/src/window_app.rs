use std::{
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    rc::Rc,
};

use anyhow::{Context, Result, anyhow, bail};
use font8x8::{BASIC_FONTS, UnicodeFonts};
use softbuffer::{Context as SoftbufferContext, Surface};
use winit::{
    application::ApplicationHandler,
    dpi::{LogicalSize, PhysicalSize},
    event::{ElementState, Ime, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowAttributes, WindowId},
};

use crate::{buffer::Buffer, filetype};

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
}

impl NativeDocument {
    fn message(source_path: Option<PathBuf>, lines: Vec<String>, status: &str) -> Self {
        Self {
            source_path,
            buffer: Buffer::from_text(&lines.join("\n")),
            status: status.to_owned(),
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
            .unwrap_or_else(|| String::from("Native app mode"))
    }
}

fn load_document(path: Option<&Path>) -> NativeDocument {
    match path {
        Some(path) => match fs::read_to_string(path) {
            Ok(contents) => NativeDocument {
                source_path: Some(path.to_path_buf()),
                buffer: Buffer::from_text(&contents),
                status: String::from("Opened file"),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => NativeDocument {
                source_path: Some(path.to_path_buf()),
                buffer: Buffer::empty(),
                status: String::from("New file"),
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
}

impl WindowApp {
    fn new(document: NativeDocument) -> Self {
        Self {
            document,
            window: None,
            context: None,
            surface: None,
            modifiers: ModifiersState::default(),
            overlay: None,
        }
    }

    fn set_document(&mut self, document: NativeDocument) {
        self.document = document;
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
        let content_height =
            size.height
                .saturating_sub((TOP_BAR_HEIGHT + BOTTOM_BAR_HEIGHT) as u32) as usize;
        let row_height = CHAR_HEIGHT + LINE_SPACING;
        (content_height / row_height).max(1)
    }

    fn visible_cols(&self, size: PhysicalSize<u32>) -> usize {
        max_cols(size.width).max(1)
    }

    fn handle_scroll_lines(&mut self, delta: isize) {
        if delta.is_negative() {
            for _ in 0..delta.unsigned_abs() {
                self.document.buffer.move_up();
            }
        } else {
            for _ in 0..delta as usize {
                self.document.buffer.move_down();
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

        let Some(surface) = self.surface.as_mut() else {
            return Ok(());
        };
        let mut buffer = surface
            .buffer_mut()
            .map_err(|error| anyhow!(error.to_string()))?;
        let width = size.width as usize;
        let height = size.height as usize;
        for pixel in buffer.iter_mut() {
            *pixel = BG;
        }

        fill_rect(&mut buffer, width, 0, 0, width, TOP_BAR_HEIGHT, PANEL);
        draw_text(
            &mut buffer,
            width,
            H_PADDING,
            10,
            &self.document.display_title(),
            ACCENT,
        );
        draw_text(
            &mut buffer,
            width,
            H_PADDING,
            24,
            &abbreviate_middle(
                &self.document.subtitle(),
                max_cols(size.width).saturating_sub(4),
            ),
            MUTED,
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
            H_PADDING,
            height.saturating_sub(BOTTOM_BAR_HEIGHT).saturating_add(8),
            &abbreviate_middle(&footer, max_cols(size.width).saturating_sub(2)),
            if self.document.buffer.is_dirty() {
                STATUS_WARN
            } else {
                STATUS_OK
            },
        );

        let visible_lines = self.document.buffer.lines();
        let start = scroll_row.min(visible_lines.len());
        let end = (start + visible_rows).min(visible_lines.len());
        let cursor_screen = self.document.buffer.cursor_screen_position();
        for (index, line) in visible_lines[start..end].iter().enumerate() {
            let y = TOP_BAR_HEIGHT + 10 + index * (CHAR_HEIGHT + LINE_SPACING);
            if y + CHAR_HEIGHT >= height.saturating_sub(BOTTOM_BAR_HEIGHT) {
                break;
            }
            if cursor_screen.map(|(_, row)| row) == Some(index) {
                fill_rect(
                    &mut buffer,
                    width,
                    0,
                    y.saturating_sub(2),
                    width,
                    CHAR_HEIGHT + 4,
                    CURRENT_LINE,
                );
            }
            let clipped = visible_fragment(line, scroll_col, visible_cols);
            draw_text(&mut buffer, width, H_PADDING, y, &clipped, TEXT);
        }

        if let Some((cursor_x, cursor_y)) = cursor_screen
            && cursor_y < visible_rows
        {
            let y = TOP_BAR_HEIGHT + 10 + cursor_y * (CHAR_HEIGHT + LINE_SPACING);
            let x = H_PADDING + cursor_x * CHAR_WIDTH;
            fill_rect(&mut buffer, width, x, y, 2, CHAR_HEIGHT, CURSOR);
        }

        draw_overlay(self.overlay.as_ref(), &mut buffer, width, height);

        buffer
            .present()
            .map_err(|error| anyhow!(error.to_string()))?;
        Ok(())
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
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        if self.request_close() {
                            event_loop.exit();
                        }
                        return;
                    }
                    Key::Named(NamedKey::ArrowUp) => self.document.buffer.move_up(),
                    Key::Named(NamedKey::ArrowDown) => self.document.buffer.move_down(),
                    Key::Named(NamedKey::ArrowLeft) => self.document.buffer.move_left(),
                    Key::Named(NamedKey::ArrowRight) => self.document.buffer.move_right(),
                    Key::Named(NamedKey::PageUp) => self.document.buffer.page_up(page as usize),
                    Key::Named(NamedKey::PageDown) => self.document.buffer.page_down(page as usize),
                    Key::Named(NamedKey::Home) => self.document.buffer.move_home(),
                    Key::Named(NamedKey::End) => self.document.buffer.move_end(),
                    Key::Named(NamedKey::Backspace) => self.document.buffer.backspace(),
                    Key::Named(NamedKey::Delete) => self.document.buffer.delete_forward(),
                    Key::Named(NamedKey::Enter) => self.document.buffer.insert_newline(),
                    Key::Named(NamedKey::Tab) => self.document.buffer.insert_spaces(4),
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
                        if !self.modifiers.super_key()
                            && !self.modifiers.control_key()
                            && !self.modifiers.alt_key() =>
                    {
                        for ch in text.chars().filter(|ch| !ch.is_control()) {
                            self.document.buffer.insert_char(ch);
                        }
                    }
                    _ => return,
                }
                self.document
                    .buffer
                    .sync_viewport(self.visible_rows(size), self.visible_cols(size));
                self.sync_window_title();
                self.request_redraw();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                if self.modifiers.super_key()
                    || self.modifiers.control_key()
                    || self.modifiers.alt_key()
                {
                    return;
                }
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

fn visible_fragment(line: &str, scroll_col: usize, visible_cols: usize) -> String {
    line.chars()
        .skip(scroll_col)
        .take(visible_cols)
        .collect::<String>()
}

fn suggested_save_path(source_path: Option<&Path>) -> PathBuf {
    source_path.map(Path::to_path_buf).unwrap_or_else(|| {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("Untitled.md")
    })
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
        });
        app.document.buffer.insert_char('x');
        let result = app.save_document_to(path.clone(), None);

        assert_eq!(result, None);
        assert_eq!(app.document.source_path.as_deref(), Some(path.as_path()));
        assert_eq!(fs::read_to_string(&path).expect("saved file"), "x");

        fs::remove_file(path).expect("cleanup");
    }
}
