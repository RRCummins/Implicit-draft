mod app;
mod buffer;
mod code;
mod config;
mod filetype;
mod markdown;
mod picker;
mod preview;
mod recents;
mod render;
mod session;
mod settings;
mod sidebar;
mod terminal;
mod theme;
mod welcome;

use std::ffi::OsStr;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use crate::{
    app::{App, StartupTarget},
    config::{AppConfig, RuntimeConfig},
};

#[derive(Debug, Parser)]
#[command(author, version, about = "Terminal markdown editor", long_about = None)]
struct Cli {
    /// Open the settings screen instead of a file or welcome flow.
    #[arg(long)]
    config: bool,

    /// File to open. The no-argument picker flow lands in a later phase.
    file: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let runtime = match AppConfig::load_runtime() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("implicit: {error}");
            RuntimeConfig {
                app: AppConfig::default(),
                keybindings: crate::config::KeyBindings::default(),
            }
        }
    };
    let mut terminal = terminal::init()?;
    let mut app = App::new(
        resolve_startup_target(cli.file, cli.config),
        runtime.app,
        runtime.keybindings,
    );

    let run_result = app.run(&mut terminal);
    let restore_result = terminal::restore();

    restore_result?;
    run_result?;
    Ok(())
}

fn resolve_startup_target(file: Option<PathBuf>, config: bool) -> StartupTarget {
    if config {
        return StartupTarget::Config;
    }

    match file {
        Some(path) if path.as_os_str() == OsStr::new("welcome") => StartupTarget::Welcome,
        Some(path) if path.is_dir() => StartupTarget::Browse(path),
        Some(path) => StartupTarget::Open(path),
        None => StartupTarget::Welcome,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn welcome_argument_opens_home_screen() {
        match resolve_startup_target(Some(PathBuf::from("welcome")), false) {
            StartupTarget::Welcome => {}
            StartupTarget::Config => panic!("unexpected config target"),
            StartupTarget::Open(path) => panic!("unexpected open target: {}", path.display()),
            StartupTarget::Browse(path) => {
                panic!("unexpected browse target: {}", path.display())
            }
        }
    }

    #[test]
    fn regular_argument_opens_file() {
        match resolve_startup_target(Some(PathBuf::from("notes.md")), false) {
            StartupTarget::Open(path) => assert_eq!(path, PathBuf::from("notes.md")),
            StartupTarget::Config => panic!("unexpected config target"),
            StartupTarget::Welcome => panic!("unexpected welcome target"),
            StartupTarget::Browse(path) => panic!("unexpected browse target: {}", path.display()),
        }
    }

    #[test]
    fn directory_argument_opens_browser() {
        match resolve_startup_target(Some(PathBuf::from(".")), false) {
            StartupTarget::Browse(path) => assert_eq!(path, PathBuf::from(".")),
            StartupTarget::Config => panic!("unexpected config target"),
            StartupTarget::Open(path) => panic!("unexpected open target: {}", path.display()),
            StartupTarget::Welcome => panic!("unexpected welcome target"),
        }
    }

    #[test]
    fn config_flag_uses_settings_flow() {
        match resolve_startup_target(None, true) {
            StartupTarget::Config => {}
            StartupTarget::Open(path) => panic!("unexpected open target: {}", path.display()),
            StartupTarget::Browse(path) => panic!("unexpected browse target: {}", path.display()),
            StartupTarget::Welcome => panic!("unexpected welcome target"),
        }
    }
}
