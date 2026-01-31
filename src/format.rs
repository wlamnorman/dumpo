use std::path::Path;

pub(crate) const DUMP_TITLE: &str = "# dumpo pack";

pub(crate) const CODEBLOCK_CLOSE: &str = "```\n\n";
pub(crate) const TRUNCATION_FOOTER: &str = "\n... (truncated: max_total_bytes reached)\n";
pub(crate) const FILE_TRUNCATED_MARKER: &str = "(file truncated)\n\n";

pub(crate) fn root_line(root: &Path) -> String {
    format!("- root: {}", root.display())
}

pub(crate) fn file_heading(rel: &Path) -> String {
    format!("## {}", rel.display())
}

pub(crate) fn code_fence_open(path: &Path) -> String {
    format!("```{}", language_hint(path))
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
