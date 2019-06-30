use ctrlc;
use snafu::{ErrorCompat, ResultExt, Snafu};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

mod config;
mod file_collector;
mod indexer;
mod last_modified_cache;
mod once_every;

#[derive(Debug, Snafu)]
enum SIDSError {
    #[snafu]
    ConfigLoad { source: config::Error },
    #[snafu]
    CollectorError { source: file_collector::Error },
    #[snafu]
    IndexerError { source: indexer::Error },
    #[snafu]
    LastModifiedCacheError { source: last_modified_cache::Error },
}

fn process_file(
    last_modified_cache: &last_modified_cache::LastModifiedCache,
    indexer: &mut indexer::DocIndexer,
    file: file_collector::FileEntry,
) -> Option<()> {
    use indexer::IndexAction;
    use last_modified_cache::FileCacheAction;

    let f = std::fs::File::open(&file.full_path).ok()?;
    let modified: u64 = f
        .metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_secs();

    let action = last_modified_cache
        .check_file(&file.full_path, modified)
        .ok()?;

    let index_action = match action {
        FileCacheAction::Outdated => IndexAction::ReIndex,
        FileCacheAction::UptoDate => return Some(()),
        FileCacheAction::NotIndexed => IndexAction::Index,
    };

    indexer.add_job(indexer::IndexRequest(file, index_action));

    Some(())
}

struct IndexerData {
    file_collector: file_collector::FilesCollectorIteror,
    doc_indexer: indexer::DocIndexer,
    modified_cache: last_modified_cache::LastModifiedCache,
    indexed_files: Arc<AtomicUsize>,
    failed_files: Arc<AtomicUsize>,
    running: Arc<AtomicBool>,
}

fn deploy_indexer(mut data: IndexerData) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        for file in data.file_collector {
            if let Ok(file) = file {
                if let Some(_) = process_file(&data.modified_cache, &mut data.doc_indexer, file) {
                    data.indexed_files.fetch_add(1, Ordering::Relaxed);
                } else {
                    data.failed_files.fetch_add(1, Ordering::Relaxed);
                }
            }

            if !data.running.load(Ordering::Relaxed) {
                break;
            }
        }

        data.doc_indexer.close();
    })
}

fn deploy_cc_handler() -> Arc<AtomicBool> {
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::Relaxed);
    })
    .expect("Error setting C-c handler");

    running
}

fn main_inner() -> Result<(), SIDSError> {
    let config = config::load_config().context(ConfigLoad)?;

    println!("config: {:#?}", config);

    let modified_cache =
        last_modified_cache::LastModifiedCache::new(&config).context(LastModifiedCacheError)?;

    let mut doc_indexer = indexer::DocIndexer::new(&config).context(IndexerError)?;
    doc_indexer.spawn_workers().context(IndexerError)?;

    let indexer = doc_indexer.indexer().clone();
    let schema = doc_indexer.schema().clone();

    let indexer_data = IndexerData {
        file_collector: file_collector::collect_files(&config).context(CollectorError)?,
        doc_indexer,
        modified_cache,
        indexed_files: Arc::new(AtomicUsize::new(0)),
        failed_files: Arc::new(AtomicUsize::new(0)),
        running: deploy_cc_handler(),
    };

    let indexer_thread = deploy_indexer(indexer_data);

    let _ = indexer_thread.join();

    Ok(())
}

fn main() {
    if let Err(e) = main_inner() {
        eprintln!("Oops: {}", e);
        if let Some(bt) = ErrorCompat::backtrace(&e) {
            eprintln!("{}", bt);
        }
    }
}
