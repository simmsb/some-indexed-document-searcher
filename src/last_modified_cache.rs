use sled;
use snafu::{ResultExt, Snafu};

use super::config;

#[derive(Debug, Snafu)]
pub enum Error {
    #[snafu(display("Some error happened with sled: {}", source))]
    SledError { source: sled::Error },
}

type Result<T, E = Error> = std::result::Result<T, E>;

pub enum FileCacheAction {
    /// The file should be evicted from the index and re-indexed
    Outdated,
    /// The file is up to date in the database, do nothing
    UptoDate,
    /// The file is not in the database, should just index
    NotIndexed,
}

pub struct LastModifiedCache {
    db: sled::Db,
}

fn read_ne_u64(inp: &[u8]) -> u64 {
    use std::convert::TryInto;

    let (int_bytes, _) = inp.split_at(std::mem::size_of::<u64>());
    u64::from_ne_bytes(
        int_bytes
            .try_into()
            .expect("Didn't get >=8 bytes to read u64 from (db fuckup)."),
    )
}

impl LastModifiedCache {
    pub fn new(config: &config::Config) -> Result<LastModifiedCache> {
        std::fs::create_dir_all(&config.index_location).unwrap();

        let modified_cache = config.index_location.join("modified_cache");

        let config = sled::ConfigBuilder::default()
            .path(&modified_cache)
            .use_compression(true)
            .build();

        let db = sled::Db::start(config).context(SledError)?;

        Ok(LastModifiedCache { db })
    }

    pub fn len(&self) -> usize {
        self.db.len()
    }

    /// remove a file from the indexed cache if it exists,
    /// returns true if the file was indexed, false otherwise
    pub fn remove_file<P: AsRef<std::path::Path>>(
        &self,
        path: P
    ) -> bool {
        self.db.del(path.as_ref().to_str().unwrap()).unwrap().is_some()
    }

    pub fn check_file<P: AsRef<std::path::Path>>(
        &self,
        path: P,
        modified: u64,
    ) -> Result<FileCacheAction> {
        Ok(
            match self
                .db
                .set(path.as_ref().to_str().unwrap(), &modified.to_ne_bytes())
                .context(SledError)?
            {
                Some(prev_modified_buf) => {
                    let prev_modified = read_ne_u64(prev_modified_buf.as_ref());
                    if modified != prev_modified {
                        FileCacheAction::Outdated
                    } else {
                        FileCacheAction::UptoDate
                    }
                }
                None => FileCacheAction::NotIndexed,
            },
        )
    }
}

impl std::ops::Drop for LastModifiedCache {
    fn drop(&mut self) {
        let _ = self.db.flush();
    }
}
