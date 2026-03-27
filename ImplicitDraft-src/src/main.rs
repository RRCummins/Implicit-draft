mod app;
mod buffer;
mod code;
mod config;
mod export;
mod filetype;
mod gitdiff;
mod install;
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
mod updater;
mod welcome;
mod window_app;

use std::ffi::OsStr;
use std::path::PathBuf;

use anyhow::{Result, bail};
use clap::Parser;

use crate::{
    app::{App, StartupTarget},
    config::{AppConfig, RuntimeConfig},
    export::{ExportFormat, LineRange, PrintMode, parse_line_range},
};

#[derive(Debug, Parser)]
#[command(author, version, about = "Terminal markdown editor", long_about = None)]
struct Cli {
    /// Open the settings screen instead of a file or welcome flow.
    #[arg(long)]
    config: bool,

    /// Install the current binary into ~/.local/bin/implicit.
    #[arg(long)]
    install: bool,

    /// Download and install the latest published release into ~/.local/bin/implicit.
    #[arg(long)]
    update: bool,

    /// Install Implicit.app into ~/Applications.
    #[arg(long)]
    install_app: bool,

    /// Open the file in the native Implicit app window instead of the terminal UI.
    #[arg(long)]
    app: bool,

    /// Launch a separate native Implicit app window process for the file.
    #[arg(long)]
    new_window: bool,

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

    /// Export a code-oriented image snapshot instead of opening the TUI.
    #[arg(long)]
    snapshot: bool,

    /// Output path used by --export.
    #[arg(long)]
    output: Option<PathBuf>,

    /// Restrict --print or --export to a 1-based line range like 10-25.
    #[arg(long, value_parser = parse_line_range)]
    lines: Option<LineRange>,

    /// File to open. The no-argument picker flow lands in a later phase.
    file: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse_from(filtered_cli_args());
    let action_count = usize::from(cli.print)
        + usize::from(cli.export.is_some())
        + usize::from(cli.snapshot)
        + usize::from(cli.install)
        + usize::from(cli.update)
        + usize::from(cli.install_app);
    if action_count > 1 {
        bail!(
            "cannot combine --print, --export, --snapshot, --install, --install-app, and --update"
        );
    }
    if cli.app && cli.new_window {
        bail!("cannot combine --app and --new-window");
    }
    if (cli.app || cli.new_window) && action_count > 0 {
        bail!(
            "cannot combine --app or --new-window with --print, --export, --snapshot, --install, --install-app, or --update"
        );
    }

    let runtime = match AppConfig::load_runtime() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("implicit: {error}");
            RuntimeConfig {
                app: AppConfig::default(),
                keybindings: crate::config::KeyBindings::default(),
                first_run: true,
            }
        }
    };

    if cli.install {
        let result = crate::install::install_current_exe()?;
        if result.already_current {
            println!(
                "implicit is already installed at {}",
                result.target.display()
            );
        } else {
            println!("installed implicit to {}", result.target.display());
        }
        if !result.on_path {
            println!("add this to your shell profile:");
            println!("{}", crate::install::path_export_hint());
        }
        return Ok(());
    }

    if cli.update {
        match crate::updater::self_update(env!("CARGO_PKG_VERSION"))? {
            Some(result) => {
                println!(
                    "updated implicit from {} to {} at {}",
                    result.previous_version,
                    result.installed_version,
                    result.target.display()
                );
                if !crate::install::current_status().on_path {
                    println!("add this to your shell profile:");
                    println!("{}", crate::install::path_export_hint());
                }
            }
            None => {
                if crate::install::current_status().installed {
                    println!("implicit {} is already current", env!("CARGO_PKG_VERSION"));
                } else {
                    let result = crate::install::install_current_exe()?;
                    if result.already_current {
                        println!(
                            "implicit is already installed at {}",
                            result.target.display()
                        );
                    } else {
                        println!(
                            "installed current implicit {} to {}",
                            env!("CARGO_PKG_VERSION"),
                            result.target.display()
                        );
                    }
                    if !result.on_path {
                        println!("add this to your shell profile:");
                        println!("{}", crate::install::path_export_hint());
                    }
                }
            }
        }
        return Ok(());
    }

    if cli.install_app {
        let result = crate::install::install_current_app_bundle()?;
        if result.already_current {
            println!(
                "Implicit.app is already installed at {}",
                result.target.display()
            );
        } else {
            println!("installed Implicit.app to {}", result.target.display());
        }
        println!("launch it with:");
        println!("open -na {}", result.target.display());
        return Ok(());
    }

    if cli.print {
        let path = crate::export::validate_input_path("--print", cli.file.as_deref())?;
        let theme_name = cli.theme.as_deref().unwrap_or(&runtime.app.theme);
        return crate::export::print_path(path, cli.mode, theme_name, cli.pager, cli.lines);
    }

    if let Some(format) = cli.export {
        let path = crate::export::validate_input_path("--export", cli.file.as_deref())?;
        let theme_name = cli.theme.as_deref().unwrap_or(&runtime.app.theme);
        let exported = crate::export::export_path(
            path,
            format,
            cli.mode,
            theme_name,
            cli.output.as_deref(),
            cli.lines,
        )?;
        if let Some(notice) = exported.fallback_notice() {
            eprintln!("implicit: {notice}");
        }
        return Ok(());
    }

    if cli.snapshot {
        let path = crate::export::validate_input_path("--snapshot", cli.file.as_deref())?;
        let theme_name = cli.theme.as_deref().unwrap_or(&runtime.app.theme);
        crate::export::snapshot_path(
            path,
            cli.mode,
            theme_name,
            cli.output.as_deref(),
            cli.lines,
            runtime.app.line_numbers,
        )?;
        return Ok(());
    }

    if cli.new_window {
        return crate::window_app::launch_new_window(cli.file.as_deref());
    }

    if cli.app {
        return crate::window_app::run(cli.file.as_deref());
    }

    let mut terminal = terminal::init()?;
    let mut app = App::new_with_runtime(
        resolve_startup_target(cli.file, cli.config),
        runtime.app,
        runtime.keybindings,
        runtime.first_run,
    );

    let run_result = app.run(&mut terminal);
    let restore_result = terminal::restore();

    restore_result?;
    run_result?;
    Ok(())
}

