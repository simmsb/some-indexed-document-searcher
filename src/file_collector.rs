use crossbeam_channel::{unbounded, Receiver};
use glob;
use notify::{watcher, RecursiveMode, Watcher};
use snafu::{ResultExt, Snafu};
use std::{collections::HashSet, path::PathBuf, time::Duration};
use walkdir::{DirEntry, WalkDir};

use super::config;
use super::last_modified_cache;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Could not read glob pattern '{}': {}", glob, source))]
    GlobParseError {
        glob: String,
        source: glob::PatternError,
    },
    #[snafu(display("Could not read glob {}", source))]
    GlobError { source: glob::GlobError },
    #[snafu(display("Could not walk something: {}", source))]
    WalkDirError { source: walkdir::Error },
    #[snafu(display("Could not notify from something: {}", source))]
    NotifyError { source: notify::Error },
}

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug)]
pub enum CollectorOp {
    Index,
    ReIndex,
    Delete,
}

#[derive(Debug)]
pub struct FileEntry {
    pub full_path: PathBuf,
    pub operation: CollectorOp,
}

impl FileEntry {
    pub fn full_path(&self) -> &str {
        self.full_path
            .to_str()
            .expect("Couldn't convert OsStr to str")
    }

    pub fn file_name(&self) -> &str {
        self.full_path
            .file_name()
            .expect("Couldn't get file name")
            .to_str()
            .expect("Couldn't convert OsStr to str")
    }

    pub fn file_ext(&self) -> &str {
        self.full_path
            .extension()
            .expect("Couldn't get file ext")
            .to_str()
            .expect("Couldn't convert OsStr to str")
    }
}

enum FileCollectorIteratorMode {
    WalkDir(Option<walkdir::IntoIter>, Vec<PathBuf>),
    Notify(Receiver<notify::Result<notify::Event>>),
}

enum CIterMAction {
    Result(Option<(Vec<PathBuf>, bool)>),
    Stop,
    IntoNotify,
}

impl FileCollectorIteratorMode {
    fn into_notify(&mut self, roots: &[PathBuf]) {

        println!("Swapping from directory traversal to notify");

        let (tx, rx) = unbounded();

        let mut watcher = watcher(tx, Duration::from_secs(5)).unwrap();

        for root in roots {
            watcher.watch(root, RecursiveMode::Recursive).unwrap();
        }

        *self = FileCollectorIteratorMode::Notify(rx);
    }

    fn fetch_next_root_iter(roots: &mut Vec<PathBuf>) -> Option<walkdir::IntoIter> {
        roots
            .pop()
            .map(|r| WalkDir::new(r).follow_links(true).into_iter())
    }

    fn next_inner(&mut self) -> CIterMAction {
        use notify::event::EventKind;

        match self {
            FileCollectorIteratorMode::WalkDir(it, roots) => loop {
                if it.is_none() {
                    let new_it = match FileCollectorIteratorMode::fetch_next_root_iter(roots) {
                        Some(it) => it,
                        None => return CIterMAction::IntoNotify,
                    };

                    it.replace(new_it);
                }

                let dent = match it.as_mut().unwrap().next() {
                    Some(result) => match result {
                        Ok(v) => v,
                        Err(e) => continue,
                    },
                    None => return CIterMAction::IntoNotify,
                };

                return CIterMAction::Result(Some((vec![dent.path().to_path_buf()], true)));
            },
            FileCollectorIteratorMode::Notify(ch) => loop {
                let e = match ch.recv().ok() {
                    Some(e) => e,
                    None => return CIterMAction::Stop,
                };

                if let Ok(e) = e {
                    let flag = match e.kind {
                        EventKind::Create(_) | EventKind::Modify(_) => true,
                        EventKind::Remove(_) => false,
                        _ => continue,
                    };
                    return CIterMAction::Result(Some((e.paths, flag)));
                }
            },
        }
    }

