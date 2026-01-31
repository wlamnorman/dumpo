use crate::filter::{should_prune_walk_entry, should_skip_file};
use crate::format as fmt;
use crate::selector::Selector;
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

pub(crate) fn build_dump_bytes(
    root: &Path,
    max_file_bytes: usize,
    max_total_bytes: usize,
    include_hidden: bool,
    selector: &Selector,
) -> Result<Vec<u8>> {
    // Reserve space for the footer so that, if we hit the budget, we can always append it.
    let budget = max_total_bytes.saturating_sub(fmt::TRUNCATION_FOOTER.len());

    let mut out = Out::new(budget);
    out.push_line(fmt::DUMP_TITLE)?;
    out.push_line(&fmt::root_line(root))?;
    out.push_line("")?;

    let mut hit_total_limit = false;
    for (rel, path) in collect_files_sorted(root, include_hidden, selector) {
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
        buf.extend_from_slice(fmt::TRUNCATION_FOOTER.as_bytes());
    }

    Ok(buf)
}

pub(crate) fn collect_files_sorted(
    root: &Path,
    include_hidden: bool,
    selector: &Selector,
) -> Vec<(PathBuf, PathBuf)> {
    let mut files = Vec::new();

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

        let rel_slash = rel.to_string_lossy().replace('\\', "/");
        if !selector.matches(&rel_slash) {
            continue;
        }

        files.push((rel, path));
    }

    files.sort_by(|(a_rel, _), (b_rel, _)| a_rel.as_os_str().cmp(b_rel.as_os_str()));
    files
}

fn looks_binary(bytes: &[u8]) -> bool {
    bytes.contains(&0)
}

fn clamp_to_utf8_boundary(bytes: &[u8], mut end: usize) -> usize {
    end = end.min(bytes.len());
    // UTF-8 codepoints are max 4 bytes
    while end > 0 && std::str::from_utf8(&bytes[..end]).is_err() {
        end -= 1;
    }
    end
}

#[derive(Debug, Clone, Copy)]
enum PrintError {
    TotalLimitReached,
}

impl std::fmt::Display for PrintError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PrintError::TotalLimitReached => write!(f, "max_total_bytes reached"),
        }
    }
}

impl std::error::Error for PrintError {}

fn print_file(
    out: &mut Out,
    rel: &Path,
    path: &Path,
    bytes: &[u8],
    max_file_bytes: usize,
) -> std::result::Result<(), PrintError> {
    out.push_line(&fmt::file_heading(rel))?;
    out.push_line("")?;
    out.push_line(&fmt::code_fence_open(path))?;

    let remaining = out.remaining();
    if remaining <= fmt::CODEBLOCK_CLOSE.len() {
        return Err(PrintError::TotalLimitReached);
    }

    // Start by reserving only the closing fence. If we end up truncating, we'll
    // also reserve for the truncation marker by shrinking the cap.
    let max_content_by_total = remaining - fmt::CODEBLOCK_CLOSE.len();
    let mut cap = max_file_bytes.min(max_content_by_total).min(bytes.len());

    // If truncation will occur, ensure we can also fit the truncation marker.
    if cap < bytes.len() {
        let needed_after_content = fmt::CODEBLOCK_CLOSE.len() + fmt::FILE_TRUNCATED_MARKER.len();
        if remaining <= needed_after_content {
            // Make room for the marker by reducing content further.
            let max_content_with_marker = remaining.saturating_sub(needed_after_content);
            if max_content_with_marker == 0 {
                return Err(PrintError::TotalLimitReached);
            }
            cap = cap.min(max_content_with_marker);
        }
    }

    let cap = clamp_to_utf8_boundary(bytes, cap);
    let text = String::from_utf8_lossy(&bytes[..cap]);
    out.push_str(&text)?;

    if !text.ends_with('\n') {
        out.push_line("")?;
    }

    out.push_str(fmt::CODEBLOCK_CLOSE)?;
    if cap < bytes.len() {
        out.push_str(fmt::FILE_TRUNCATED_MARKER)?;
    }

    Ok(())
}

struct Out {
    buf: Vec<u8>,
    max: usize,
}

impl Out {
    fn new(max: usize) -> Self {
        Self {
            buf: Vec::new(),
            max,
        }
    }

    fn into_inner(self) -> Vec<u8> {
        self.buf
    }

    fn remaining(&self) -> usize {
        self.max.saturating_sub(self.buf.len())
    }

    fn push_str(&mut self, s: &str) -> std::result::Result<(), PrintError> {
        if s.is_empty() {
            return Ok(());
        }
        if self.buf.len().saturating_add(s.len()) > self.max {
            return Err(PrintError::TotalLimitReached);
        }
        self.buf.extend_from_slice(s.as_bytes());
        Ok(())
    }

