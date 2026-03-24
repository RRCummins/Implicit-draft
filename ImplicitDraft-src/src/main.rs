mod app;
mod buffer;
mod code;
mod config;
mod export;
mod filetype;
mod gitdiff;
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

use anyhow::{Result, bail};
use clap::Parser;

use crate::{
    app::{App, StartupTarget},
    config::{AppConfig, RuntimeConfig},
    export::{ExportFormat, PrintMode},
};

#[derive(Debug, Parser)]
#[command(author, version, about = "Terminal markdown editor", long_about = None)]
struct Cli {
    /// Open the settings screen instead of a file or welcome flow.
    #[arg(long)]
    config: bool,

    /// Render the file to stdout using ANSI styling instead of opening the TUI.
    #[arg(long)]
    print: bool,

    /// Render mode used by --print.
    #[arg(long, value_enum, default_value_t = PrintMode::Auto)]
    mode: PrintMode,

    /// Export the file to a shareable format instead of opening the TUI.
    #[arg(long, value_enum)]
    export: Option<ExportFormat>,

    /// Override the theme used by --print.
    #[arg(long)]
    theme: Option<String>,

    /// Send --print output through a pager instead of writing directly to stdout.
    #[arg(long)]
    pager: bool,

    /// Output path used by --export.
    #[arg(long)]
    output: Option<PathBuf>,

    /// File to open. The no-argument picker flow lands in a later phase.
    file: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    if cli.print && cli.export.is_some() {
        bail!("cannot combine --print with --export");
    }

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

    if cli.print {
        let path = crate::export::validate_input_path("--print", cli.file.as_deref())?;
        let theme_name = cli.theme.as_deref().unwrap_or(&runtime.app.theme);
        return crate::export::print_path(path, cli.mode, theme_name, cli.pager);
    }

    if let Some(format) = cli.export {
        let path = crate::export::validate_input_path("--export", cli.file.as_deref())?;
        let theme_name = cli.theme.as_deref().unwrap_or(&runtime.app.theme);
        crate::export::export_path(path, format, cli.mode, theme_name, cli.output.as_deref())?;
        return Ok(());
    }

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

    #[test]
    fn print_requires_a_file() {
        let error = crate::export::validate_input_path("--print", None).expect_err("missing path");
        assert!(error.to_string().contains("--print requires a file path"));
    }

    #[test]
    fn pager_flag_parses_for_print_mode() {
        let cli = Cli::parse_from(["implicit", "--print", "--pager", "notes.md"]);
        assert!(cli.print);
        assert!(cli.pager);
        assert_eq!(cli.file, Some(PathBuf::from("notes.md")));
    }

    #[test]
    fn export_flag_parses_output_path() {
        let cli = Cli::parse_from([
            "implicit",
            "--export",
            "html",
            "--output",
            "notes.html",
            "notes.md",
        ]);
        assert_eq!(cli.export, Some(ExportFormat::Html));
        assert_eq!(cli.output, Some(PathBuf::from("notes.html")));
        assert_eq!(cli.file, Some(PathBuf::from("notes.md")));
    }
}
