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

    let (cfg_path, cfg) = load_config_for_pack(&root, &args)?;

    // Resolve effective settings
    let max_file_bytes = args
        .max_file_bytes
        .or(cfg.max_file_bytes)
        .unwrap_or(DEFAULT_MAX_FILE_BYTES);

    let max_total_bytes = args
        .max_total_bytes
        .or(cfg.max_total_bytes)
        .unwrap_or(DEFAULT_MAX_TOTAL_BYTES);

    let include_hidden = args
        .include_hidden
        .or(args.no_include_hidden)
        .or(cfg.include_hidden)
        .unwrap_or(false);

    let (include_from_cli, include) = if !args.include.is_empty() {
        (true, args.include)
    } else {
        (false, cfg.include.unwrap_or_default())
    };

    let (exclude_from_cli, exclude) = if !args.exclude.is_empty() {
        (true, args.exclude)
    } else {
        (false, cfg.exclude.unwrap_or_default())
    };

    if args.verbose {
        let cfg_display = cfg_path
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|| "<none>".to_string());

        eprintln!(
            "dumpo: root={} config={} max_file_bytes={} max_total_bytes={} include_hidden={} {} {} stdout={} clipboard={}",
            root.display(),
            cfg_display,
            max_file_bytes,
            max_total_bytes,
            include_hidden,
            summarize_patterns("include", include_from_cli, &include),
            summarize_patterns("exclude", exclude_from_cli, &exclude),
            args.stdout,
            args.clipboard,
        );
    }

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

use std::path::{Path, PathBuf};

fn load_config_for_pack(root: &Path, args: &PackArgs) -> Result<(Option<PathBuf>, DumpoConfig)> {
    if args.no_config {
        return Ok((None, DumpoConfig::default()));
    }

    if let Some(path) = &args.config {
        let s = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;
        let cfg = toml::from_str::<DumpoConfig>(&s)
            .with_context(|| format!("failed to parse config: {}", path.display()))?;
        return Ok((Some(path.clone()), cfg));
    }

    DumpoConfig::load_nearest(root)
}

fn summarize_patterns(label: &str, from_cli: bool, patterns: &[String]) -> String {
    if patterns.is_empty() {
        return format!("{label}=<none>");
    }

    let n = patterns.len().min(20);
    let head = patterns[..n].join(", ");
    let src = if from_cli { "cli" } else { "config" };

    if patterns.len() > n {
        format!("{label}({src})=[{head}, ...] (n={})", patterns.len())
    } else {
        format!("{label}({src})=[{head}] (n={})", patterns.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    fn base_args(repo: &TempRepo) -> PackArgs {
        PackArgs {
            path: repo.path().to_path_buf(),
            max_file_bytes: None,
            max_total_bytes: None,
            include_hidden: None,
            no_include_hidden: None,
            verbose: false,
            include: vec![],
            exclude: vec![],
            config: None,
            no_config: false,
            stdout: true,
            clipboard: false,
        }
    }

    #[test]
    fn no_config_ignores_repo_dumpo_toml() {
        let repo = TempRepo::new();
        repo.write("dumpo.toml", "max_total_bytes = 111\n");

        let args = PackArgs {
            no_config: true,
            ..base_args(&repo)
        };

        let root = args.path.canonicalize().unwrap();
        let (_path, cfg) = load_config_for_pack(&root, &args).unwrap();

        // default config struct when --no-config is set
        assert!(cfg.max_total_bytes.is_none());
        assert!(cfg.max_file_bytes.is_none());
        assert!(cfg.include_hidden.is_none());
        assert!(cfg.include.is_none());
        assert!(cfg.exclude.is_none());
    }

    #[test]
    fn explicit_config_overrides_nearest_search() {
        let repo = TempRepo::new();
        repo.write("dumpo.toml", "max_total_bytes = 111\n");
        repo.write("custom.toml", "max_total_bytes = 222\n");

        let mut args = base_args(&repo);
        args.config = Some(repo.path().join("custom.toml"));

        let root = args.path.canonicalize().unwrap();
        let (path, cfg) = load_config_for_pack(&root, &args).unwrap();

        assert!(path.unwrap().ends_with("custom.toml"));
        assert_eq!(cfg.max_total_bytes, Some(222));
    }
}
