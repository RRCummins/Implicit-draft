//! Loads editor configuration and first-run defaults from `~/.config/implicit/`.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(default)]
pub struct AppConfig {
    pub theme: String,
    pub default_mode: DefaultMode,
    pub tab_width: usize,
    pub line_numbers: bool,
    pub wrap: bool,
    pub vim_keys: bool,
    pub auto_save: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeConfig {
    pub app: AppConfig,
    pub keybindings: KeyBindings,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KeyBindings {
    pub global: GlobalKeys,
    pub editor: EditorKeys,
    pub home: HomeKeys,
    pub picker: PickerKeys,
    pub config: ConfigKeys,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GlobalKeys {
    pub settings: Shortcut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorKeys {
    pub save: Shortcut,
    pub export: Shortcut,
    pub quit: Shortcut,
    pub home: Shortcut,
    pub find: Shortcut,
    pub find_next: Shortcut,
    pub find_prev: Shortcut,
    pub goto_line: Shortcut,
    pub cycle_mode: Shortcut,
    pub toggle_split: Shortcut,
    pub swap_split: Shortcut,
    pub toggle_sidebar: Shortcut,
    pub sidebar_narrower: Shortcut,
    pub sidebar_wider: Shortcut,
    pub undo: Shortcut,
    pub redo: Shortcut,
    pub controls: Shortcut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HomeKeys {
    pub open_picker: Shortcut,
    pub new_buffer: Shortcut,
    pub search: Shortcut,
    pub settings: Shortcut,
    pub quit: Shortcut,
    pub controls: Shortcut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PickerKeys {
    pub search: Shortcut,
    pub toggle_filter: Shortcut,
    pub back_home: Shortcut,
    pub controls: Shortcut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConfigKeys {
    pub close: Shortcut,
    pub save: Shortcut,
    pub cancel: Shortcut,
    pub controls: Shortcut,
    pub switch_pane: Shortcut,
    pub apply: Shortcut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Shortcut {
    code: ShortcutCode,
    modifiers: KeyModifiers,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum ShortcutCode {
    Char(char),
    Enter,
    Esc,
    Tab,
    Backspace,
    Delete,
    Home,
    End,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct KeyBindingsFile {
    global: GlobalKeysFile,
    editor: EditorKeysFile,
    home: HomeKeysFile,
    picker: PickerKeysFile,
    config: ConfigKeysFile,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct GlobalKeysFile {
    settings: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct EditorKeysFile {
    save: Option<String>,
    export: Option<String>,
    quit: Option<String>,
    home: Option<String>,
    find: Option<String>,
    find_next: Option<String>,
    find_prev: Option<String>,
    goto_line: Option<String>,
    cycle_mode: Option<String>,
    toggle_split: Option<String>,
    swap_split: Option<String>,
    toggle_sidebar: Option<String>,
    sidebar_narrower: Option<String>,
    sidebar_wider: Option<String>,
    undo: Option<String>,
    redo: Option<String>,
    controls: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct HomeKeysFile {
    open_picker: Option<String>,
    new_buffer: Option<String>,
    search: Option<String>,
    settings: Option<String>,
    quit: Option<String>,
    controls: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct PickerKeysFile {
    search: Option<String>,
    toggle_filter: Option<String>,
    back_home: Option<String>,
    controls: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
struct ConfigKeysFile {
    close: Option<String>,
    save: Option<String>,
    cancel: Option<String>,
    controls: Option<String>,
    switch_pane: Option<String>,
    apply: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DefaultMode {
    #[default]
    SourceHints,
    Preview,
    Source,
}

impl DefaultMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::SourceHints => "source_hints",
            Self::Preview => "preview",
            Self::Source => "source",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::SourceHints => Self::Preview,
            Self::Preview => Self::Source,
            Self::Source => Self::SourceHints,
        }
    }

    pub fn previous(self) -> Self {
        match self {
            Self::SourceHints => Self::Source,
            Self::Preview => Self::SourceHints,
            Self::Source => Self::Preview,
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: String::from("dark"),
            default_mode: DefaultMode::SourceHints,
            tab_width: 4,
            line_numbers: false,
            wrap: true,
            vim_keys: false,
            auto_save: false,
        }
    }
}

impl AppConfig {
    pub fn load_runtime() -> Result<RuntimeConfig> {
        let dir = config_dir();
        ensure_default_files(&dir)?;
        Ok(RuntimeConfig {
            app: Self::load_from_dir(&dir)?,
            keybindings: KeyBindings::load_from_dir(&dir)?,
        })
    }

    pub fn save(&self) -> Result<()> {
        let dir = config_dir();
        ensure_default_files(&dir)?;
        let path = config_path(&dir);
        fs::write(&path, self.to_toml())
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }

    fn load_from_dir(dir: &Path) -> Result<Self> {
        ensure_default_files(dir)?;
        let path = config_path(dir);
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let parsed =
            toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(parsed)
    }

    pub fn tab_width(&self) -> usize {
        self.tab_width.max(1)
    }

    fn to_toml(&self) -> String {
        let defaults = Self::default();
        let mut lines = Vec::new();

        if self.theme != defaults.theme {
            lines.push(format!("theme = {:?}", self.theme));
        }
        if self.default_mode != defaults.default_mode {
            lines.push(format!("default_mode = {:?}", self.default_mode.label()));
        }
        if self.tab_width() != defaults.tab_width {
            lines.push(format!("tab_width = {}", self.tab_width()));
        }
        if self.line_numbers != defaults.line_numbers {
            lines.push(format!("line_numbers = {}", self.line_numbers));
        }
        if self.wrap != defaults.wrap {
            lines.push(format!("wrap = {}", self.wrap));
        }
        if self.vim_keys != defaults.vim_keys {
            lines.push(format!("vim_keys = {}", self.vim_keys));
        }
        if self.auto_save != defaults.auto_save {
            lines.push(format!("auto_save = {}", self.auto_save));
        }

        if lines.is_empty() {
            String::from("# implicit uses built-in defaults\n")
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self {
            global: GlobalKeys {
                settings: Shortcut::parse("ctrl+,").expect("default shortcut"),
            },
            editor: EditorKeys {
                save: Shortcut::parse("ctrl+s").expect("default shortcut"),
                export: Shortcut::parse("ctrl+shift+e").expect("default shortcut"),
                quit: Shortcut::parse("ctrl+q").expect("default shortcut"),
                home: Shortcut::parse("ctrl+w").expect("default shortcut"),
                find: Shortcut::parse("ctrl+f").expect("default shortcut"),
                find_next: Shortcut::parse("alt+n").expect("default shortcut"),
                find_prev: Shortcut::parse("alt+p").expect("default shortcut"),
                goto_line: Shortcut::parse("ctrl+g").expect("default shortcut"),
                cycle_mode: Shortcut::parse("ctrl+p").expect("default shortcut"),
                toggle_split: Shortcut::parse("ctrl+\\").expect("default shortcut"),
                swap_split: Shortcut::parse("ctrl+.").expect("default shortcut"),
                toggle_sidebar: Shortcut::parse("ctrl+e").expect("default shortcut"),
                sidebar_narrower: Shortcut::parse("ctrl+[").expect("default shortcut"),
                sidebar_wider: Shortcut::parse("ctrl+]").expect("default shortcut"),
                undo: Shortcut::parse("ctrl+z").expect("default shortcut"),
                redo: Shortcut::parse("ctrl+r").expect("default shortcut"),
                controls: Shortcut::parse("?").expect("default shortcut"),
            },
            home: HomeKeys {
                open_picker: Shortcut::parse("o").expect("default shortcut"),
                new_buffer: Shortcut::parse("n").expect("default shortcut"),
                search: Shortcut::parse("/").expect("default shortcut"),
                settings: Shortcut::parse("c").expect("default shortcut"),
                quit: Shortcut::parse("q").expect("default shortcut"),
                controls: Shortcut::parse("?").expect("default shortcut"),
            },
            picker: PickerKeys {
                search: Shortcut::parse("/").expect("default shortcut"),
                toggle_filter: Shortcut::parse("a").expect("default shortcut"),
                back_home: Shortcut::parse("esc").expect("default shortcut"),
                controls: Shortcut::parse("?").expect("default shortcut"),
            },
            config: ConfigKeys {
                close: Shortcut::parse("ctrl+,").expect("default shortcut"),
                save: Shortcut::parse("s").expect("default shortcut"),
                cancel: Shortcut::parse("esc").expect("default shortcut"),
                controls: Shortcut::parse("?").expect("default shortcut"),
                switch_pane: Shortcut::parse("tab").expect("default shortcut"),
                apply: Shortcut::parse("enter").expect("default shortcut"),
            },
        }
    }
}

impl KeyBindings {
    fn load_from_dir(dir: &Path) -> Result<Self> {
        let path = dir.join("keybindings.toml");
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let parsed: KeyBindingsFile =
            toml::from_str(&text).with_context(|| format!("failed to parse {}", path.display()))?;
        Self::merge(parsed)
    }

    fn merge(file: KeyBindingsFile) -> Result<Self> {
        let defaults = Self::default();

        Ok(Self {
            global: GlobalKeys {
                settings: parse_or_default(file.global.settings, defaults.global.settings)?,
            },
            editor: EditorKeys {
                save: parse_or_default(file.editor.save, defaults.editor.save)?,
                export: parse_or_default(file.editor.export, defaults.editor.export)?,
                quit: parse_or_default(file.editor.quit, defaults.editor.quit)?,
                home: parse_or_default(file.editor.home, defaults.editor.home)?,
                find: parse_or_default(file.editor.find, defaults.editor.find)?,
                find_next: parse_or_default(file.editor.find_next, defaults.editor.find_next)?,
                find_prev: parse_or_default(file.editor.find_prev, defaults.editor.find_prev)?,
                goto_line: parse_or_default(file.editor.goto_line, defaults.editor.goto_line)?,
                cycle_mode: parse_or_default(file.editor.cycle_mode, defaults.editor.cycle_mode)?,
                toggle_split: parse_or_default(
                    file.editor.toggle_split,
                    defaults.editor.toggle_split,
                )?,
                swap_split: parse_or_default(file.editor.swap_split, defaults.editor.swap_split)?,
                toggle_sidebar: parse_or_default(
                    file.editor.toggle_sidebar,
                    defaults.editor.toggle_sidebar,
                )?,
                sidebar_narrower: parse_or_default(
                    file.editor.sidebar_narrower,
                    defaults.editor.sidebar_narrower,
                )?,
                sidebar_wider: parse_or_default(
                    file.editor.sidebar_wider,
                    defaults.editor.sidebar_wider,
                )?,
                undo: parse_or_default(file.editor.undo, defaults.editor.undo)?,
                redo: parse_or_default(file.editor.redo, defaults.editor.redo)?,
                controls: parse_or_default(file.editor.controls, defaults.editor.controls)?,
            },
            home: HomeKeys {
                open_picker: parse_or_default(file.home.open_picker, defaults.home.open_picker)?,
                new_buffer: parse_or_default(file.home.new_buffer, defaults.home.new_buffer)?,
                search: parse_or_default(file.home.search, defaults.home.search)?,
                settings: parse_or_default(file.home.settings, defaults.home.settings)?,
                quit: parse_or_default(file.home.quit, defaults.home.quit)?,
                controls: parse_or_default(file.home.controls, defaults.home.controls)?,
            },
            picker: PickerKeys {
                search: parse_or_default(file.picker.search, defaults.picker.search)?,
                toggle_filter: parse_or_default(
                    file.picker.toggle_filter,
                    defaults.picker.toggle_filter,
                )?,
                back_home: parse_or_default(file.picker.back_home, defaults.picker.back_home)?,
                controls: parse_or_default(file.picker.controls, defaults.picker.controls)?,
            },
            config: ConfigKeys {
                close: parse_or_default(file.config.close, defaults.config.close)?,
                save: parse_or_default(file.config.save, defaults.config.save)?,
                cancel: parse_or_default(file.config.cancel, defaults.config.cancel)?,
                controls: parse_or_default(file.config.controls, defaults.config.controls)?,
                switch_pane: parse_or_default(
                    file.config.switch_pane,
                    defaults.config.switch_pane,
                )?,
                apply: parse_or_default(file.config.apply, defaults.config.apply)?,
            },
        })
    }
}

impl Shortcut {
    pub fn parse(value: &str) -> Result<Self> {
        let mut modifiers = KeyModifiers::NONE;
        let mut key = None;

        for part in value.split('+') {
            let token = part.trim().to_lowercase();
            match token.as_str() {
                "ctrl" | "control" => modifiers.insert(KeyModifiers::CONTROL),
                "alt" => modifiers.insert(KeyModifiers::ALT),
                "shift" => modifiers.insert(KeyModifiers::SHIFT),
                "enter" => key = Some(ShortcutCode::Enter),
                "esc" | "escape" => key = Some(ShortcutCode::Esc),
                "tab" => key = Some(ShortcutCode::Tab),
                "backspace" => key = Some(ShortcutCode::Backspace),
                "delete" | "del" => key = Some(ShortcutCode::Delete),
                "home" => key = Some(ShortcutCode::Home),
                "end" => key = Some(ShortcutCode::End),
                "left" => key = Some(ShortcutCode::Left),
                "right" => key = Some(ShortcutCode::Right),
                "up" => key = Some(ShortcutCode::Up),
                "down" => key = Some(ShortcutCode::Down),
                "pageup" | "page_up" => key = Some(ShortcutCode::PageUp),
                "pagedown" | "page_down" => key = Some(ShortcutCode::PageDown),
                _ => {
                    let mut chars = token.chars();
                    let Some(ch) = chars.next() else {
                        continue;
                    };
                    if chars.next().is_some() {
                        anyhow::bail!("unsupported shortcut token: {token}");
                    }
                    key = Some(ShortcutCode::Char(ch));
                }
            }
        }

        let Some(code) = key else {
            anyhow::bail!("shortcut is missing a key: {value}");
        };

        Ok(Self { code, modifiers })
    }

    pub fn matches(&self, key: KeyEvent) -> bool {
        match (&self.code, key.code) {
            (ShortcutCode::Char(expected), KeyCode::Char(actual)) => {
                let modifiers = key.modifiers;
                if modifiers == self.modifiers && expected.eq_ignore_ascii_case(&actual) {
                    return true;
                }

                !self.modifiers.contains(KeyModifiers::SHIFT)
                    && modifiers == (self.modifiers | KeyModifiers::SHIFT)
                    && !expected.is_ascii_alphanumeric()
                    && *expected == actual
            }
            (ShortcutCode::Enter, KeyCode::Enter)
            | (ShortcutCode::Esc, KeyCode::Esc)
            | (ShortcutCode::Tab, KeyCode::Tab)
            | (ShortcutCode::Backspace, KeyCode::Backspace)
            | (ShortcutCode::Delete, KeyCode::Delete)
            | (ShortcutCode::Home, KeyCode::Home)
            | (ShortcutCode::End, KeyCode::End)
            | (ShortcutCode::Left, KeyCode::Left)
            | (ShortcutCode::Right, KeyCode::Right)
            | (ShortcutCode::Up, KeyCode::Up)
            | (ShortcutCode::Down, KeyCode::Down)
            | (ShortcutCode::PageUp, KeyCode::PageUp)
            | (ShortcutCode::PageDown, KeyCode::PageDown) => key.modifiers == self.modifiers,
            _ => false,
        }
    }
}

fn parse_or_default(value: Option<String>, default: Shortcut) -> Result<Shortcut> {
    match value {
        Some(value) => Shortcut::parse(&value),
        None => Ok(default),
    }
}

fn ensure_default_files(dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("failed to create {}", dir.display()))?;

    let themes_dir = dir.join("themes");
    fs::create_dir_all(&themes_dir)
        .with_context(|| format!("failed to create {}", themes_dir.display()))?;

    let config_path = config_path(dir);
    if !config_path.exists() {
        fs::write(&config_path, default_config_toml())
            .with_context(|| format!("failed to write {}", config_path.display()))?;
    }

    let keybindings_path = dir.join("keybindings.toml");
    if !keybindings_path.exists() {
        fs::write(&keybindings_path, default_keybindings_toml())
            .with_context(|| format!("failed to write {}", keybindings_path.display()))?;
    }

    Ok(())
}

fn config_path(dir: &Path) -> PathBuf {
    let mut path = dir.to_path_buf();
    path.push("config.toml");
    path
}

fn config_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("IMPLICIT_CONFIG_DIR") {
        return PathBuf::from(path);
    }

    match std::env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(".config/implicit"),
        None => PathBuf::from(".implicit"),
    }
}

fn default_config_toml() -> &'static str {
    r#"theme = "dark"
default_mode = "source_hints"
tab_width = 4
line_numbers = false
wrap = true
vim_keys = false
auto_save = false
"#
}

fn default_keybindings_toml() -> &'static str {
    r#"[global]
settings = "ctrl+,"

[editor]
save = "ctrl+s"
export = "ctrl+shift+e"
quit = "ctrl+q"
home = "ctrl+w"
find = "ctrl+f"
find_next = "alt+n"
find_prev = "alt+p"
goto_line = "ctrl+g"
cycle_mode = "ctrl+p"
toggle_split = "ctrl+\\"
swap_split = "ctrl+."
toggle_sidebar = "ctrl+e"
sidebar_narrower = "ctrl+["
sidebar_wider = "ctrl+]"
undo = "ctrl+z"
redo = "ctrl+r"
controls = "?"

[home]
open_picker = "o"
new_buffer = "n"
search = "/"
settings = "c"
quit = "q"
controls = "?"

[picker]
search = "/"
toggle_filter = "a"
back_home = "esc"
controls = "?"

[config]
close = "ctrl+,"
save = "s"
cancel = "esc"
controls = "?"
switch_pane = "tab"
apply = "enter"
"#
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_config_dir(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        std::env::temp_dir().join(format!("implicit-config-{name}-{unique}"))
    }

    #[test]
    fn load_creates_default_files_on_first_run() {
        let dir = temp_config_dir("create-defaults");

        let config = AppConfig::load_from_dir(&dir).expect("load config");

        assert_eq!(config.theme, "dark");
        assert_eq!(config.default_mode, DefaultMode::SourceHints);
        assert_eq!(config.tab_width, 4);
        assert!(dir.join("config.toml").exists());
        assert!(dir.join("keybindings.toml").exists());
        assert!(dir.join("themes").is_dir());
        fs::remove_dir_all(dir).expect("cleanup");
    }

    #[test]
    fn parses_partial_config() {
        let parsed: AppConfig = toml::from_str(
            r#"
default_mode = "source"
tab_width = 8
line_numbers = true
"#,
        )
        .expect("config");

        assert_eq!(parsed.theme, "dark");
        assert_eq!(parsed.default_mode, DefaultMode::Source);
        assert_eq!(parsed.tab_width, 8);
        assert!(parsed.line_numbers);
        assert!(parsed.wrap);
        assert!(!parsed.auto_save);
    }

    #[test]
    fn zero_tab_width_falls_back_to_one() {
        let parsed: AppConfig = toml::from_str(r#"tab_width = 0"#).expect("config");

        assert_eq!(parsed.tab_width(), 1);
    }

    #[test]
    fn save_omits_default_values() {
        let config = AppConfig::default();

        assert_eq!(config.to_toml(), "# implicit uses built-in defaults\n");
    }

    #[test]
    fn save_only_writes_changed_values() {
        let config = AppConfig {
            theme: String::from("gruvbox"),
            line_numbers: true,
            ..AppConfig::default()
        };

        assert_eq!(
            config.to_toml(),
            "theme = \"gruvbox\"\nline_numbers = true\n"
        );
    }

    #[test]
    fn parses_ctrl_shortcut_strings() {
        let shortcut = Shortcut::parse("ctrl+s").expect("shortcut");

        assert!(shortcut.matches(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)));
        assert!(!shortcut.matches(KeyEvent::new(KeyCode::Char('s'), KeyModifiers::NONE)));
    }

    #[test]
    fn question_mark_shortcut_matches_shifted_char() {
        let shortcut = Shortcut::parse("?").expect("shortcut");

        assert!(shortcut.matches(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::SHIFT)));
    }

    #[test]
    fn ctrl_shift_shortcut_matches_shifted_letter() {
        let shortcut = Shortcut::parse("ctrl+shift+e").expect("shortcut");

        assert!(shortcut.matches(KeyEvent::new(
            KeyCode::Char('E'),
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
        )));
        assert!(!shortcut.matches(KeyEvent::new(KeyCode::Char('e'), KeyModifiers::CONTROL,)));
    }

    #[test]
    fn plain_letter_shortcut_does_not_match_shifted_letter() {
        let shortcut = Shortcut::parse("n").expect("shortcut");

        assert!(!shortcut.matches(KeyEvent::new(KeyCode::Char('N'), KeyModifiers::SHIFT,)));
    }

    #[test]
    fn keybindings_merge_overrides_defaults() {
        let parsed: KeyBindingsFile = toml::from_str(
            r#"
[global]
settings = "ctrl+g"

[editor]
save = "ctrl+x"
"#,
        )
        .expect("keybindings");
        let merged = KeyBindings::merge(parsed).expect("merge");

        assert!(
            merged
                .global
                .settings
                .matches(KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL,))
        );
        assert!(
            merged
                .editor
                .save
                .matches(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::CONTROL,))
        );
        assert!(
            merged
                .editor
                .quit
                .matches(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::CONTROL,))
        );
    }
}
