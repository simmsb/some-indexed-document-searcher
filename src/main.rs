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

    let mut doc_indexer = indexer::DocIndexer::new();
    doc_indexer.create_indexer(&config).context(IndexerError)?;

    for file in file_collector::collect_files(&config).context(CollectorError)? {
        println!("{:?}", file.context(CollectorError));
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
