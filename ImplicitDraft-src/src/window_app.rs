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
    event::{ElementState, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, ModifiersState, NamedKey},
    window::{Window, WindowAttributes, WindowId},
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

#[derive(Clone, Debug)]
struct ViewerDocument {
    source_path: Option<PathBuf>,
    title: String,
    subtitle: String,
    lines: Vec<String>,
}

fn load_document(path: Option<&Path>) -> ViewerDocument {
    match path {
        Some(path) => match fs::read_to_string(path) {
            Ok(contents) => ViewerDocument {
                source_path: Some(path.to_path_buf()),
                title: path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Implicit")
                    .to_owned(),
                subtitle: path.display().to_string(),
                lines: split_lines(&contents),
            },
            Err(error) => ViewerDocument {
                source_path: Some(path.to_path_buf()),
                title: String::from("Implicit"),
                subtitle: path.display().to_string(),
                lines: vec![
                    String::from("Failed to open file."),
                    String::new(),
                    error.to_string(),
                ],
            },
        },
        None => ViewerDocument {
            source_path: None,
            title: String::from("Implicit"),
            subtitle: String::from("Native app mode"),
            lines: vec![
                String::from("Implicit native window mode"),
                String::new(),
                String::from("Open a file with:"),
                String::from("implicit --app /path/to/note.md"),
                String::from("implicit --new-window /path/to/note.md"),
                String::new(),
                String::from("Keys: Esc close, Up/Down scroll, PageUp/PageDown move faster."),
            ],
        },
    }
}

