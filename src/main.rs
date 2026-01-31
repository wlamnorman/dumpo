use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use walkdir::{DirEntry, WalkDir};

const PRUNED_DIRS: [&str; 3] = [".git", "target", "node_modules"];
const EXCLUDED_FILENAMES: [&str; 5] = ["LICENSE", "Makefile", "Cargo.lock", "dump.md", "dumpo.md"];

const SECRET_FILENAMES: [&str; 1] = [".env"];
const SECRET_PREFIXES: [&str; 1] = [".env."];
const SECRET_EXTS: [&str; 4] = ["pem", "key", "p12", "pfx"];

const EXCLUDED_EXTS: [&str; 24] = [
    "png", "jpg", "jpeg", "gif", "webp", "pdf", "zip", "gz", "bz2", "xz", "7z", "woff", "woff2",
    "ttf", "otf", "mp4", "mov", "mp3", "wav", "bin", "exe", "dll", "so", "dylib",
];
const TRUNCATION_FOOTER: &str = "\n... (truncated: max_total_bytes reached)\n";

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

        #[arg(long, default_value_t = false)]
        no_copy: bool,
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
            no_copy,
        } => run_pack(
            &path,
            max_file_bytes,
            max_total_bytes,
            include_hidden,
            debug,
            stdout,
            no_copy,
        ),
    }
}

fn run_pack(
    root: &Path,
    max_file_bytes: usize,
    max_total_bytes: usize,
    include_hidden: bool,
    debug: bool,
    stdout: bool,
    no_copy: bool,
) -> Result<()> {
    let root = root
        .canonicalize()
        .with_context(|| format!("failed to canonicalize path: {}", root.display()))?;

    let bytes = build_dump_bytes(&root, max_file_bytes, max_total_bytes, include_hidden)?;

    if !no_copy {
        copy_to_clipboard(&bytes)?;
    }

    if debug {
        let debug_path = root.join("dumpo.md");
        fs::write(&debug_path, &bytes)
            .with_context(|| format!("failed to write {}", debug_path.display()))?;
    }

    if stdout {
        let mut out = io::stdout().lock();
        out.write_all(&bytes).context("failed writing to stdout")?;
    }

    Ok(())
}

fn copy_to_clipboard(bytes: &[u8]) -> Result<()> {
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

fn build_dump_bytes(
    root: &Path,
    max_file_bytes: usize,
    max_total_bytes: usize,
    include_hidden: bool,
) -> Result<Vec<u8>> {
    let reserved = TRUNCATION_FOOTER.len();
    let budget = max_total_bytes.saturating_sub(reserved);

    let mut out = Out::new(Vec::<u8>::new(), budget);

    out.write_line("# dumpo pack")?;
    out.write_line(&format!("- root: {}", root.display()))?;
    out.write_line("")?;

    let files = collect_files_sorted(root, include_hidden);

    let mut hit_total_limit = false;

    for (rel, path) in files {
        let bytes = match fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };

        if looks_binary(&bytes) {
            continue;
        }

        match print_file(&mut out, &rel, &path, &bytes, max_file_bytes) {
            Ok(()) => {}
            Err(PrintError::TotalLimitReached) => {
                hit_total_limit = true;
                break;
            }
        }
    }

    let mut buf = out.into_inner();
    if hit_total_limit {
        buf.extend_from_slice(TRUNCATION_FOOTER.as_bytes());
    }

    Ok(buf)
}

fn collect_files_sorted(root: &Path, include_hidden: bool) -> Vec<(PathBuf, PathBuf)> {
    let mut files: Vec<(PathBuf, PathBuf)> = Vec::new();

    for entry in WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !should_prune_walk_entry(e, include_hidden))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        if entry.file_type().is_dir() {
            continue;
        }

        let path = entry.into_path();

        if should_skip_file(&path, include_hidden) {
            continue;
        }

        let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
        files.push((rel, path));
    }

    files.sort_by(|(a_rel, _), (b_rel, _)| a_rel.as_os_str().cmp(b_rel.as_os_str()));
    files
}

fn should_prune_walk_entry(e: &DirEntry, include_hidden: bool) -> bool {
    let name = e.file_name().to_string_lossy();

    if e.file_type().is_dir() && PRUNED_DIRS.iter().any(|d| name == *d) {
        return true;
    }

    if !include_hidden && is_hidden(&name) {
        return true;
    }

    false
}

