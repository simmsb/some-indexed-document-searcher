use snafu::{ErrorCompat, ResultExt, Snafu};

mod config;
mod file_collector;

#[derive(Debug, Snafu)]
enum SIDSError {
    #[snafu]
    ConfigLoad { source: config::ConfigError },
    #[snafu]
    CollectorError {
        source: file_collector::FileCollectorError,
    },
}

fn main_inner() -> Result<(), SIDSError> {
    let config = config::load_config().context(ConfigLoad)?;

    println!("config: {:#?}", config);

    for file in file_collector::collect_files(&config).context(CollectorError)? {
        println!("{:?}", file.context(CollectorError));
    }

    Ok(())
}

fn main() {
    match main_inner() {
        Err(e) => {
            eprintln!("Oops: {}", e);
            if let Some(bt) = ErrorCompat::backtrace(&e) {
                eprintln!("{}", bt);
            }
        }
        _ => (),
    }
}