fn split_lines(contents: &str) -> Vec<String> {
    let mut lines = contents
        .split('\n')
        .map(|line| {
            line.strip_suffix('\r')
                .unwrap_or(line)
                .replace('\t', "    ")
        })
        .collect::<Vec<_>>();
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

struct WindowApp {
    document: ViewerDocument,
    window: Option<Rc<Window>>,
    context: Option<SoftbufferContext<Rc<Window>>>,
    surface: Option<Surface<Rc<Window>, Rc<Window>>>,
    modifiers: ModifiersState,
    scroll_line: usize,
}

impl WindowApp {
    fn new(document: ViewerDocument) -> Self {
        Self {
            document,
            window: None,
            context: None,
            surface: None,
            modifiers: ModifiersState::default(),
            scroll_line: 0,
        }
    }

    fn set_document(&mut self, document: ViewerDocument) {
        self.document = document;
        self.scroll_line = 0;
        if let Some(window) = &self.window {
            window.set_title(&format!("Implicit — {}", self.document.title));
        }
    }

    fn open_document(&mut self, path: &Path) {
        self.set_document(load_document(Some(path)));
        self.request_redraw();
    }

    fn reload_document(&mut self) {
        if let Some(path) = self.document.source_path.clone() {
            self.set_document(load_document(Some(&path)));
            self.request_redraw();
        }
    }

    fn window_attributes(&self) -> WindowAttributes {
        Window::default_attributes()
            .with_title(format!("Implicit — {}", self.document.title))
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

    fn clamp_scroll(&mut self, size: PhysicalSize<u32>) {
        let wrapped = wrap_lines(&self.document.lines, max_cols(size.width));
        let max_scroll = wrapped.len().saturating_sub(self.visible_rows(size));
        self.scroll_line = self.scroll_line.min(max_scroll);
    }

    fn handle_scroll_lines(&mut self, delta: isize) {
        if delta.is_negative() {
            self.scroll_line = self.scroll_line.saturating_sub(delta.unsigned_abs());
        } else {
            self.scroll_line = self.scroll_line.saturating_add(delta as usize);
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
        self.clamp_scroll(size);
        let wrapped = wrap_lines(&self.document.lines, max_cols(size.width));
        let visible_rows = self.visible_rows(size);

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
            &self.document.title,
            ACCENT,
        );
        draw_text(
            &mut buffer,
            width,
            H_PADDING,
            24,
            &abbreviate_middle(
                &self.document.subtitle,
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
            "Esc close  Up/Down scroll  PageUp/PageDown faster  Cmd+R reload  Drop file to open  {} lines",
            wrapped.len()
        );
        draw_text(
            &mut buffer,
            width,
            H_PADDING,
            height.saturating_sub(BOTTOM_BAR_HEIGHT).saturating_add(8),
            &abbreviate_middle(&footer, max_cols(size.width).saturating_sub(2)),
            MUTED,
        );

        let start = self.scroll_line.min(wrapped.len());
        let end = (start + visible_rows).min(wrapped.len());
        for (index, line) in wrapped[start..end].iter().enumerate() {
            let y = TOP_BAR_HEIGHT + 10 + index * (CHAR_HEIGHT + LINE_SPACING);
            if y + CHAR_HEIGHT >= height.saturating_sub(BOTTOM_BAR_HEIGHT) {
                break;
            }
            draw_text(&mut buffer, width, H_PADDING, y, line, TEXT);
        }

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
            WindowEvent::CloseRequested => event_loop.exit(),
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
                    if let Some(window) = &self.window {
                        self.clamp_scroll(window.inner_size());
                    }
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
                match &event.logical_key {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Named(NamedKey::ArrowUp) => self.handle_scroll_lines(-1),
                    Key::Named(NamedKey::ArrowDown) => self.handle_scroll_lines(1),
                    Key::Named(NamedKey::PageUp) => self.handle_scroll_lines(-page),
                    Key::Named(NamedKey::PageDown) => self.handle_scroll_lines(page),
                    Key::Named(NamedKey::Home) => self.scroll_line = 0,
                    Key::Named(NamedKey::End) => self.scroll_line = usize::MAX,
                    Key::Character(text)
                        if self.modifiers.super_key() && text.eq_ignore_ascii_case("w") =>
                    {
                        event_loop.exit();
                    }
                    Key::Character(text)
                        if self.modifiers.super_key() && text.eq_ignore_ascii_case("r") =>
                    {
                        self.reload_document();
                    }
                    _ => return,
                }
                self.clamp_scroll(size);
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

fn wrap_lines(lines: &[String], max_cols: usize) -> Vec<String> {
    let max_cols = max_cols.max(1);
    let mut wrapped = Vec::new();
    for line in lines {
        let chars = line.chars().collect::<Vec<_>>();
        if chars.is_empty() {
            wrapped.push(String::new());
            continue;
        }

        for chunk in chars.chunks(max_cols) {
            wrapped.push(chunk.iter().collect());
        }
    }
    if wrapped.is_empty() {
        wrapped.push(String::new());
    }
    wrapped
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
    fn wrap_lines_chunks_long_lines() {
        let wrapped = wrap_lines(&[String::from("abcdefgh")], 3);
        assert_eq!(wrapped, vec!["abc", "def", "gh"]);
    }

    #[test]
    fn abbreviate_middle_preserves_ends() {
        let result = abbreviate_middle("abcdefghijklmnopqrstuvwxyz", 9);
        assert_eq!(result, "abcd…wxyz");
    }

    #[test]
    fn load_document_without_path_returns_welcome() {
        let document = load_document(None);
        assert_eq!(document.title, "Implicit");
        assert!(document.lines[0].contains("Implicit native window mode"));
    }

    #[test]
    fn load_document_from_path_reads_file_contents() {
        let path = std::env::temp_dir().join("implicit-window-app-load.txt");
        fs::write(&path, "hello\nworld").expect("write temp file");

        let document = load_document(Some(&path));

        assert_eq!(document.source_path.as_deref(), Some(path.as_path()));
        assert_eq!(document.title, "implicit-window-app-load.txt");
        assert_eq!(
            document.lines,
            vec![String::from("hello"), String::from("world")]
        );

        fs::remove_file(path).expect("cleanup");
    }
}
