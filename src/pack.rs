use crate::clipboard::copy_to_clipboard;
use crate::config::DumpoConfig;
use crate::dump::build_dump_bytes;
use crate::selector::Selector;
use crate::PackArgs;
use anyhow::{Context, Result};
use std::io::{self, Write};

const DEFAULT_MAX_FILE_BYTES: usize = 20_000;
const DEFAULT_MAX_TOTAL_BYTES: usize = 400_000;

pub(crate) fn run_pack(args: PackArgs) -> Result<()> {
    let root = args
        .path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize path: {}", args.path.display()))?;

    // Load config (optional)
    let cfg = if args.no_config {
        DumpoConfig::default()
    } else if let Some(path) = &args.config {
        let s = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        toml::from_str::<DumpoConfig>(&s)
            .with_context(|| format!("failed to parse config: {}", path.display()))?
    } else {
        let (_path, cfg) = DumpoConfig::load_nearest(&root)?;
        cfg
    };

    // Resolve effective settings
    let max_file_bytes = args
        .max_file_bytes
        .or(cfg.max_file_bytes)
        .unwrap_or(DEFAULT_MAX_FILE_BYTES);

    let max_total_bytes = args
        .max_total_bytes
        .or(cfg.max_total_bytes)
        .unwrap_or(DEFAULT_MAX_TOTAL_BYTES);

    let include_hidden = args.include_hidden.or(cfg.include_hidden).unwrap_or(false);

    let include = if !args.include.is_empty() {
        args.include
    } else {
        cfg.include.unwrap_or_default()
    };

    let exclude = if !args.exclude.is_empty() {
        args.exclude
    } else {
        cfg.exclude.unwrap_or_default()
    };

    let selector = Selector::new(&include, &exclude)?;

    let bytes = build_dump_bytes(
        &root,
        max_file_bytes,
        max_total_bytes,
        include_hidden,
        &selector,
    )?;

    if !args.clipboard && !args.stdout {
        anyhow::bail!("no output selected (use --stdout and/or --clipboard)");
    }

    if args.clipboard {
        copy_to_clipboard(&bytes)?;
    }

    if args.stdout {
        let mut out = io::stdout().lock();
        out.write_all(&bytes).context("failed writing to stdout")?;
    }

    Ok(())
}
