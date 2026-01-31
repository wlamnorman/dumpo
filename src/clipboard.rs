use anyhow::{Context, Result};
use std::io::Write;
use std::process::{Command, Stdio};

pub(crate) fn copy_to_clipboard(bytes: &[u8]) -> Result<()> {
    let mut child = Command::new("pbcopy")
        .stdin(Stdio::piped())
        .spawn()
        .context("failed to spawn pbcopy")?;

    {
        let mut stdin = child.stdin.take().context("failed to open pbcopy stdin")?;
        stdin
            .write_all(bytes)
            .context("failed writing to pbcopy stdin")?;
    }

    let status = child.wait().context("failed to wait for pbcopy")?;
    if !status.success() {
        anyhow::bail!("pbcopy failed");
    }

    Ok(())
}
