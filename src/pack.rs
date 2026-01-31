use anyhow::{Context, Result};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

use crate::clipboard::copy_to_clipboard;
use crate::dump::build_dump_bytes;

pub(crate) fn run_pack(
    root: &Path,
    max_file_bytes: usize,
    max_total_bytes: usize,
    include_hidden: bool,
    debug: bool,
    stdout: bool,
) -> Result<()> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize path: {}", root.display()))?;

    let bytes = build_dump_bytes(&root, max_file_bytes, max_total_bytes, include_hidden)?;

    copy_to_clipboard(&bytes)?;

    if debug {
        let debug_path = root.join(".dumpo.debug.md");
        fs::write(&debug_path, &bytes)
            .with_context(|| format!("failed to write {}", debug_path.display()))?;
    }

    if stdout {
        let mut out = io::stdout().lock();
        out.write_all(&bytes).context("failed writing to stdout")?;
    }

    Ok(())
}
