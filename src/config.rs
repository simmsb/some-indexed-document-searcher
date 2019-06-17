// configuration stuff

use config;
use directories::{ProjectDirs, UserDirs};
use serde_derive::{Deserialize, Serialize};
use snafu::{OptionExt, ResultExt, Snafu};
use std::{fs, io::Write, path::PathBuf};
use toml;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Could not locate config directory"))]
    NoConfigDir,
    #[snafu(display("Could not open config from {}: {}", filename.display(), source))]
    ConfigFile {
        filename: PathBuf,
        source: config::ConfigError,
    },
    #[snafu(display("Could not locate data dir {}: {}", dir.display(), source))]
    DataDir {
        dir: PathBuf,
        source: config::ConfigError,
    },
    #[snafu(display("Could not do something with config: {}", source))]
    GeneralConfigError { source: config::ConfigError },
}

type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub index_location: PathBuf,
    pub root_globs: Vec<String>,
    pub indexed_exts: Vec<String>,
    pub ignored_globs: Vec<String>,
}

pub fn load_config() -> Result<Config> {
    let project_dirs =
        ProjectDirs::from("org", "nitros12", "some_document_indexer").context(NoConfigDir)?;

    let mut config = config::Config::new();

    config
        .set_default("index_location", project_dirs.data_dir().to_str())
        .with_context(|| DataDir {
            dir: project_dirs.config_dir(),
        })?;

    let user_dirs = UserDirs::new().expect("Where's your home dir?");

    config
        .set_default("indexed_exts",  vec!["txt", "org", "pdf", "md", "rst"])
        .context(GeneralConfigError)?;
    config
        .set_default("root_globs", vec![user_dirs.home_dir().to_str()])
        .context(GeneralConfigError)?;
    config
        .set_default("ignored_globs", vec![".*"])
        .context(GeneralConfigError)?;

    let config_dir = project_dirs.config_dir().with_extension("toml");

    if !config_dir.exists() {
        let serialized: Config = config.clone().try_into().context(GeneralConfigError)?;
        let stringified = toml::to_string_pretty(&serialized).unwrap();

        fs::create_dir_all(config_dir.parent().unwrap()).unwrap();
        let mut file = fs::File::create(&config_dir).unwrap();

        file.write_all(stringified.as_bytes()).unwrap();
    }

    config
        .merge(config::File::from(config_dir))
        .with_context(|| ConfigFile {
            filename: project_dirs.config_dir(),
        })?;

    config.try_into().context(GeneralConfigError)
}
