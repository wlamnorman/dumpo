use anyhow::{Context, Result};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Default, Clone, Deserialize)]
pub(crate) struct DumpoConfig {
    pub(crate) max_file_bytes: Option<usize>,
    pub(crate) max_total_bytes: Option<usize>,
    pub(crate) include_hidden: Option<bool>,
    pub(crate) include: Option<Vec<String>>,
    pub(crate) exclude: Option<Vec<String>>,
}

impl DumpoConfig {
    pub(crate) fn load_nearest(root: &Path) -> Result<(Option<PathBuf>, DumpoConfig)> {
        let cfg_path = find_nearest_config_path(root);
        let Some(path) = cfg_path.clone() else {
            return Ok((None, DumpoConfig::default()));
        };

        let s = fs::read_to_string(&path)
            .with_context(|| format!("failed to read config: {}", path.display()))?;

        let cfg: DumpoConfig = toml::from_str(&s)
            .with_context(|| format!("failed to parse config: {}", path.display()))?;

        Ok((Some(path), cfg))
    }
}

fn find_nearest_config_path(root: &Path) -> Option<PathBuf> {
    for dir in root.ancestors() {
        let p = dir.join("dumpo.toml");
        if p.is_file() {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempRepo;
    use std::fs;

    #[test]
    fn load_nearest_picks_repo_root_config() {
        let repo = TempRepo::new();
        repo.write("dumpo.toml", "max_total_bytes = 123\n");

        let (path, cfg) = DumpoConfig::load_nearest(repo.path()).unwrap();
        assert!(path.unwrap().ends_with("dumpo.toml"));
        assert_eq!(cfg.max_total_bytes, Some(123));
    }

    #[test]
    fn load_nearest_prefers_closer_config() {
        let repo = TempRepo::new();

        // parent config (repo root)
        repo.write("dumpo.toml", "max_total_bytes = 111\n");

        // nested dir with its own config
        let nested = repo.path().join("sub/dir");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nested.join("dumpo.toml"), "max_total_bytes = 222\n").unwrap();

        let (_path, cfg) = DumpoConfig::load_nearest(&nested).unwrap();
        assert_eq!(cfg.max_total_bytes, Some(222));
    }
}
