use ctrlc;
use snafu::{ErrorCompat, ResultExt, Snafu};
use std::sync::{
    atomic::{AtomicBool, Ordering},
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

fn main_inner() -> Result<(), SIDSError> {
    let config = config::load_config().context(ConfigLoad)?;

    println!("config: {:#?}", config);

    let modified_cache =
        last_modified_cache::LastModifiedCache::new(&config).context(LastModifiedCacheError)?;

    let mut doc_indexer = indexer::DocIndexer::new(&config).context(IndexerError)?;
    doc_indexer.spawn_workers().context(IndexerError)?;

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::Relaxed);
    }).expect("Error setting C-c handler");

    for file in file_collector::collect_files(&config).context(CollectorError)? {
        if let Ok(file) = file {
            let _ = process_file(&modified_cache, &mut doc_indexer, file);
        }

        if !running.load(Ordering::Relaxed) {
            break;
        }
    }

    doc_indexer.close();

    let reader = doc_indexer.indexer.reader().unwrap();
    let searcher = reader.searcher();
    let qp = tantivy::query::QueryParser::for_index(
        &doc_indexer.indexer,
        vec![doc_indexer.schema.content],
    );

    let query = qp.parse_query("fuck").unwrap();

    let top_docs: Vec<(tantivy::Score, tantivy::DocAddress)> = searcher
        .search(&query, &tantivy::collector::TopDocs::with_limit(10))
        .unwrap();

    for (score, addr) in top_docs {
        let retr_doc = searcher.doc(addr).unwrap();

        println!(
            "{}: {}",
            score,
            doc_indexer.schema.schema.to_json(&retr_doc)
        );
    }

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