fn filtered_cli_args() -> Vec<std::ffi::OsString> {
    std::env::args_os()
        .filter(|arg| !is_ignored_platform_arg(arg))
        .collect()
}

fn is_ignored_platform_arg(arg: &std::ffi::OsString) -> bool {
    #[cfg(target_os = "macos")]
    {
        arg.to_str()
            .map(|value| value.starts_with("-psn_"))
            .unwrap_or(false)
    }

    #[cfg(not(target_os = "macos"))]
    {
        let _ = arg;
        false
    }
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
    fn install_flag_parses_without_file() {
        let cli = Cli::parse_from(["implicit", "--install"]);
        assert!(cli.install);
        assert!(!cli.update);
        assert_eq!(cli.file, None);
    }

    #[test]
    fn update_flag_parses_without_file() {
        let cli = Cli::parse_from(["implicit", "--update"]);
        assert!(cli.update);
        assert!(!cli.install);
        assert_eq!(cli.file, None);
    }

    #[test]
    fn install_app_flag_parses_without_file() {
        let cli = Cli::parse_from(["implicit", "--install-app"]);
        assert!(cli.install_app);
        assert!(!cli.install);
        assert_eq!(cli.file, None);
    }

    #[test]
    fn app_flag_parses_without_file() {
        let cli = Cli::parse_from(["implicit", "--app"]);
        assert!(cli.app);
        assert!(!cli.new_window);
        assert_eq!(cli.file, None);
    }

    #[test]
    fn new_window_flag_parses_with_file() {
        let cli = Cli::parse_from(["implicit", "--new-window", "notes.md"]);
        assert!(cli.new_window);
        assert!(!cli.app);
        assert_eq!(cli.file, Some(PathBuf::from("notes.md")));
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
            "--lines",
            "10-25",
            "--output",
            "notes.html",
            "notes.md",
        ]);
        assert_eq!(cli.export, Some(ExportFormat::Html));
        assert_eq!(cli.output, Some(PathBuf::from("notes.html")));
        assert_eq!(cli.lines, Some(LineRange { start: 10, end: 25 }));
        assert_eq!(cli.file, Some(PathBuf::from("notes.md")));
    }

    #[test]
    fn snapshot_flag_parses_output_path() {
        let cli = Cli::parse_from([
            "implicit",
            "--snapshot",
            "--lines",
            "12-18",
            "--output",
            "snippet.svg",
            "main.rs",
        ]);
        assert!(cli.snapshot);
        assert_eq!(cli.output, Some(PathBuf::from("snippet.svg")));
        assert_eq!(cli.lines, Some(LineRange { start: 12, end: 18 }));
        assert_eq!(cli.file, Some(PathBuf::from("main.rs")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn filters_finder_process_serial_number_arg() {
        let args = vec![
            std::ffi::OsString::from("implicit"),
            std::ffi::OsString::from("--app"),
            std::ffi::OsString::from("-psn_0_12345"),
            std::ffi::OsString::from("notes.md"),
        ];
        let filtered = args
            .into_iter()
            .filter(|arg| !is_ignored_platform_arg(arg))
            .collect::<Vec<_>>();
        let cli = Cli::parse_from(filtered);
        assert!(cli.app);
        assert_eq!(cli.file, Some(PathBuf::from("notes.md")));
    }
}