fn should_skip_file(path: &Path, include_hidden: bool) -> bool {
    let name = match path.file_name().and_then(|s| s.to_str()) {
        Some(n) => n,
        None => return true,
    };

    if is_secret_name(name) {
        return true;
    }

    if EXCLUDED_FILENAMES.contains(&name) {
        return true;
    }

    if !include_hidden && is_hidden(name) {
        return true;
    }

    if has_extension_in(path, &EXCLUDED_EXTS) {
        return true;
    }

    if has_extension_in(path, &SECRET_EXTS) {
        return true;
    }

    false
}

fn is_hidden(name: &str) -> bool {
    name.starts_with('.') && name != "."
}

fn is_secret_name(name: &str) -> bool {
    if SECRET_FILENAMES.contains(&name) {
        return true;
    }
    SECRET_PREFIXES.iter().any(|p| name.starts_with(*p))
}

fn has_extension_in(path: &Path, exts: &[&str]) -> bool {
    let ext = match path.extension().and_then(|e| e.to_str()) {
        Some(e) => e,
        None => return false,
    };
    exts.iter().any(|x| ext.eq_ignore_ascii_case(x))
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0)
}

fn language_hint(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "rs" => "rust",
        "toml" => "toml",
        "md" => "markdown",
        "yml" | "yaml" => "yaml",
        "json" => "json",
        "py" => "python",
        "sh" => "bash",
        _ => "",
    }
}

#[derive(Debug, Clone, Copy)]
enum PrintError {
    TotalLimitReached,
}

fn print_file(
    out: &mut Out<impl Write>,
    rel: &Path,
    path: &Path,
    bytes: &[u8],
    max_file_bytes: usize,
) -> std::result::Result<(), PrintError> {
    out.try_write_line(&format!("## {}", rel.display()))?;
    out.try_write_line("")?;
    out.try_write_line(&format!("```{}", language_hint(path)))?;

    let reserve_for_footer = "\n```\n\n".len();
    let remaining_for_content = out.remaining().saturating_sub(reserve_for_footer);
    if remaining_for_content == 0 {
        return Err(PrintError::TotalLimitReached);
    }

    let cap = max_file_bytes.min(remaining_for_content);
    let slice = &bytes[..bytes.len().min(cap)];

    let text = String::from_utf8_lossy(slice);
    out.try_write(&text)?;

    if !text.ends_with('\n') {
        out.try_write_line("")?;
    }

    out.try_write_line("```")?;
    out.try_write_line("")?;

    if bytes.len() > cap {
        let _ = out.try_write_line("(file truncated)");
        let _ = out.try_write_line("");
    }

    Ok(())
}

struct Out<W: Write> {
    w: W,
    written: usize,
    max: usize,
}

impl<W: Write> Out<W> {
    fn new(w: W, max: usize) -> Self {
        Self { w, written: 0, max }
    }

    fn into_inner(self) -> W {
        self.w
    }

    fn remaining(&self) -> usize {
        self.max.saturating_sub(self.written)
    }

    fn try_write(&mut self, s: &str) -> std::result::Result<(), PrintError> {
        if s.is_empty() {
            return Ok(());
        }
        let n = s.len();
        if self.written.saturating_add(n) > self.max {
            return Err(PrintError::TotalLimitReached);
        }
        self.w
            .write_all(s.as_bytes())
            .map_err(|_| PrintError::TotalLimitReached)?;
        self.written += n;
        Ok(())
    }

    fn try_write_line(&mut self, s: &str) -> std::result::Result<(), PrintError> {
        self.try_write(s)?;
        self.try_write("\n")?;
        Ok(())
    }

