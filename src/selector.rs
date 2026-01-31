use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};

#[derive(Debug, Clone)]
pub(crate) struct Selector {
    include: Option<GlobSet>, // None means "include all"
    exclude: Option<GlobSet>, // None means "exclude nothing"
}

impl Selector {
    pub(crate) fn new(includes: &[String], excludes: &[String]) -> Result<Self> {
        let include = if includes.is_empty() {
            None
        } else {
            Some(build_globset("--include", includes)?)
        };

        let exclude = if excludes.is_empty() {
            None
        } else {
            Some(build_globset("--exclude", excludes)?)
        };

        Ok(Self { include, exclude })
    }

    pub(crate) fn matches(&self, rel_path_slash: &str) -> bool {
        let included = match &self.include {
            None => true,
            Some(set) => set.is_match(rel_path_slash),
        };

        let not_excluded = match &self.exclude {
            None => true,
            Some(set) => !set.is_match(rel_path_slash),
        };

        included && not_excluded
    }
}

fn build_globset(flag: &str, patterns: &[String]) -> Result<GlobSet> {
    let mut b = GlobSetBuilder::new();
    for p in patterns {
        let g = Glob::new(p).with_context(|| format!("{flag}: invalid glob pattern: {p:?}"))?;
        b.add(g);
    }
    b.build()
        .with_context(|| format!("{flag}: failed to build glob set"))
}
