use glob;
use snafu::{ResultExt, Snafu};
use std::{collections::HashSet, path::PathBuf};
use walkdir::{DirEntry, WalkDir};

use super::config;

#[derive(Debug, Snafu)]
pub enum FileCollectorError {
    #[snafu(display("Could not read glob pattern '{}': {}", glob, source))]
    GlobParseError {
        glob: String,
        source: glob::PatternError,
    },
    #[snafu(display("Could not read glob {}", source))]
    GlobError { source: glob::GlobError },
    #[snafu(display("Could not walk something: {}", source))]
    WalkDirError { source: walkdir::Error },
}

type Result<T, E = FileCollectorError> = std::result::Result<T, E>;

pub struct FilesCollectorIteror {
    ignored: Vec<glob::Pattern>,
    roots: Vec<PathBuf>,
    exts: HashSet<String>,
    current_iterator: Option<walkdir::IntoIter>,
}

impl FilesCollectorIteror {
    fn fetch_next_root_iter(&mut self) -> Option<walkdir::IntoIter> {
        self.roots
            .pop()
            .map(|r| WalkDir::new(r).follow_links(true).into_iter())
    }

    fn predicate(&self, entry: &DirEntry) -> bool {
        entry
            .file_name()
            .to_str()
            .map(|s| !self.ignored.iter().any(|p| p.matches(s)))
            .unwrap_or(false)

        // if !p {
        //     println!("Ignoring entry: {:?}", entry);
        // }
        // p
    }
}

impl Iterator for FilesCollectorIteror {
    type Item = Result<DirEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let mut cur_it = match self.current_iterator.take() {
                Some(it) => it,
                None => self.fetch_next_root_iter()?,
            };

            let dent = match cur_it.next() {
                Some(result) => match result.context(WalkDirError) {
                    Ok(v) => v,
                    Err(e) => {
                        self.current_iterator = Some(cur_it);
                        return Some(Err(e));
                    }
                },
                None => return None,
            };

            if !self.predicate(&dent) {
                if dent.path().is_dir() {
                    cur_it.skip_current_dir();
                }
                self.current_iterator = Some(cur_it);
                continue;
            }

            self.current_iterator = Some(cur_it);

            // never yield directories
            if dent.path().is_dir() {
                continue;
            }

            // skip extensions we don't care about
            if !dent
                .path()
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .map(|e| self.exts.contains(e))
                .unwrap_or(false)
            {
                continue;
            }

            return Some(Ok(dent));
        }
    }
}

pub fn collect_files(config: &config::Config) -> Result<FilesCollectorIteror> {
    let roots: Vec<glob::Paths> = config
        .root_globs
        .iter()
        .map(|g| glob::glob(g).with_context(|| GlobParseError { glob: g.to_owned() }))
        .collect::<Result<Vec<_>>>()?;

    let roots: Vec<PathBuf> = roots
        .into_iter()
        .flatten()
        .map(|p| p.context(GlobError))
        .collect::<Result<Vec<_>>>()?;

    let ignored: Vec<glob::Pattern> = config
        .ignored_globs
        .iter()
        .map(|g| glob::Pattern::new(g).with_context(|| GlobParseError { glob: g.to_owned() }))
        .collect::<Result<Vec<_>>>()?;

    Ok(FilesCollectorIteror {
        ignored,
        roots,
        exts: config.indexed_exts.iter().cloned().collect(),
        current_iterator: None,
    })
}
