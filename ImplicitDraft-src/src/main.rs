mod app;
mod render;
mod terminal;

use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;

use crate::app::App;

#[derive(Debug, Parser)]
#[command(author, version, about = "Terminal markdown editor", long_about = None)]
struct Cli {
    /// File to open. The no-argument picker flow lands in a later phase.
    file: Option<PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut terminal = terminal::init()?;
    let mut app = App::new(cli.file);

    let run_result = app.run(&mut terminal);
    let restore_result = terminal::restore();

    restore_result?;
    run_result?;
    Ok(())
}