    fn write_line(&mut self, s: &str) -> Result<()> {
        self.try_write_line(s)
            .map_err(|_| anyhow::anyhow!("max_total_bytes reached"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempRepo {
        root: PathBuf,
    }

    impl TempRepo {
        fn new() -> Self {
            let mut root = std::env::temp_dir();
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            root.push(format!("dumpo-test-{}", nanos));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn path(&self) -> &Path {
            &self.root
        }

        fn write(&self, rel: &str, contents: &str) {
            let p = self.root.join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(p, contents).unwrap();
        }
    }

    impl Drop for TempRepo {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn should_skip_file_excludes_secrets_even_when_hidden_included() {
        let repo = TempRepo::new();

        repo.write(".env", "SECRET=1");
        repo.write(".env.local", "SECRET=2");

        let env = repo.path().join(".env");
        let env_local = repo.path().join(".env.local");

        assert!(should_skip_file(&env, true));
        assert!(should_skip_file(&env_local, true));
        assert!(should_skip_file(&env, false));
        assert!(should_skip_file(&env_local, false));
    }

    #[test]
    fn should_skip_file_respects_include_hidden_flag_for_non_secrets() {
        let repo = TempRepo::new();

        repo.write(".hidden.txt", "ok");
        let hidden = repo.path().join(".hidden.txt");

        assert!(should_skip_file(&hidden, false));
        assert!(!should_skip_file(&hidden, true));
    }

    #[test]
    fn should_skip_file_excludes_lockfiles_and_self_outputs() {
        let repo = TempRepo::new();

        repo.write("Cargo.lock", "lock");
        repo.write("dumpo.md", "self");
        repo.write("dump.md", "self");

        assert!(should_skip_file(&repo.path().join("Cargo.lock"), true));
        assert!(should_skip_file(&repo.path().join("dumpo.md"), true));
        assert!(should_skip_file(&repo.path().join("dump.md"), true));
    }

    #[test]
    fn should_skip_file_excludes_binaryish_extensions_case_insensitive() {
        let repo = TempRepo::new();

        repo.write("a.PNG", "not actually png but extension should exclude");
        repo.write("b.PdF", "not actually pdf but extension should exclude");

        assert!(should_skip_file(&repo.path().join("a.PNG"), true));
        assert!(should_skip_file(&repo.path().join("b.PdF"), true));
    }

    #[test]
    fn collect_files_sorted_skips_pruned_dirs() {
        let repo = TempRepo::new();

        repo.write("target/keep.rs", "nope");
        repo.write(".git/config", "nope");
        repo.write("node_modules/x.js", "nope");
        repo.write("src/lib.rs", "yes");

        let got: Vec<PathBuf> = collect_files_sorted(repo.path(), true)
            .into_iter()
            .map(|(rel, _)| rel)
            .collect();

        assert_eq!(got, vec![PathBuf::from("src/lib.rs")]);
    }

    #[test]
    fn build_dump_bytes_enforces_max_file_bytes_truncation() {
        let repo = TempRepo::new();

        let long = "a".repeat(1_000);
        repo.write("src/lib.rs", &long);

        let out = build_dump_bytes(repo.path(), 50, 10_000, true).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("## src/lib.rs"));
        assert!(s.contains("(file truncated)"));
        assert!(!s.contains(&long));
    }

    #[test]
    fn build_dump_bytes_enforces_max_total_bytes_truncation_marker() {
        let repo = TempRepo::new();

        repo.write("a.rs", &"a".repeat(2_000));
        repo.write("b.rs", &"b".repeat(2_000));
        repo.write("c.rs", &"c".repeat(2_000));

        let out = build_dump_bytes(repo.path(), 2_000, 1_200, true).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("... (truncated: max_total_bytes reached)"));
    }

    #[test]
    fn looks_binary_detects_nul_byte() {
        assert!(looks_binary(b"abc\0def"));
        assert!(!looks_binary(b"abcdef"));
    }

    #[test]
    fn collect_files_sorted_is_deterministic_and_lexicographic() {
        let repo = TempRepo::new();

        // Create in intentionally “unsorted” order.
        repo.write("z.rs", "z");
        repo.write("a.rs", "a");
        repo.write("dir/c.rs", "c");
        repo.write("dir/b.rs", "b");

        let got1: Vec<PathBuf> = collect_files_sorted(repo.path(), true)
            .into_iter()
            .map(|(rel, _)| rel)
            .collect();

        let got2: Vec<PathBuf> = collect_files_sorted(repo.path(), true)
            .into_iter()
            .map(|(rel, _)| rel)
            .collect();

        let expected = vec![
            PathBuf::from("a.rs"),
            PathBuf::from("dir/b.rs"),
            PathBuf::from("dir/c.rs"),
            PathBuf::from("z.rs"),
        ];

        assert_eq!(got1, expected);
        assert_eq!(got2, expected);
    }

    #[test]
    fn should_skip_file_excludes_default_meta_files() {
        let repo = TempRepo::new();

        repo.write("LICENSE", "mit");
        repo.write("Makefile", "all:\n\techo hi\n");

        assert!(should_skip_file(&repo.path().join("LICENSE"), true));
        assert!(should_skip_file(&repo.path().join("Makefile"), true));
        assert!(should_skip_file(&repo.path().join("LICENSE"), false));
        assert!(should_skip_file(&repo.path().join("Makefile"), false));
    }
}