    fn push_line(&mut self, s: &str) -> std::result::Result<(), PrintError> {
        self.push_str(s)?;
        self.push_str("\n")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    fn sel_all() -> crate::selector::Selector {
        crate::selector::Selector::new(&[], &[]).unwrap()
    }

    fn sel(includes: &[&str], excludes: &[&str]) -> crate::selector::Selector {
        let inc: Vec<String> = includes.iter().map(|s| s.to_string()).collect();
        let exc: Vec<String> = excludes.iter().map(|s| s.to_string()).collect();
        crate::selector::Selector::new(&inc, &exc).unwrap()
    }

    #[test]
    fn collect_files_sorted_is_deterministic_and_lexicographic() {
        let repo = TempRepo::new();

        repo.write("z.rs", "z");
        repo.write("a.rs", "a");
        repo.write("dir/c.rs", "c");
        repo.write("dir/b.rs", "b");

        let selector = sel_all();

        let got1: Vec<PathBuf> = collect_files_sorted(repo.path(), true, &selector)
            .into_iter()
            .map(|(rel, _)| rel)
            .collect();

        let got2: Vec<PathBuf> = collect_files_sorted(repo.path(), true, &selector)
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

        let selector = sel_all();
        let out = build_dump_bytes(repo.path(), 50, 10_000, true, &selector).unwrap();
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

        let selector = sel_all();
        let out = build_dump_bytes(repo.path(), 2_000, 1_200, true, &selector).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains(crate::format::TRUNCATION_FOOTER.trim_end()));
    }

    #[test]
    fn build_dump_bytes_is_ordered_and_respects_hidden_flag_end_to_end() {
        let repo = TempRepo::new();

        repo.write("z.rs", "fn z() {}\n");
        repo.write("a.rs", "fn a() {}\n");
        repo.write("dir/b.rs", "fn b() {}\n");
        repo.write(".hidden.txt", "secret-ish but not excluded\n");

        let selector = sel_all();

        let out_no_hidden =
            build_dump_bytes(repo.path(), 10_000, 200_000, false, &selector).unwrap();
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

        let out_with_hidden =
            build_dump_bytes(repo.path(), 10_000, 200_000, true, &selector).unwrap();
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

    #[test]
    fn build_dump_bytes_never_exceeds_max_total_bytes() {
        let repo = TempRepo::new();

        repo.write("a.rs", &"a".repeat(50_000));
        repo.write("b.rs", &"b".repeat(50_000));

        let selector = sel_all();
        let max_total = 1_200;
        let out = build_dump_bytes(repo.path(), 50_000, max_total, true, &selector).unwrap();

        assert!(out.len() <= max_total);
    }

    #[test]
    fn build_dump_bytes_truncation_footer_is_appended_and_within_budget() {
        let repo = TempRepo::new();

        // One huge file is enough to force total truncation when max_total is small.
        repo.write("a.rs", &"a".repeat(50_000));

        let selector = sel_all();
        let max_total = 500;
        let out = build_dump_bytes(repo.path(), 50_000, max_total, true, &selector).unwrap();

        assert!(out.len() <= max_total);

        let s = String::from_utf8(out).unwrap();
        assert!(s.contains(crate::format::TRUNCATION_FOOTER.trim_end()));
    }

    #[test]
    fn build_dump_bytes_respects_include_globs() {
        let repo = TempRepo::new();
        repo.write("src/lib.rs", "x\n");
        repo.write("README.md", "y\n");

        let sel = crate::selector::Selector::new(&["src/**".to_string()], &[]).unwrap();

        let out = build_dump_bytes(repo.path(), 10_000, 200_000, true, &sel).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("## src/lib.rs"));
        assert!(!s.contains("## README.md"));
    }

    #[test]
    fn build_dump_bytes_exclude_overrides_include() {
        let repo = TempRepo::new();
        repo.write("src/lib.rs", "x\n");
        repo.write("src/secret.rs", "y\n");

        let sel =
            crate::selector::Selector::new(&["src/**".to_string()], &["**/secret.rs".to_string()])
                .unwrap();

        let out = build_dump_bytes(repo.path(), 10_000, 200_000, true, &sel).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("## src/lib.rs"));
        assert!(!s.contains("## src/secret.rs"));
    }

    #[test]
    fn selector_cannot_include_secrets() {
        let repo = TempRepo::new();
        repo.write(".env", "SECRET=1\n");

        let selector = sel(&[".env"], &[]);
        let out = build_dump_bytes(repo.path(), 10_000, 200_000, true, &selector).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(!s.contains("## .env"));
        assert!(!s.contains("SECRET=1"));
    }

    #[test]
    fn build_dump_bytes_respects_exclude_globs_without_includes() {
        let repo = TempRepo::new();
        repo.write("src/lib.rs", "x\n");
        repo.write("README.md", "y\n");

        let sel = crate::selector::Selector::new(&[], &["README.md".to_string()]).unwrap();

        let out = build_dump_bytes(repo.path(), 10_000, 200_000, true, &sel).unwrap();
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("## src/lib.rs"));
        assert!(!s.contains("## README.md"));
    }
}
