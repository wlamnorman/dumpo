use std::path::Path;
use walkdir::DirEntry;

pub(crate) const PRUNED_DIRS: [&str; 3] = [".git", "target", "node_modules"];
pub(crate) const EXCLUDED_FILENAMES: [&str; 4] =
    ["LICENSE", "Makefile", "Cargo.lock", ".dumpo.debug.md"];

pub(crate) const SECRET_FILENAMES: [&str; 1] = [".env"];
pub(crate) const SECRET_PREFIXES: [&str; 1] = [".env."];
pub(crate) const SECRET_EXTS: [&str; 4] = ["pem", "key", "p12", "pfx"];

pub(crate) const EXCLUDED_EXTS: [&str; 24] = [
    "png", "jpg", "jpeg", "gif", "webp", "pdf", "zip", "gz", "bz2", "xz", "7z", "woff", "woff2",
    "ttf", "otf", "mp4", "mov", "mp3", "wav", "bin", "exe", "dll", "so", "dylib",
];

pub(crate) fn should_prune_walk_entry(e: &DirEntry, include_hidden: bool) -> bool {
    let name = e.file_name().to_string_lossy();

    if e.file_type().is_dir() && PRUNED_DIRS.iter().any(|d| name == *d) {
        return true;
    }

    if !include_hidden && is_hidden(&name) {
        return true;
    }

    false
}

pub(crate) fn should_skip_file(path: &Path, include_hidden: bool) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;

    #[test]
    fn should_skip_file_excludes_secrets_even_when_hidden_included() {
        let repo = TempRepo::new();
        repo.write(".env", "SECRET=1");
        repo.write(".env.local", "SECRET=2");

        assert!(should_skip_file(&repo.path().join(".env"), true));
        assert!(should_skip_file(&repo.path().join(".env.local"), true));
        assert!(should_skip_file(&repo.path().join(".env"), false));
        assert!(should_skip_file(&repo.path().join(".env.local"), false));
    }

    #[test]
    fn should_skip_file_respects_include_hidden_flag_for_non_secrets() {
        let repo = TempRepo::new();
        repo.write(".hidden.txt", "ok");

        assert!(should_skip_file(&repo.path().join(".hidden.txt"), false));
        assert!(!should_skip_file(&repo.path().join(".hidden.txt"), true));
    }

    #[test]
    fn should_skip_file_excludes_lockfile() {
        let repo = TempRepo::new();
        repo.write("Cargo.lock", "lock");
        assert!(should_skip_file(&repo.path().join("Cargo.lock"), true));
    }

    #[test]
    fn should_skip_file_excludes_binaryish_extensions_case_insensitive() {
        let repo = TempRepo::new();
        repo.write("a.PNG", "x");
        repo.write("b.PdF", "x");

        assert!(should_skip_file(&repo.path().join("a.PNG"), true));
        assert!(should_skip_file(&repo.path().join("b.PdF"), true));
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

    #[test]
    fn should_skip_file_excludes_debug_output_file() {
        let repo = TempRepo::new();
        repo.write(".dumpo.debug.md", "debug");

        assert!(should_skip_file(&repo.path().join(".dumpo.debug.md"), true));
        assert!(should_skip_file(
            &repo.path().join(".dumpo.debug.md"),
            false
        ));
    }
}
