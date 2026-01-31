use anyhow::Result;
use clap::{Parser, Subcommand};
use std::path::PathBuf;

mod clipboard;
mod dump;
mod filter;
mod pack;

#[cfg(test)]
mod testutil;

#[derive(Parser)]
#[command(name = "dumpo", about = "Dump a repo into a paste-ready LLM prompt")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Pack {
        #[arg(default_value = ".")]
        path: PathBuf,

        #[arg(long, default_value_t = 20_000)]
        max_file_bytes: usize,

        #[arg(long, default_value_t = 400_000)]
        max_total_bytes: usize,

        #[arg(long, default_value_t = false)]
        include_hidden: bool,

        #[arg(long, default_value_t = false)]
        debug: bool,

        #[arg(long, default_value_t = false)]
        stdout: bool,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Pack {
            path,
            max_file_bytes,
            max_total_bytes,
            include_hidden,
            debug,
            stdout,
        } => pack::run_pack(
            &path,
            max_file_bytes,
            max_total_bytes,
            include_hidden,
            debug,
            stdout,
        ),
    }
}
