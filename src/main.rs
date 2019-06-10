use snafu::{ErrorCompat, ResultExt, Snafu};

mod config;
mod file_collector;
mod indexer;

#[derive(Debug, Snafu)]
enum SIDSError {
    #[snafu]
    ConfigLoad { source: config::ConfigError },
    #[snafu]
    CollectorError {
        source: file_collector::FileCollectorError,
    },
    #[snafu]
    IndexerError { source: indexer::IndexerError },
}

fn main_inner() -> Result<(), SIDSError> {
    let config = config::load_config().context(ConfigLoad)?;

    println!("config: {:#?}", config);

    let mut doc_indexer = indexer::DocIndexer::new(&config)
        .context(IndexerError)?;
    doc_indexer.spawn_workers()
        .context(IndexerError)?;

    for file in file_collector::collect_files(&config).context(CollectorError)? {
        if let Ok(file) = file {
            println!("{:?}", file);
            doc_indexer.add_job(indexer::IndexRequest(file));
        }
    }

    doc_indexer.close();

    let reader = doc_indexer.indexer.reader().unwrap();
    let searcher = reader.searcher();
    let qp = tantivy::query::QueryParser::for_index(&doc_indexer.indexer, vec![doc_indexer.schema.content]);

    let query = qp.parse_query("fuck").unwrap();

    let top_docs: Vec<(tantivy::Score, tantivy::DocAddress)> =
        searcher.search(&query, &tantivy::collector::TopDocs::with_limit(10)).unwrap();

    for (score, addr) in top_docs {
        let retr_doc = searcher.doc(addr).unwrap();

        println!("{}: {}", score, doc_indexer.schema.schema.to_json(&retr_doc));
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
