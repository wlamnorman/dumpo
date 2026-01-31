use anyhow::{Context, Result};
use std::io::{self, Write};

use crate::clipboard::copy_to_clipboard;
use crate::dump::build_dump_bytes;
use crate::PackArgs;

pub(crate) fn run_pack(args: PackArgs) -> Result<()> {
    let root = args
        .path
        .canonicalize()
        .with_context(|| format!("failed to canonicalize path: {}", args.path.display()))?;

    let bytes = build_dump_bytes(
        &root,
        args.max_file_bytes,
        args.max_total_bytes,
        args.include_hidden,
    )?;

    copy_to_clipboard(&bytes)?;

    if args.stdout {
        let mut out = io::stdout().lock();
        out.write_all(&bytes).context("failed writing to stdout")?;
    }

    Ok(())
}
