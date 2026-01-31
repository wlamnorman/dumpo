use anyhow::Result;
use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

mod clipboard;
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

    #[arg(long, default_value_t = 20_000)]
    pub(crate) max_file_bytes: usize,

    #[arg(long, default_value_t = 400_000)]
    pub(crate) max_total_bytes: usize,

    #[arg(long, default_value_t = false)]
    pub(crate) include_hidden: bool,

    // Repeatable glob patterns matched against repo-relative paths.
    // Paths are matched with '/' separators (Windows paths are normalized).
    // Example: --include 'src/**' --include 'Cargo.toml'
    #[arg(long, action = clap::ArgAction::Append)]
    pub(crate) include: Vec<String>,

    // Repeatable glob patterns matched against repo-relative paths.
    // Paths are matched with '/' separators (Windows paths are normalized).
    // Example: --exclude 'target/**' --exclude '**/*.min.js'
    #[arg(long, action = clap::ArgAction::Append)]
    pub(crate) exclude: Vec<String>,

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
