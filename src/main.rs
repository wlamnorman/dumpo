use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

mod clipboard;
mod config;
mod dump;
mod filter;
mod format;
mod pack;
mod selector;

#[cfg(test)]
mod testutil;

#[derive(Parser)]
#[command(name = "dumpo", about = "Dump a repo into a paste-ready LLM prompt")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Args, Debug, Clone)]
pub(crate) struct PackArgs {
    #[arg(default_value = ".")]
    pub(crate) path: PathBuf,

    // Let config/defaults decide if user didn't pass it
    #[arg(long)]
    pub(crate) max_file_bytes: Option<usize>,

    #[arg(long)]
    pub(crate) max_total_bytes: Option<usize>,

    // When present, sets true. Absence means “use config/default”.
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub(crate) include_hidden: Option<bool>,

    // When present, sets false. Absence means “use config/default”.
    #[arg(long = "no-include-hidden", action = clap::ArgAction::SetFalse)]
    pub(crate) no_include_hidden: Option<bool>,

    #[arg(long, action = clap::ArgAction::Append)]
    pub(crate) include: Vec<String>,

    #[arg(long, action = clap::ArgAction::Append)]
    pub(crate) exclude: Vec<String>,

    // Optional explicit config path; if not set, search ancestors.
    #[arg(long)]
    pub(crate) config: Option<PathBuf>,

    // Disable config loading entirely.
    #[arg(long, default_value_t = false)]
    pub(crate) no_config: bool,

    #[arg(long, default_value_t = !cfg!(target_os = "macos"))]
    pub(crate) stdout: bool,

    #[arg(long, default_value_t = cfg!(target_os = "macos"))]
    pub(crate) clipboard: bool,
}

#[derive(Subcommand)]
enum Commands {
    Pack(PackArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Pack(args) => pack::run_pack(args),
    }
}
