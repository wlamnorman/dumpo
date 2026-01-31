use anyhow::Result;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

use crate::filter::{should_prune_walk_entry, should_skip_file};

const TRUNCATION_FOOTER: &str = "\n... (truncated: max_total_bytes reached)\n";

pub(crate) fn build_dump_bytes(
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

pub(crate) fn collect_files_sorted(root: &Path, include_hidden: bool) -> Vec<(PathBuf, PathBuf)> {
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
    use crate::testutil::TempRepo;

    #[test]
    fn collect_files_sorted_is_deterministic_and_lexicographic() {
        let repo = TempRepo::new();

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
    fn build_dump_bytes_is_ordered_and_respects_hidden_flag_end_to_end() {
        let repo = TempRepo::new();

        repo.write("z.rs", "fn z() {}\n");
        repo.write("a.rs", "fn a() {}\n");
        repo.write("dir/b.rs", "fn b() {}\n");
        repo.write(".hidden.txt", "secret-ish but not excluded\n");

        let out_no_hidden = build_dump_bytes(repo.path(), 10_000, 200_000, false).unwrap();
        let s1 = String::from_utf8(out_no_hidden).unwrap();

        let a_idx = s1.find("## a.rs").unwrap();
        let b_idx = s1.find("## dir/b.rs").unwrap();
        let z_idx = s1.find("## z.rs").unwrap();

        assert!(a_idx < b_idx);
        assert!(b_idx < z_idx);

        assert!(s1.contains("```rust"));
        assert!(s1.contains("fn a() {}"));
        assert!(s1.contains("fn b() {}"));
        assert!(s1.contains("fn z() {}"));

        assert!(!s1.contains("## .hidden.txt"));
        assert!(!s1.contains("secret-ish but not excluded"));

        let out_with_hidden = build_dump_bytes(repo.path(), 10_000, 200_000, true).unwrap();
        let s2 = String::from_utf8(out_with_hidden).unwrap();

        assert!(s2.contains("## .hidden.txt"));
        assert!(s2.contains("secret-ish but not excluded"));
        assert!(s2.contains("```"));
    }

    #[test]
    fn looks_binary_detects_nul_byte() {
        assert!(super::looks_binary(b"abc\0def"));
        assert!(!super::looks_binary(b"abcdef"));
    }
}