    fn next(&mut self, roots: &[PathBuf]) -> Option<(Vec<PathBuf>, bool)> {
        loop {
            let r = self.next_inner();
            match r {
                CIterMAction::Result(r) => return r,
                CIterMAction::Stop => return None,
                CIterMAction::IntoNotify => self.into_notify(roots),
            }
        }
    }
}

pub struct FilesCollectorIteror {
    ignored: Vec<glob::Pattern>,
    roots: Vec<PathBuf>,
    exts: HashSet<String>,
    last_modified_cache: last_modified_cache::LastModifiedCache,
    current_iterator: FileCollectorIteratorMode,
    extra_paths: Vec<(PathBuf, bool)>,
}

impl FilesCollectorIteror {
    fn new(
        ignored: Vec<glob::Pattern>,
        roots: Vec<PathBuf>,
        exts: HashSet<String>,
        last_modified_cache: last_modified_cache::LastModifiedCache,
    ) -> Self {
        let walker_roots = roots.clone();

        FilesCollectorIteror {
            ignored,
            roots,
            exts,
            last_modified_cache,
            current_iterator: FileCollectorIteratorMode::WalkDir(None, walker_roots),
            extra_paths: Vec::new(),
        }
    }
    fn fetch_next_root_iter(&mut self) -> Option<walkdir::IntoIter> {
        self.roots
            .pop()
            .map(|r| WalkDir::new(r).follow_links(true).into_iter())
    }

    fn predicate(&self, entry: &std::path::Path) -> bool {
        entry
            .file_name()
            .map(|s| !self.ignored.iter().any(|p| p.matches(s.to_str().unwrap())))
            .unwrap_or(false)
    }
}

impl Iterator for FilesCollectorIteror {
    type Item = Result<FileEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        use super::last_modified_cache::FileCacheAction;

        loop {
            let (path, was_removed) = match self.extra_paths.pop() {
                Some(p) => p,
                None => {
                    let (mut paths, was_removed) = self.current_iterator.next(&self.roots)?;

                    let path_to_use = match paths.pop() {
                        Some(p) => p,
                        None => continue,
                    };

                    // if paths is now empty, don't replace extra_paths since it's already empty

                    if !paths.is_empty() {
                        self.extra_paths = paths.into_iter().map(|p| (p, was_removed)).collect();
                    }

                    (path_to_use, was_removed)
                }
            };

            if !self.predicate(&path) {
                if path.is_dir() {
                    if let FileCollectorIteratorMode::WalkDir(Some(ref mut it), _) =
                        self.current_iterator
                    {
                        it.skip_current_dir();
                    }
                }
                continue;
            }

            // never yield directories
            if path.is_dir() {
                continue;
            }

            // skip extensions we don't care about
            if !path
                .extension()
                .and_then(std::ffi::OsStr::to_str)
                .map(|e| self.exts.contains(e))
                .unwrap_or(false)
            {
                continue;
            }

            let f = std::fs::File::open(&path).unwrap();

            let modified: u64 = f
                .metadata()
                .ok()?
                .modified()
                .ok()?
                .duration_since(std::time::UNIX_EPOCH)
                .ok()?
                .as_secs();

            let action = self.last_modified_cache.check_file(&path, modified).ok()?;

            let op = match action {
                FileCacheAction::Outdated => CollectorOp::ReIndex,
                FileCacheAction::UptoDate => continue,
                FileCacheAction::NotIndexed => CollectorOp::Index,
            };

            return Some(Ok(FileEntry {
                full_path: path,
                operation: op,
            }));
        }
    }
}
pub fn collect_files(
    config: &config::Config,
    last_modified_cache: last_modified_cache::LastModifiedCache,
) -> Result<FilesCollectorIteror> {
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

    Ok(FilesCollectorIteror::new(
        ignored,
        roots,
        config.indexed_exts.iter().cloned().collect(),
        last_modified_cache,
    ))
}
